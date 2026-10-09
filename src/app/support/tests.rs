use super::*;
use secblitz::debloat::{Batch, Removed};
use secblitz::filter::config::{AllowOnce, DayCount, ErrorCode, State as WebState};
use secblitz::updater::UpdateOutcome;

const USER_PATH: &str = r"C:\Users\Maria Lopez\AppData\Local\Secblitz\x.json";
const SID: &str = "S-1-5-21-1004336348-1177238915-682003330-1001";
const SITE: &str = "shop.private-example.com";

fn redactor() -> Redactor {
    Redactor::new("Maria Lopez", "MARIAS-PC")
}

fn facts() -> Facts {
    let mut config = Config {
        ads: true,
        dangerous: true,
        allow: vec![SITE.into(), "other.example.org".into()],
        allow_once: vec![AllowOnce {
            site: "once.example.net".into(),
            until: 99,
        }],
        ..Config::default()
    };
    config.scam = true;
    Facts {
        at: 1_791_556_200,
        version: "1.0.0".into(),
        windows: "Windows 11 Pro, version 24H2, build 26100, processor AMD64".into(),
        language: "en".into(),
        theme: "light".into(),
        installed: true,
        background: Some(true),
        tray: false,
        checked_at: Some(1_791_556_000),
        checks: vec![
            ("defender.realtime".into(), "compliant".into()),
            ("firewall.public.inbound".into(), "attention".into()),
            ("privacy.wifi_random_address".into(), "compliant".into()),
        ],
        history: vec![
            Entry {
                t: 1_791_000_000,
                kind: HistoryKind::Check,
                protected: 10,
                total: 20,
                n: 0,
            },
            Entry {
                t: 1_791_500_000,
                kind: HistoryKind::Fix,
                protected: 12,
                total: 20,
                n: 2,
            },
        ],
        changes: Changes::Read(ChangeSummary {
            sets: 3,
            applied: 2,
            pending: 0,
            reverted: 1,
            checks: vec!["firewall.public.inbound".into()],
        }),
        apps: Some(vec![
            AppFact {
                family: "Microsoft.BingNews_8wekyb3d8bbwe".into(),
                state: AppState::Removed { copy_kept: true },
            },
            AppFact {
                family: "Microsoft.549981C3F5F10_8wekyb3d8bbwe".into(),
                state: AppState::BroughtBack,
            },
        ]),
        web: Some(WebFacts {
            config,
            status: Some(Status {
                listening: true,
                state: WebState::Ready,
                lists_updated: Some(1_791_000_000),
                last_error: Some(ErrorCode::DownloadFailed),
                gaps: Some(vec![secblitz::filter::gaps::Gap::Vpn]),
                notice: Some(secblitz::filter::config::Notice {
                    kind: SiteKind::Scam,
                    site: SITE.into(),
                    at: 1,
                }),
                ..Status::default()
            }),
            lists: vec![
                ("adguard-dns".into(), Some(1_791_000_000)),
                ("x".into(), None),
            ],
            stats: Some(BlockHistory {
                days: vec![DayCount {
                    day: 20_000,
                    blocked: [5, 4, 3, 2, 1, 0, 0],
                }],
                top: vec![secblitz::filter::config::TopSite {
                    site: SITE.into(),
                    count: 9,
                }],
                top_companies: Vec::new(),
            }),
        }),
        update: Some(UpdateStatus {
            checked_at: 1_791_000_000,
            result: UpdateOutcome::Failed {
                reason: format!("Could not reach {SITE} for MARIAS-PC"),
            },
        }),
        services: vec![
            ("SecblitzMonitor".into(), "Running".into()),
            ("SecblitzFilter".into(), "Stopped".into()),
        ],
        problems: vec![format!(
            "Could not open {USER_PATH} for {SID} on MARIAS-PC (maria lopez)"
        )],
    }
}

