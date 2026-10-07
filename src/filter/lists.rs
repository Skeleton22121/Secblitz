//! Block-list sources, strict parsing and the per-switch classification.

use super::config::Config;
use super::matcher::{hash, Category, Filter, HashSet64};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Dns,
    WindowsTracking,
    Threats,
    Adult,
    Gambling,
    TrackingClassifier,
    AdClassifier,
}

impl Role {
    pub fn wanted(self, config: &Config) -> bool {
        match self {
            Role::Adult => config.adult,
            Role::Gambling => config.gambling,
            _ => true,
        }
    }
}

pub struct Source {
    pub id: &'static str,
    pub url: &'static str,
    pub max_bytes: u64,
    pub refresh_days: u64,
    pub role: Role,
}

const MIB: u64 = 1024 * 1024;

pub const SOURCES: [Source; 10] = [
    Source {
        id: "adguard-dns",
        url: "https://adguardteam.github.io/HostlistsRegistry/assets/filter_1.txt",
        max_bytes: 16 * MIB,
        refresh_days: 1,
        role: Role::Dns,
    },
    Source {
        id: "hagezi-windows",
        url: "https://adguardteam.github.io/HostlistsRegistry/assets/filter_63.txt",
        max_bytes: 16 * MIB,
        refresh_days: 1,
        role: Role::WindowsTracking,
    },
    Source {
        id: "hagezi-tif",
        url: "https://adguardteam.github.io/HostlistsRegistry/assets/filter_44.txt",
        max_bytes: 128 * MIB,
        refresh_days: 1,
        role: Role::Threats,
    },
    // The medium gambling list is a third of the full size and keeps the well known sites.
    Source {
        id: "hagezi-nsfw",
        url: "https://raw.githubusercontent.com/hagezi/dns-blocklists/main/adblock/nsfw.txt",
        max_bytes: 8 * MIB,
        refresh_days: 7,
        role: Role::Adult,
    },
    Source {
        id: "hagezi-gambling",
        url: "https://raw.githubusercontent.com/hagezi/dns-blocklists/main/adblock/gambling.medium.txt",
        max_bytes: 16 * MIB,
        refresh_days: 7,
        role: Role::Gambling,
    },
    Source {
        id: "adguard-tracking",
        url: "https://filters.adtidy.org/extension/ublock/filters/3.txt",
        max_bytes: 32 * MIB,
        refresh_days: 7,
        role: Role::TrackingClassifier,
    },
    Source {
        id: "easyprivacy",
        url: "https://easylist.to/easylist/easyprivacy.txt",
        max_bytes: 16 * MIB,
        refresh_days: 7,
        role: Role::TrackingClassifier,
    },
    Source {
        id: "adguard-base",
        url: "https://filters.adtidy.org/extension/ublock/filters/2_without_easylist.txt",
        max_bytes: 32 * MIB,
        refresh_days: 7,
        role: Role::AdClassifier,
    },
    Source {
        id: "adguard-mobile",
        url: "https://filters.adtidy.org/extension/ublock/filters/11.txt",
        max_bytes: 16 * MIB,
        refresh_days: 7,
        role: Role::AdClassifier,
    },
    Source {
        id: "easylist",
        url: "https://easylist.to/easylist/easylist.txt",
        max_bytes: 16 * MIB,
        refresh_days: 7,
        role: Role::AdClassifier,
    },
];

pub const NEVER_BLOCK: &[&str] = &[
    "windowsupdate.com",
    "update.microsoft.com",
    "windowsupdate.microsoft.com",
    "delivery.mp.microsoft.com",
    "do.dsp.mp.microsoft.com",
    "emdl.ws.microsoft.com",
    "sls.update.microsoft.com",
    "activation.sls.microsoft.com",
    "validation.sls.microsoft.com",
    "licensing.mp.microsoft.com",
    "msftconnecttest.com",
    "msftncsi.com",
    "wdcp.microsoft.com",
    "wdcpalt.microsoft.com",
    "definitionupdates.microsoft.com",
    "go.microsoft.com",
    // Defender cloud protection also answers on these (HaGeZi's Windows
    // tracker list carries them as telemetry; protection wins).
    "wd.microsoft.com",
    "spynet2.microsoft.com",
    "spynetalt.microsoft.com",
    "smartscreen-prod.microsoft.com",
    "smartscreen.microsoft.com",
    "checkappexec.microsoft.com",
    "urs.microsoft.com",
    "displaycatalog.mp.microsoft.com",
    "storeedgefd.dsx.mp.microsoft.com",
    "purchase.mp.microsoft.com",
    "cdn.winget.microsoft.com",
    "winget.azureedge.net",
    "login.live.com",
    "login.microsoftonline.com",
    "crl.microsoft.com",
    "ocsp.msocsp.com",
    "oneocsp.microsoft.com",
    "time.windows.com",
    "secblitz.lol",
    "beacons.lol",
];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub block: Vec<String>,
    pub allow: Vec<String>,
}

