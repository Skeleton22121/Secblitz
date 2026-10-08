//! The last full check, reused when Secblitz reopens soon after.
use secblitz::engine::Report;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

pub const FILE: &str = "last-check.json";
pub const FRESH_SECONDS: u64 = 3600;

#[derive(Serialize, Deserialize)]
struct Saved {
    version: String,
    user: String,
    at: u64,
    report: Report,
}

// A restart can finish changes that were waiting for it, so older checks are stale.
fn fresh(at: u64, now: u64, boot: u64) -> bool {
    at >= boot && at <= now && now - at < FRESH_SECONDS
}

pub fn save(dir: &Path, user: &str, at: u64, report: &Report) -> anyhow::Result<()> {
    #[derive(Serialize)]
    struct SavedRef<'a> {
        version: &'a str,
        user: &'a str,
        at: u64,
        report: &'a Report,
    }
    let data = serde_json::to_vec(&SavedRef {
        version: env!("CARGO_PKG_VERSION"),
        user,
        at,
        report,
    })?;
    let path = dir.join(FILE);
    let tmp = dir.join(format!("{FILE}.tmp"));
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(&data)?;
        file.sync_all()?;
    }
    if let Err(e) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

pub fn load(dir: &Path, user: &str, now: u64, boot: u64) -> Option<(Report, u64)> {
    let data = std::fs::read(dir.join(FILE)).ok()?;
    let saved: Saved = serde_json::from_slice(&data).ok()?;
    (saved.version == env!("CARGO_PKG_VERSION") && saved.user == user && fresh(saved.at, now, boot))
        .then_some((saved.report, saved.at))
}

/// Settings the monitor saw switched back that this check still shows as protected.
pub fn missed_switch_backs<'a>(report: &Report, reverted: &'a [String]) -> Vec<&'a String> {
    reverted
        .iter()
        .filter(|id| {
            report
                .results
                .iter()
                .any(|r| &&r.id == id && r.status == secblitz::model::CheckStatus::Compliant)
        })
        .collect()
}

pub fn forget(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(FILE));
}

pub fn boot_time(now: u64) -> Option<u64> {
    #[cfg(windows)]
    {
        // Milliseconds since Windows started, counting sleep.
        // SAFETY: GetTickCount64 takes no arguments and has no preconditions.
        let up = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() };
        Some(now.saturating_sub(up / 1000))
    }
    #[cfg(not(windows))]
    {
        let _ = now;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secblitz::engine::Outcome;
    use secblitz::model::CheckStatus;

    fn report() -> Report {
        Report {
            results: vec![Outcome {
                id: "defender.realtime".into(),
                title: "Live virus protection".into(),
                status: CheckStatus::Compliant,
                detail: "On".into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_check_is_fresh_for_an_hour_after_the_last_restart() {
        assert!(fresh(1000, 1000, 500));
        assert!(fresh(1000, 1000 + FRESH_SECONDS - 1, 500));
        assert!(!fresh(1000, 1000 + FRESH_SECONDS, 500));
        // Made before Windows restarted: changes may have taken effect since.
        assert!(!fresh(400, 1000, 500));
        // A clock that went backwards never makes an old check look new.
        assert!(!fresh(2000, 1000, 500));
    }

    #[test]
    fn a_switch_back_counts_only_when_the_check_still_shows_it_protected() {
        let mut r = report();
        let ids = vec!["defender.realtime".to_string(), "other".to_string()];
        assert_eq!(missed_switch_backs(&r, &ids), vec![&ids[0]]);
        r.results[0].status = CheckStatus::Attention;
        assert!(missed_switch_backs(&r, &ids).is_empty());
        assert!(missed_switch_backs(&report(), &[]).is_empty());
    }

    #[test]
    fn a_saved_check_comes_back_only_for_the_same_account() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), "S-1-5-21-1", 1000, &report()).unwrap();
        let (back, at) = load(dir.path(), "S-1-5-21-1", 1100, 900).unwrap();
        assert_eq!(at, 1000);
        assert_eq!(back.results.len(), 1);
        assert_eq!(back.results[0].id, "defender.realtime");
        assert!(load(dir.path(), "S-1-5-21-2", 1100, 900).is_none());
        assert!(load(dir.path(), "S-1-5-21-1", 1000 + FRESH_SECONDS, 900).is_none());
        assert!(load(dir.path(), "S-1-5-21-1", 1100, 1050).is_none());
    }

    #[test]
    fn a_saved_check_with_an_unrecognised_status_still_loads_and_keeps_its_text() {
        let dir = tempfile::tempdir().unwrap();
        let text = format!(
            r#"{{"version":"{}","user":"S-1","at":1000,"report":{{"transaction":null,"results":[{{"id":"a","title":"A","status":"attention","detail":"d"}},{{"id":"b","title":"B","status":"from_a_newer_build","detail":"d"}}],"findings":[{{"title":"F","status":"review","detail":"d"}}]}}}}"#,
            env!("CARGO_PKG_VERSION")
        );
        std::fs::write(dir.path().join(FILE), text).unwrap();
        let (back, _) = load(dir.path(), "S-1", 1100, 900).unwrap();
        assert_eq!(back.results[0].status, CheckStatus::Attention);
        assert_eq!(
            back.results[1].status,
            CheckStatus::Other("from_a_newer_build".into())
        );
        assert_eq!(back.findings[0].status, CheckStatus::Review);
        save(dir.path(), "S-1", 1000, &back).unwrap();
        let again = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
        assert!(again.contains(r#""status":"from_a_newer_build""#));
    }

    #[test]
    fn another_version_or_a_damaged_file_means_a_new_check() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), "S-1-5-21-1", 1000, &report()).unwrap();
        let path = dir.path().join(FILE);
        let text = std::fs::read_to_string(&path).unwrap();
        let other = text.replace(env!("CARGO_PKG_VERSION"), "0.0.1");
        std::fs::write(&path, other).unwrap();
        assert!(load(dir.path(), "S-1-5-21-1", 1100, 900).is_none());
        std::fs::write(&path, "{not json").unwrap();
        assert!(load(dir.path(), "S-1-5-21-1", 1100, 900).is_none());
        forget(dir.path());
        assert!(!path.exists());
        assert!(load(dir.path(), "S-1-5-21-1", 1100, 900).is_none());
    }
}