fn all_text(files: &[(String, Vec<u8>)]) -> String {
    files
        .iter()
        .map(|(_, d)| String::from_utf8(d.clone()).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn no_personal_text_reaches_any_file() {
    let text = all_text(&files(&facts(), &redactor()));
    for secret in [
        "Maria",
        "maria",
        "Lopez",
        "MARIAS-PC",
        "S-1-5-21",
        "1004336348",
        "private-example",
        "example.org",
        "example.net",
        r"C:\Users\Maria",
    ] {
        assert!(!text.contains(secret), "{secret} leaked:\n{text}");
    }
    assert!(text.contains(r"C:\Users\[user]\AppData"));
    assert!(text.contains("[sid]"));
}

#[test]
fn the_last_problem_is_cleaned_not_dropped() {
    let files = files(&facts(), &redactor());
    let problem = String::from_utf8(
        files
            .iter()
            .find(|(n, _)| n == "last-problem.txt")
            .unwrap()
            .1
            .clone(),
    )
    .unwrap();
    assert_eq!(
        problem,
        "Could not open C:\\Users\\[user]\\AppData\\Local\\Secblitz\\x.json for [sid] on [computer] ([user])\n"
    );
}

#[test]
fn the_update_failure_text_hides_site_names() {
    let files = files(&facts(), &redactor());
    let updates = String::from_utf8(
        files
            .iter()
            .find(|(n, _)| n == "updates.txt")
            .unwrap()
            .1
            .clone(),
    )
    .unwrap();
    assert!(
        updates.contains("Could not reach [site] for [computer]"),
        "{updates}"
    );
}

#[test]
fn the_web_file_has_counts_not_names() {
    let files = files(&facts(), &redactor());
    let web = String::from_utf8(
        files
            .iter()
            .find(|(n, _)| n == "web-protection.txt")
            .unwrap()
            .1
            .clone(),
    )
    .unwrap();
    assert!(web.contains("ads: on"));
    assert!(web.contains("adult sites: off"));
    assert!(web.contains("Sites always allowed: 2"));
    assert!(web.contains("Sites allowed for a few minutes: 1"));
    assert!(web.contains("Vpn"));
    assert!(web.contains("DownloadFailed"));
    assert!(web.contains("adguard-dns"));
    assert!(web.contains("not downloaded"));
    assert!(web.contains("Ads 5, Tracking 4, Dangerous 3, Adult 2, Gambling 1"));
}

#[test]
fn every_planned_file_is_in_the_archive_and_reads_back() {
    let bytes = archive(&facts(), &redactor()).unwrap();
    let read = zip::read(&bytes).unwrap();
    let names: Vec<_> = read.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "about.txt",
            "last-check.json",
            "history.txt",
            "changes.txt",
            "apps.txt",
            "web-protection.txt",
            "updates.txt",
            "services.txt",
            "last-problem.txt"
        ]
    );
    assert!(bytes.len() < zip::MAX_TOTAL);
}

#[test]
fn the_check_file_keeps_ids_and_statuses_only() {
    let text = last_check(&facts());
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["counts"]["compliant"], 2);
    assert_eq!(value["counts"]["attention"], 1);
    assert_eq!(value["checks"][1]["id"], "firewall.public.inbound");
    assert_eq!(value["checks"][1]["status"], "attention");
    assert_eq!(value["checks"][1].as_object().unwrap().len(), 2);
    assert_eq!(value["checked_at"], "2026-10-09 14:26");
}

#[test]
fn about_names_the_version_and_how_it_runs() {
    let text = about(&facts());
    assert!(text.contains("Secblitz: 1.0.0 (installed)"));
    assert!(text.contains("Windows: Windows 11 Pro, version 24H2, build 26100"));
    assert!(text.contains("Background checks: on"));
    assert!(text.contains("System tray icon: off"));
    let mut f = facts();
    f.installed = false;
    f.background = None;
    let text = about(&f);
    assert!(text.contains("(portable)"));
    assert!(text.contains("Background checks: unknown"));
}

#[test]
fn history_changes_and_apps_say_what_they_hold() {
    let f = facts();
    let history = history_text(&f);
    assert!(history.contains("Recorded entries: 2"));
    assert!(history.contains("fix: 1"));
    assert!(history.contains("12 of 20 protected"));
    let changes = changes_text(&f);
    assert!(changes.contains("History damaged: no"));
    assert!(changes.contains("Applied: 2"));
    assert!(changes.contains("  firewall.public.inbound"));
    let apps = apps_text(&f);
    assert!(apps.contains("Apps Secblitz has removed: 2"));
    assert!(apps.contains("Microsoft.BingNews_8wekyb3d8bbwe: removed, copy kept"));
    assert!(apps.contains("brought back"));
}

#[test]
fn unreadable_and_damaged_sources_say_so() {
    let mut f = facts();
    f.changes = Changes::Damaged;
    assert!(changes_text(&f).contains("History damaged: yes"));
    f.changes = Changes::Unreadable;
    assert!(changes_text(&f).contains("could not be read"));
    f.apps = None;
    f.web = None;
    f.update = None;
    f.services.clear();
    f.problems.clear();
    assert!(apps_text(&f).contains("could not be read"));
    assert!(web_text(&f).contains("could not be read"));
    assert!(updates_text(&f).contains("could not be read"));
    assert!(services_text(&f).contains("could not be read"));
    assert_eq!(problem_text(&f), "No problem is recorded.\n");
}

