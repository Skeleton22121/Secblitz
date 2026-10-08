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
    Wait(&'static str),
}

/// Whether full protection may be offered. `started` is the recorded start of watching; `mode` is what Defender reports now. Only mode 2, which watches everything full protection would block, counts as watching. Watching that was turned on outside Secblitz has no recorded start, so it is offered at once rather than never.
pub(crate) fn verdict(started: Option<u64>, mode: Option<u64>, now: u64) -> Verdict {
    match (started, mode) {
        (Some(at), Some(2)) if at <= now && now - at >= WATCH_SECONDS => Verdict::Offer,
        (Some(_), Some(2)) => Verdict::Wait(STILL_WATCHING),
        (None, Some(2)) => Verdict::Offer,
        _ => Verdict::Wait(NOT_WATCHED),
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

    fn cfa_record(&self) -> serde_json::Map<String, serde_json::Value> {
        let path = self.cfa_dir().join(RECORD);
        let is_file = fs::symlink_metadata(&path).is_ok_and(|m| m.is_file());
        is_file
            .then(|| fs::read(&path).ok())
            .flatten()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    }

    fn cfa_started(&self) -> Option<u64> {
        self.cfa_record().get("started")?.as_u64()
    }

    /// Whether Secblitz turned full protection on and has not undone it.
    fn cfa_protected(&self) -> bool {
        self.cfa_record().get("protected") == Some(&serde_json::Value::Bool(true))
    }

    fn cfa_update(&self, change: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>)) {
        let mut record = self.cfa_record();
        change(&mut record);
        let dir = self.cfa_dir();
        let path = dir.join(RECORD);
        if record.is_empty() {
            let _ = fs::remove_file(&path);
            return;
        }
        let _ = fs::create_dir(&dir);
        let tmp = dir.join(format!("{RECORD}.tmp"));
        let data = serde_json::Value::Object(record).to_string();
        if fs::write(&tmp, data).is_ok() && fs::rename(&tmp, &path).is_err() {
            let _ = fs::remove_file(&tmp);
        }
    }

    /// Full protection waits for a week of watching. Once Secblitz has turned it on, it stays offered so a switch back can be put back. This only narrows what is offered; it never changes a setting.
    pub(super) fn cfa_gate(&self, id: &str, obs: &mut Observation) {
        if id != BLOCK || !obs.eligible {
            return;
        }
        let unsafe_now = crate::hardening::spec(id).is_some_and(|s| s.any_unsafe(&obs.value));
        if !unsafe_now || self.cfa_protected() {
            return;
        }
        let mode = obs.value["items"][MODE].as_u64();
        let now = now();
        match verdict(self.cfa_started(), mode, now) {
            Verdict::Offer => {}
            Verdict::Wait(reason) => {
                obs.eligible = false;
                obs.reason = reason.into();
            }
        }
    }

    /// Starts the week when watching is turned on and forgets it when watching is undone, and remembers whether full protection is Secblitz's.
    pub(super) fn cfa_note(&self, results: &[Outcome]) {
        for r in results.iter().filter(|r| r.id == WATCH || r.id == BLOCK) {
            let key = if r.id == WATCH {
                "started"
            } else {
                "protected"
            };
            match r.status {
                CheckStatus::Applied => self.cfa_update(|record| {
                    let value = if r.id == WATCH {
                        now().into()
                    } else {
                        true.into()
                    };
                    record.insert(key.into(), value);
                }),
                CheckStatus::Restored => self.cfa_update(|record| {
                    record.remove(key);
                }),
                _ => {}
            }
        }
    }
}
