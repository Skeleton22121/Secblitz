//! The web protection service reads untrusted network packets and files; these
//! checks go through the public filter API only.
use secblitz::filter::config::{
    fresh, load_config, load_status, save_config, save_status, Config, ErrorCode, State, Status,
};
use secblitz::filter::dns::{
    blocked_reply, nxdomain_reply, parse_query, reply_matches, servfail_reply, truncated, with_id,
};
use secblitz::filter::lists::{build, parse_blocklist, parse_classifier, valid_hostname, Inputs};
use secblitz::filter::matcher::{Filter, Kind, Switches};
use std::fs;
use std::path::PathBuf;

fn packet(name: &str, qtype: u16) -> Vec<u8> {
    let mut p = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in name.split('.').filter(|l| !l.is_empty()) {
        p.push(label.len() as u8);
        p.extend(label.as_bytes());
    }
    p.push(0);
    p.extend(qtype.to_be_bytes());
    p.extend(1u16.to_be_bytes());
    p
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("secblitz-filter-boundary-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn a_normal_lookup_is_read_with_its_name_lowercased() {
    let q = parse_query(&packet("Ads.Example.COM", 1)).unwrap();
    assert_eq!(q.id, 0x1234);
    assert_eq!(q.question.name, "ads.example.com");
    assert_eq!(q.question.qtype, 1);
}

#[test]
fn packets_shorter_than_a_header_are_refused() {
    for len in 0..12 {
        assert!(parse_query(&vec![0u8; len]).is_none());
    }
}

#[test]
fn a_packet_cut_off_inside_the_question_is_refused() {
    let full = packet("example.com", 1);
    for cut in 12..full.len() {
        assert!(parse_query(&full[..cut]).is_none(), "cut at {cut}");
    }
}

#[test]
fn replies_are_never_accepted_as_questions() {
    let mut p = packet("example.com", 1);
    p[2] |= 0x80;
    assert!(parse_query(&p).is_none());
}

#[test]
fn lookups_with_an_unusual_opcode_are_refused() {
    let mut p = packet("example.com", 1);
    p[2] |= 0x08;
    assert!(parse_query(&p).is_none());
}

#[test]
fn lookups_that_carry_extra_sections_or_several_questions_are_refused() {
    for slot in [4usize, 6, 8] {
        let mut p = packet("example.com", 1);
        p[slot + 1] = 2;
        assert!(parse_query(&p).is_none(), "count at {slot}");
    }
}

#[test]
fn compression_pointers_in_a_question_are_refused() {
    let mut p = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0, 0xC0, 0x0C];
    p.extend([0, 1, 0, 1]);
    assert!(parse_query(&p).is_none());
}

#[test]
fn names_longer_than_the_wire_limit_are_refused() {
    let label = "a".repeat(63);
    let name = vec![label; 5].join(".");
    assert!(parse_query(&packet(&name, 1)).is_none());
}

#[test]
fn oversized_packets_are_refused() {
    let mut p = packet("example.com", 1);
    p.resize(5000, 0);
    assert!(parse_query(&p).is_none());
}

#[test]
fn a_blocked_address_lookup_gets_a_zero_address_answer() {
    let raw = packet("ads.example", 1);
    let q = parse_query(&raw).unwrap();
    let reply = blocked_reply(&raw, &q);
    assert_eq!(&reply[..2], &raw[..2]);
    assert_eq!(reply[2] & 0x80, 0x80);
    assert_eq!(&reply[6..8], &[0, 1]);
    assert_eq!(&reply[reply.len() - 4..], &[0, 0, 0, 0]);
    assert!(reply_matches(&reply, q.id, &q.question));
}

#[test]
fn a_blocked_lookup_of_another_kind_gets_an_empty_answer() {
    let raw = packet("ads.example", 16);
    let q = parse_query(&raw).unwrap();
    let reply = blocked_reply(&raw, &q);
    assert_eq!(&reply[6..8], &[0, 0]);
    assert_eq!(reply[3] & 0x0F, 0);
}

#[test]
fn the_not_found_and_failure_answers_carry_their_codes() {
    let raw = packet("example.com", 1);
    let q = parse_query(&raw).unwrap();
    assert_eq!(nxdomain_reply(&raw, &q)[3] & 0x0F, 3);
    assert_eq!(servfail_reply(&raw, &q)[3] & 0x0F, 2);
}

