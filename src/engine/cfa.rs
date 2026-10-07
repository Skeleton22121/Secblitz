//! Folder protection starts in watch mode. The week of watching is timed by a small record next to the journal, so full protection is only offered once the person has seen what it would block.

use super::{Engine, Outcome};
use crate::model::{CheckStatus, Observation};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) const WATCH: &str = "defender.cfa_watch";
pub(crate) const BLOCK: &str = "defender.cfa_block";
const MODE: &str = "EnableControlledFolderAccess";
const RECORD: &str = "cfa-watch.json";
pub(crate) const WATCH_SECONDS: u64 = 7 * 24 * 60 * 60;

pub(crate) const NOT_WATCHED: &str = "Not offered: folder protection has not been watched yet";
pub(crate) const STILL_WATCHING: &str =
    "Not offered: folder protection has been watched for less than a week";

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Offer,
    StartClock,
    Wait(&'static str),
}

/// Whether full protection may be offered. `started` is the recorded start of watching; `mode` is what Defender reports now (2 and 4 are the watch-only modes). A start that lies in the future is a wrong clock and never counts.
pub(crate) fn verdict(started: Option<u64>, mode: Option<u64>, now: u64) -> Verdict {
    match started {
        Some(at) if at <= now && now - at >= WATCH_SECONDS => Verdict::Offer,
        Some(_) => Verdict::Wait(STILL_WATCHING),
        None if matches!(mode, Some(2 | 4)) => Verdict::StartClock,
        None => Verdict::Wait(NOT_WATCHED),
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Engine {
    fn cfa_dir(&self) -> PathBuf {
        self.dir.join("App")
    }

    fn cfa_started(&self) -> Option<u64> {
        let path = self.cfa_dir().join(RECORD);
        if !fs::symlink_metadata(&path).ok()?.is_file() {
            return None;
        }
        let record: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
        record.get("started")?.as_u64()
    }

    fn cfa_write(&self, started: u64) {
        let dir = self.cfa_dir();
        let _ = fs::create_dir(&dir);
        let path = dir.join(RECORD);
        let tmp = dir.join(format!("{RECORD}.tmp"));
        let data = serde_json::json!({ "started": started }).to_string();
        if fs::write(&tmp, data).is_ok() && fs::rename(&tmp, &path).is_err() {
            let _ = fs::remove_file(&tmp);
        }
    }

    /// Full protection waits for a week of watching. This only narrows what is offered; it never changes a setting.
    pub(super) fn cfa_gate(&self, id: &str, obs: &mut Observation) {
        if id != BLOCK || !obs.eligible {
            return;
        }
        let unsafe_now = crate::hardening::spec(id).is_some_and(|s| s.any_unsafe(&obs.value));
        if !unsafe_now {
            return;
        }
        let mode = obs.value["items"][MODE].as_u64();
        let now = now();
        match verdict(self.cfa_started(), mode, now) {
            Verdict::Offer => {}
            Verdict::StartClock => {
                self.cfa_write(now);
                obs.eligible = false;
                obs.reason = STILL_WATCHING.into();
            }
            Verdict::Wait(reason) => {
                obs.eligible = false;
                obs.reason = reason.into();
            }
        }
    }

    /// Starts the week when watching is turned on and forgets it when watching is undone.
    pub(super) fn cfa_note(&self, results: &[Outcome]) {
        for r in results.iter().filter(|r| r.id == WATCH) {
            match r.status {
                CheckStatus::Applied => self.cfa_write(now()),
                CheckStatus::Restored => {
                    let _ = fs::remove_file(self.cfa_dir().join(RECORD));
                }
                _ => {}
            }
        }
    }
}
