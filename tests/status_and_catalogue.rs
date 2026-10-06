//! The tray status file and the control catalogue, through the public API.
use secblitz::status::{self, Item, State, Status};
use std::collections::HashSet;

fn items(states: &[Item]) -> Vec<(String, Item)> {
    states
        .iter()
        .enumerate()
        .map(|(n, item)| (format!("check.{n}"), *item))
        .collect()
}

#[test]
fn a_summary_survives_the_status_file() {
    let dir = tempfile::tempdir().unwrap();
    let summary = status::summarize(
        &items(&[Item::Protected, Item::Attention, Item::Protected]),
        true,
        1_000,
    );
    assert_eq!(summary.state, State::Attention);
    assert_eq!(summary.attention, ["check.1"]);

    status::write_to(dir.path(), &summary).unwrap();
    let bytes = std::fs::read(dir.path().join("status.json")).unwrap();
    assert!(bytes.len() <= status::LIMIT);
    assert_eq!(Status::parse(&bytes).unwrap(), summary);
}

#[test]
fn only_a_complete_clean_scan_reports_ok() {
    let all_good = items(&[Item::Protected, Item::Protected]);
    assert_eq!(status::summarize(&all_good, true, 1).state, State::Ok);
    assert_eq!(status::summarize(&all_good, false, 1).state, State::Unknown);
    assert_eq!(status::summarize(&[], true, 1).state, State::Unknown);
    assert_eq!(
        status::summarize(&items(&[Item::Unknown, Item::Protected]), true, 1).state,
        State::Unknown
    );
}

#[test]
fn stale_or_forged_status_never_reads_as_protected() {
    let ok = status::summarize(&items(&[Item::Protected]), true, 10_000);
    assert_eq!(ok.clone().fresh(10_000).state, State::Ok);
    assert_eq!(
        ok.clone().fresh(10_000 + status::MAX_AGE + 1).state,
        State::Unknown
    );
    let from_the_future = status::summarize(&items(&[Item::Protected]), true, 10_000);
    assert_eq!(
        from_the_future.fresh(10_000 - status::MAX_FUTURE - 1).state,
        State::Unknown
    );
}

#[test]
fn malformed_status_is_refused() {
    let good = serde_json::to_vec(&status::summarize(&items(&[Item::Protected]), true, 5)).unwrap();
    assert!(Status::parse(&good).is_ok());
    assert!(Status::parse(b"{not json").is_err());
    assert!(Status::parse(&vec![b' '; status::LIMIT + 1]).is_err());

    let mut forged: serde_json::Value = serde_json::from_slice(&good).unwrap();
    forged["protected"] = 99.into();
    assert!(Status::parse(forged.to_string().as_bytes()).is_err());
    forged["protected"] = 1.into();
    forged["attention"] = serde_json::json!(["bad id with spaces"]);
    assert!(Status::parse(forged.to_string().as_bytes()).is_err());
    forged["attention"] = serde_json::json!([]);
    forged["schema"] = 2.into();
    assert!(Status::parse(forged.to_string().as_bytes()).is_err());
}

#[test]
fn control_ids_are_unique_and_include_every_hardening_spec() {
    let ids = secblitz::platform::control_ids();
    let unique: HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "duplicate control id");
    for spec in secblitz::hardening::all() {
        assert!(
            ids.iter().any(|id| id == spec.id),
            "{} is missing from the platform catalogue",
            spec.id
        );
        assert!(secblitz::hardening::is_hardening(spec.id));
        assert_eq!(secblitz::hardening::is_ask(spec.id), spec.ask);
    }
}