/// A hostname with at least two labels, each 1..=63 characters of
/// `[a-z0-9-_]` not starting or ending with `-`, at most 253 in total.
/// Uppercase is accepted and means the lowercase name.
pub fn valid_hostname(host: &str) -> bool {
    if host.len() > 253 {
        return false;
    }
    let mut labels = 0;
    for label in host.split('.') {
        labels += 1;
        let b = label.as_bytes();
        if b.is_empty() || b.len() > 63 || b[0] == b'-' || b[b.len() - 1] == b'-' {
            return false;
        }
        if !b
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'-' || *c == b'_')
        {
            return false;
        }
    }
    labels >= 2
}

/// One line of a DNS block list: `(is_exception, host)` for `||host^`,
/// `||host^$important` and the `@@` forms of both. Everything else is `None`.
fn parse_line(line: &str) -> Option<(bool, &str)> {
    let line = line.trim();
    let (allow, rest) = match line.strip_prefix("@@") {
        Some(r) => (true, r),
        None => (false, line),
    };
    let (host, after) = rest.strip_prefix("||")?.split_once('^')?;
    if !(after.is_empty() || after == "$important") || !valid_hostname(host) {
        return None;
    }
    Some((allow, host))
}

pub fn parse_blocklist(text: &str) -> Parsed {
    let mut out = Parsed::default();
    for (allow, host) in text.lines().filter_map(parse_line) {
        let host = host.to_ascii_lowercase();
        if allow {
            out.allow.push(host);
        } else {
            out.block.push(host);
        }
    }
    out
}

/// The same lines as `parse_blocklist`, but only the hashes are kept (block,
/// allow), sorted and without duplicates. The threat feed has millions of
/// names; this never holds them as strings.
pub fn parse_blocklist_hashes(text: &str) -> (Vec<u64>, Vec<u64>) {
    let mut block = Vec::new();
    let mut allow = Vec::new();
    for (is_allow, host) in text.lines().filter_map(parse_line) {
        let h = hash(host);
        if is_allow {
            allow.push(h);
        } else {
            block.push(h);
        }
    }
    for v in [&mut block, &mut allow] {
        v.sort_unstable();
        v.dedup();
        v.shrink_to_fit();
    }
    (block, allow)
}

/// Hosts from `||host^`, `||host/` and `||host$` lines of a browser filter
/// list, whatever modifiers follow. Exceptions (`@@`) never count.
pub fn parse_classifier(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("||") else {
            continue;
        };
        let end = rest.find(['^', '/', '$']).unwrap_or(rest.len());
        let host = &rest[..end];
        if valid_hostname(host) {
            out.push(host.to_ascii_lowercase());
        }
    }
    out
}

pub struct Inputs<'a> {
    pub dns: Option<&'a str>,
    pub windows: Option<&'a str>,
    pub threats: Option<&'a str>,
    pub tracking_classifiers: Vec<&'a str>,
    pub ad_classifiers: Vec<&'a str>,
}

fn classifier_set(texts: &[&str]) -> HashSet64 {
    let hosts: Vec<String> = texts.iter().flat_map(|t| parse_classifier(t)).collect();
    HashSet64::from_names(hosts.iter().map(String::as_str))
}

pub fn build(inputs: &Inputs) -> Filter {
    let have_classifier = !inputs.tracking_classifiers.is_empty();
    let t = classifier_set(&inputs.tracking_classifiers);
    let a = classifier_set(&inputs.ad_classifiers);

    let dns = inputs.dns.map(parse_blocklist).unwrap_or_default();
    let mut ads: Vec<&str> = Vec::new();
    let mut tracking: Vec<&str> = Vec::new();
    for d in &dns.block {
        let is_t = !have_classifier || t.any_suffix(d);
        let is_a = !have_classifier || a.any_suffix(d);
        if is_t {
            tracking.push(d);
        }
        if is_a || !is_t {
            ads.push(d);
        }
    }

    let windows = inputs.windows.map(parse_blocklist).unwrap_or_default();
    tracking.extend(windows.block.iter().map(String::as_str));
    let mut tracking_allow: Vec<&str> = dns.allow.iter().map(String::as_str).collect();
    tracking_allow.extend(windows.allow.iter().map(String::as_str));

    let threats = inputs.threats.map(parse_blocklist).unwrap_or_default();

    Filter {
        ads: Category {
            block: HashSet64::from_names(ads),
            allow: HashSet64::from_names(dns.allow.iter().map(String::as_str)),
        },
        tracking: Category {
            block: HashSet64::from_names(tracking),
            allow: HashSet64::from_names(tracking_allow),
        },
        dangerous: Category {
            block: HashSet64::from_names(threats.block.iter().map(String::as_str)),
            allow: HashSet64::from_names(threats.allow.iter().map(String::as_str)),
        },
        never: HashSet64::from_names(NEVER_BLOCK.iter().copied()),
        ..Filter::empty()
    }
}