#[test]
fn an_answer_for_a_different_name_or_id_is_not_accepted() {
    let raw = packet("example.com", 1);
    let q = parse_query(&raw).unwrap();
    let reply = blocked_reply(&raw, &q);
    assert!(!reply_matches(&reply, q.id ^ 1, &q.question));
    let other = parse_query(&packet("other.com", 1)).unwrap();
    assert!(!reply_matches(&reply, q.id, &other.question));
    assert!(!reply_matches(&reply[..8], q.id, &q.question));
    assert!(!reply_matches(&raw, q.id, &q.question));
}

#[test]
fn rewriting_the_id_changes_only_the_first_two_bytes() {
    let raw = packet("example.com", 1);
    let out = with_id(&raw, 0xBEEF);
    assert_eq!(&out[..2], &[0xBE, 0xEF]);
    assert_eq!(&out[2..], &raw[2..]);
    assert_eq!(with_id(&[7], 1), vec![7]);
}

#[test]
fn only_a_marked_answer_counts_as_cut_short() {
    assert!(!truncated(&[]));
    assert!(!truncated(&[0, 0, 0x80, 0]));
    assert!(truncated(&[0, 0, 0x82, 0]));
}

#[test]
fn hostnames_must_be_well_formed_to_be_listed() {
    for good in [
        "example.com",
        "a-b.example.co",
        "x_y.example",
        "EXAMPLE.com",
    ] {
        assert!(valid_hostname(good), "{good}");
    }
    let long_label = format!("{}.com", "a".repeat(64));
    let long_name = format!("{}.com", vec!["a".repeat(60); 5].join("."));
    for bad in [
        "",
        "localhost",
        "-a.example.com",
        "a-.example.com",
        "a..com",
        "ex*.com",
        "exa mple.com",
        "1.2.3.4/x",
        long_label.as_str(),
        long_name.as_str(),
    ] {
        assert!(!valid_hostname(bad), "{bad}");
    }
}

#[test]
fn block_list_lines_that_are_not_plain_rules_are_ignored() {
    let text = "! comment\n||ads.example^\n||Dup.Example^$important\n@@||ok.example^\n||path.example/x^\n||bad^\nexample.com##.banner\n||x.example^$third-party\n||*.example^\n";
    let parsed = parse_blocklist(text);
    assert_eq!(parsed.block, vec!["ads.example", "dup.example"]);
    assert_eq!(parsed.allow, vec!["ok.example"]);
}

#[test]
fn filter_list_hosts_are_read_whatever_modifiers_follow() {
    let hosts = parse_classifier(
        "||a.example^$third-party\n||b.example/path\n@@||c.example^\n||d.example$script\n||nodots^\n",
    );
    assert_eq!(hosts, vec!["a.example", "b.example", "d.example"]);
}

fn filter() -> Filter {
    build(&Inputs {
        dns: Some("||ads.example^\n||both.example^\n@@||fine.ads.example^\n"),
        windows: Some("||telemetry.example^\n"),
        threats: Some("||evil.example^\n||both.example^\n"),
        tracking_classifiers: vec![],
        ad_classifiers: vec![],
    })
}

fn all() -> Switches {
    Switches {
        ads: true,
        tracking: true,
        dangerous: true,
    }
}

#[test]
fn a_blocked_name_and_its_subdomains_are_stopped_when_the_switch_is_on() {
    let f = filter();
    assert_eq!(f.decide("evil.example", all()), Some(Kind::Dangerous));
    assert_eq!(
        f.decide("deep.sub.evil.example", all()),
        Some(Kind::Dangerous)
    );
    assert_eq!(f.decide("telemetry.example", all()), Some(Kind::Tracking));
    assert_eq!(f.decide("unrelated.example", all()), None);
}

#[test]
fn a_blocked_name_passes_when_its_switch_is_off() {
    let f = filter();
    let none = Switches::default();
    assert_eq!(f.decide("evil.example", none), None);
    let ads_only = Switches { ads: true, ..none };
    assert_eq!(f.decide("evil.example", ads_only), None);
}

#[test]
fn a_name_on_several_lists_is_reported_as_the_most_serious_kind() {
    assert_eq!(
        filter().decide("both.example", all()),
        Some(Kind::Dangerous)
    );
}