#[test]
fn removed_apps_are_listed_once_with_their_latest_state() {
    let removed = |index, restored| Removed {
        index,
        package: format!("Package.Full.Name{index}_1.0_x64__abc"),
        version: "1.0".into(),
        restored,
    };
    let batches = vec![
        Batch {
            t: 1,
            removed: vec![removed(3, false), removed(5, false)],
            ..Batch::default()
        },
        Batch {
            t: 2,
            removed: vec![removed(3, true), removed(9, false)],
            ..Batch::default()
        },
    ];
    let apps = apps_from(
        &batches,
        |i| (i != 9).then(|| format!("Family{i}")),
        |i| i == 5,
    );
    assert_eq!(
        apps,
        vec![
            AppFact {
                family: "Family3".into(),
                state: AppState::BroughtBack
            },
            AppFact {
                family: "Family5".into(),
                state: AppState::Removed { copy_kept: true }
            },
        ]
    );
}

#[test]
fn a_long_file_is_cut_at_a_character_boundary() {
    let long = "é".repeat(MAX_FILE);
    let bytes = capped(long);
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.len() <= MAX_FILE + 32);
    assert!(text.ends_with("(cut: too long)\n"));
    assert_eq!(capped("short".into()), b"short");
}

#[test]
fn the_file_name_has_the_local_date_and_minute() {
    assert_eq!(
        file_name(1_791_556_200),
        "Secblitz-support-2026-10-09-1430.zip"
    );
    assert_eq!(file_name(0), "Secblitz-support-1970-01-01-0000.zip");
}

#[test]
fn a_taken_name_gets_a_number_and_nothing_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let name = "Secblitz-support-2026-10-09-1010.zip";
    let first = save_new(dir.path(), name, b"one").unwrap();
    let second = save_new(dir.path(), name, b"two").unwrap();
    let third = save_new(dir.path(), name, b"three").unwrap();
    assert_eq!(first.file_name().unwrap(), name);
    assert_eq!(
        second.file_name().unwrap(),
        "Secblitz-support-2026-10-09-1010-2.zip"
    );
    assert_eq!(
        third.file_name().unwrap(),
        "Secblitz-support-2026-10-09-1010-3.zip"
    );
    assert_eq!(std::fs::read(first).unwrap(), b"one");
    assert_eq!(std::fs::read(second).unwrap(), b"two");
}

#[cfg(unix)]
#[test]
fn a_link_with_the_same_name_is_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.txt");
    std::fs::write(&target, b"keep").unwrap();
    let name = "Secblitz-support-2026-10-09-1010.zip";
    std::os::unix::fs::symlink(&target, dir.path().join(name)).unwrap();
    let saved = save_new(dir.path(), name, b"new").unwrap();
    assert_eq!(std::fs::read(target).unwrap(), b"keep");
    assert_eq!(
        saved.file_name().unwrap(),
        "Secblitz-support-2026-10-09-1010-2.zip"
    );
}

#[test]
fn windows_11_is_named_by_its_build_number() {
    assert_eq!(
        windows_line(
            Some("Windows 10 Pro"),
            Some("24H2"),
            Some(26100),
            Some("AMD64"),
            false
        ),
        "Windows 11 Pro, version 24H2, build 26100, processor AMD64"
    );
    assert_eq!(
        windows_line(
            Some("Windows 10 Enterprise"),
            Some("22H2"),
            Some(19045),
            Some("AMD64"),
            false
        ),
        "Windows 10 Enterprise, version 22H2, build 19045, processor AMD64"
    );
    assert_eq!(
        windows_line(Some("Windows 11 Pro"), None, None, None, true),
        "Windows 11 Pro, ARM processor, running the x64 build"
    );
    assert_eq!(
        windows_line(None, None, None, None, false),
        "Windows (unknown edition), processor unknown"
    );
}

#[test]
fn the_newest_support_file_is_found_and_other_files_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(newest_file(dir.path()), None);
    std::fs::write(dir.path().join("holiday.zip"), b"x").unwrap();
    std::fs::write(dir.path().join("Secblitz-support-notes.txt"), b"x").unwrap();
    assert_eq!(newest_file(dir.path()), None);
    let old = save_new(dir.path(), "Secblitz-support-2026-10-01-0900.zip", b"old").unwrap();
    let past = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(past)
        .unwrap();
    let new = save_new(dir.path(), "Secblitz-support-2026-10-09-1200.zip", b"new").unwrap();
    assert_eq!(newest_file(dir.path()), Some(new));
}