pub fn counts(filter: &Filter) -> [usize; 5] {
    [
        filter.ads.block.len(),
        filter.tracking.block.len(),
        filter.dangerous.block.len(),
        filter.adult.block.len(),
        filter.gambling.block.len(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::matcher::{hash, Kind, Switches};

    #[test]
    fn hashes_match_parsed_names() {
        let text = "||Ads.Example^\n||b.example^$important\n@@||ok.example^\nexample.com##.x\n||ex*.com^\n||ads.example^\n";
        let (block, allow) = parse_blocklist_hashes(text);
        let parsed = parse_blocklist(text);
        let expect = |names: &[String]| {
            let mut v: Vec<u64> = names.iter().map(|n| hash(n)).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        assert_eq!(block, expect(&parsed.block));
        assert_eq!(allow, expect(&parsed.allow));
        assert_eq!(block.len(), 2);
    }

    #[test]
    fn parse_accepts_plain_and_important() {
        let p = parse_blocklist("||Example.com^\n||ads.example.org^$important\n  ||x.net^  \r\n");
        assert_eq!(p.block, ["example.com", "ads.example.org", "x.net"]);
        assert!(p.allow.is_empty());
    }

    #[test]
    fn parse_reads_exceptions() {
        let p = parse_blocklist("@@||ok.example.com^\n@@||ok2.example.com^$important\n||a.com^");
        assert_eq!(p.allow, ["ok.example.com", "ok2.example.com"]);
        assert_eq!(p.block, ["a.com"]);
    }

    #[test]
    fn parse_ignores_cosmetic_and_regex_rules() {
        let text = "! comment\n# comment\nexample.com##.ad\n/ads?/\n||ex*.com^\n\
                    ||example.com^$third-party\n||example.com/path\n||example.com\n\
                    @@||example.com^$script\n||localhost^\n||a.com^|\n0.0.0.0 hosts.example\n";
        assert_eq!(parse_blocklist(text), Parsed::default());
    }

    #[test]
    fn hostname_validation() {
        assert!(valid_hostname("example.com"));
        assert!(valid_hostname("a-b.c_d.example.com"));
        assert!(valid_hostname("EXAMPLE.com"));
        assert!(!valid_hostname("com"));
        assert!(!valid_hostname(""));
        assert!(!valid_hostname("-a.com"));
        assert!(!valid_hostname("a-.com"));
        assert!(!valid_hostname("a..com"));
        assert!(!valid_hostname("a.com."));
        assert!(!valid_hostname("a b.com"));
        assert!(!valid_hostname("ex*.com"));
        assert!(!valid_hostname("exämple.com"));
        assert!(valid_hostname(&format!("{}.com", "a".repeat(63))));
        assert!(!valid_hostname(&format!("{}.com", "a".repeat(64))));
        let long = format!("{0}.{0}.{0}.{0}.com", "a".repeat(62));
        assert!(long.len() > 253 && !valid_hostname(&long));
    }

    #[test]
    fn classifier_reads_hosts_with_modifiers() {
        let text = "||a.com^$third-party\n||b.com/path\n||c.com$script\n||d.com^\n@@||e.com^\n\
                    ##.x\n||f*.com^\n";
        assert_eq!(parse_classifier(text), ["a.com", "b.com", "c.com", "d.com"]);
    }

    fn contains(c: &Category, name: &str) -> bool {
        c.block.contains(hash(name))
    }

    #[test]
    fn build_classifies_overlap_into_both() {
        let dns = "||doubleclick.net^\n||adservice.example^\n||hotjar.com^\n";
        let f = build(&Inputs {
            dns: Some(dns),
            windows: None,
            threats: None,
            tracking_classifiers: vec!["||doubleclick.net^\n||hotjar.com^\n"],
            ad_classifiers: vec!["||doubleclick.net^\n"],
        });
        assert_eq!(f.ads.block.len(), 2);
        assert!(contains(&f.ads, "doubleclick.net"));
        assert!(contains(&f.ads, "adservice.example"));
        assert_eq!(f.tracking.block.len(), 2);
        assert!(contains(&f.tracking, "doubleclick.net"));
        assert!(contains(&f.tracking, "hotjar.com"));
        assert_eq!(counts(&f), [2, 2, 0, 0, 0]);
    }

    #[test]
    fn build_matches_classifier_on_parent_domains() {
        let f = build(&Inputs {
            dns: Some("||x.hotjar.com^\n"),
            windows: None,
            threats: None,
            tracking_classifiers: vec!["||hotjar.com^\n"],
            ad_classifiers: vec![],
        });
        assert_eq!(f.ads.block.len(), 0);
        assert_eq!(f.tracking.block.len(), 1);
    }

    #[test]
    fn build_without_classifiers_puts_everything_in_both() {
        let f = build(&Inputs {
            dns: Some("||a.com^\n||b.com^\n"),
            windows: None,
            threats: None,
            tracking_classifiers: vec![],
            ad_classifiers: vec![],
        });
        assert_eq!(counts(&f), [2, 2, 0, 0, 0]);
    }

    #[test]
    fn windows_list_goes_to_tracking() {
        let f = build(&Inputs {
            dns: None,
            windows: Some("||telemetry.example^\n@@||ok.telemetry.example^\n"),
            threats: Some("||evil.example^\n"),
            tracking_classifiers: vec![],
            ad_classifiers: vec![],
        });
        assert_eq!(counts(&f), [0, 1, 1, 0, 0]);
        let on = Switches {
            tracking: true,
            dangerous: true,
            ..Switches::default()
        };
        assert_eq!(f.decide("x.telemetry.example", on), Some(Kind::Tracking));
        assert_eq!(f.decide("ok.telemetry.example", on), None);
        assert_eq!(f.decide("evil.example", on), Some(Kind::Dangerous));
        assert!(f.never.any_suffix("dl.delivery.mp.microsoft.com"));
    }

    #[test]
    fn sources_are_https_and_unique() {
        let mut ids: Vec<_> = SOURCES.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), SOURCES.len());
        let mut urls: Vec<_> = SOURCES.iter().map(|s| s.url).collect();
        urls.sort_unstable();
        urls.dedup();
        assert_eq!(urls.len(), SOURCES.len());
        for s in &SOURCES {
            assert!(s.url.starts_with("https://"), "{}", s.id);
            assert!(s.max_bytes > 0 && s.refresh_days > 0);
            let weekly = matches!(s.role, Role::TrackingClassifier | Role::AdClassifier);
            let weekly = weekly || matches!(s.role, Role::Adult | Role::Gambling);
            assert_eq!(s.refresh_days, if weekly { 7 } else { 1 }, "{}", s.id);
        }
        let tif = SOURCES.iter().find(|s| s.id == "hagezi-tif").unwrap();
        assert_eq!(tif.max_bytes, 128 * MIB);
    }

    #[test]
    fn family_lists_are_only_wanted_when_their_switch_is_on() {
        let off = Config::default();
        let adult = Config {
            adult: true,
            ..Config::default()
        };
        let gambling = Config {
            gambling: true,
            ..Config::default()
        };
        assert!(Role::Dns.wanted(&off) && Role::Threats.wanted(&off));
        assert!(!Role::Adult.wanted(&off) && !Role::Gambling.wanted(&off));
        assert!(Role::Adult.wanted(&adult) && !Role::Gambling.wanted(&adult));
        assert!(Role::Gambling.wanted(&gambling) && !Role::Adult.wanted(&gambling));
        let caps: Vec<_> = SOURCES
            .iter()
            .filter(|s| matches!(s.role, Role::Adult | Role::Gambling))
            .map(|s| (s.id, s.max_bytes))
            .collect();
        assert_eq!(
            caps,
            [("hagezi-nsfw", 8 * MIB), ("hagezi-gambling", 16 * MIB)]
        );
    }

    #[test]
    fn never_block_names_are_valid() {
        for n in NEVER_BLOCK {
            assert!(valid_hostname(n), "{n}");
        }
    }

    #[test]
    fn defender_cloud_protection_is_never_blocked() {
        // HaGeZi's Windows tracker list carries these; they must still resolve.
        let windows = "||spynet2.microsoft.com^\n||spynetalt.microsoft.com^\n||wdcp.microsoft.com^\n||unitedstates.cp.wd.microsoft.com^\n||telemetry.example^\n";
        let filter = build(&Inputs {
            dns: None,
            windows: Some(windows),
            threats: None,
            tracking_classifiers: Vec::new(),
            ad_classifiers: Vec::new(),
        });
        let on = Switches {
            ads: true,
            tracking: true,
            dangerous: true,
            ..Switches::default()
        };
        for name in [
            "spynet2.microsoft.com",
            "spynetalt.microsoft.com",
            "wdcp.microsoft.com",
            "unitedstates.cp.wd.microsoft.com",
        ] {
            assert_eq!(filter.decide(name, on), None, "{name}");
        }
        assert_eq!(filter.decide("telemetry.example", on), Some(Kind::Tracking));
    }
}