#[test]
fn an_exception_lets_one_name_through_while_its_parent_stays_blocked() {
    let f = filter();
    assert_eq!(f.decide("fine.ads.example", all()), None);
    assert_eq!(f.decide("ads.example", all()), Some(Kind::Ads));
}

#[test]
fn names_the_service_needs_are_never_blocked_even_if_listed() {
    let f = build(&Inputs {
        dns: None,
        windows: None,
        threats: Some("||secblitz.lol^\n||sub.secblitz.lol^\n"),
        tracking_classifiers: vec![],
        ad_classifiers: vec![],
    });
    assert_eq!(f.decide("secblitz.lol", all()), None);
    assert_eq!(f.decide("sub.secblitz.lol", all()), None);
}

#[test]
fn an_empty_filter_blocks_nothing() {
    assert_eq!(Filter::empty().decide("evil.example", all()), None);
}

#[test]
fn a_pause_turns_every_switch_off_until_it_ends() {
    let c = Config {
        ads: true,
        tracking: true,
        dangerous: true,
        paused_until: Some(100),
    };
    assert!(c.paused(99));
    assert_eq!(c.active(99), Switches::default());
    assert!(!c.paused(100));
    assert_eq!(c.active(100), all());
}

#[test]
fn a_missing_config_means_everything_is_off() {
    assert!(!load_config(&temp("does-not-exist.json")).any_on());
}

#[test]
fn a_damaged_config_means_everything_is_off() {
    let path = temp("damaged.json");
    for bytes in [
        &b"{\"ads\":tr"[..],
        b"",
        b"[]",
        b"{\"ads\":\"yes\"}",
        &[0xFF, 0xFE, 0x00],
    ] {
        fs::write(&path, bytes).unwrap();
        assert_eq!(load_config(&path), Config::default());
    }
}

#[test]
fn a_config_larger_than_the_limit_is_ignored() {
    let path = temp("huge.json");
    let mut text = String::from("{\"ads\":true,\"tracking\":true,\"dangerous\":true,\"pad\":\"");
    text.push_str(&"x".repeat(8 * 1024));
    text.push_str("\"}");
    fs::write(&path, text).unwrap();
    assert!(!load_config(&path).any_on());
}

#[test]
fn a_saved_config_loads_back_unchanged() {
    let path = temp("roundtrip.json");
    let c = Config {
        ads: true,
        tracking: false,
        dangerous: true,
        paused_until: Some(42),
    };
    save_config(&path, &c).unwrap();
    assert_eq!(load_config(&path), c);
}

#[test]
fn an_older_config_without_a_pause_field_still_loads() {
    let path = temp("older.json");
    fs::write(&path, br#"{"ads":true,"tracking":false,"dangerous":false}"#).unwrap();
    let c = load_config(&path);
    assert!(c.ads);
    assert_eq!(c.paused_until, None);
}

#[test]
fn a_status_file_is_written_in_the_shape_the_app_reads() {
    let path = temp("status.json");
    let s = Status {
        listening: true,
        state: State::NoLists,
        lists_updated: Some(5),
        day: 9,
        blocked: [1, 2, 3],
        domains: [4, 5, 6],
        last_error: Some(ErrorCode::PortInUse),
        written_at: 77,
    };
    save_status(&path, &s).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"no-lists\""));
    assert!(text.contains("\"port-in-use\""));
    assert_eq!(load_status(&path), Some(s));
}

#[test]
fn a_damaged_or_unknown_status_is_not_trusted() {
    let path = temp("bad-status.json");
    assert_eq!(load_status(&temp("absent-status.json")), None);
    for bytes in [
        &b"{"[..],
        br#"{"listening":true,"state":"exploding","day":1,"blocked":[0,0,0],"domains":[0,0,0],"written_at":1,"lists_updated":null,"last_error":null}"#,
        br#"{"listening":true,"state":"ready","day":1,"blocked":[0,0],"domains":[0,0,0],"written_at":1,"lists_updated":null,"last_error":null}"#,
    ] {
        fs::write(&path, bytes).unwrap();
        assert_eq!(load_status(&path), None);
    }
}

#[test]
fn a_status_counts_as_alive_only_when_recent_and_not_from_the_future() {
    let s = Status {
        written_at: 1000,
        ..Status::default()
    };
    assert!(fresh(&s, 1000));
    assert!(fresh(&s, 1120));
    assert!(!fresh(&s, 1121));
    assert!(!fresh(&s, 999));
}
