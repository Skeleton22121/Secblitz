//! Tiny user-readable protection summary for the tray (`status.json`).
//!
//! OWNER: platform agent. Written by the monitor service (LocalService) and by
//! the elevated GUI after each check; read by the unelevated tray. Contains no
//! paths, evidence or details — only counts, ids and a timestamp.
use crate::model::{Authority, EffectiveFirewall, InboundAction, Observation};
use serde::{Deserialize, Serialize};

pub const SCHEMA: u32 = 1;
pub const LIMIT: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Ok,
    Attention,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub schema: u32,
    /// Unix seconds when the check finished.
    pub t: u64,
    pub protected: u32,
    pub total: u32,
    /// Control ids needing attention (ASCII ids only, capped).
    pub attention: Vec<String>,
    pub state: State,
}

impl Status {
    /// Downgrade a stale or future-dated status to `Unknown` so a tray never
    /// shows "protected" from old or forged data.
    pub fn fresh(mut self, now: u64) -> Self {
        let stale = now.saturating_sub(self.t) > MAX_AGE;
        let future = self.t.saturating_sub(now) > MAX_FUTURE;
        if stale || future {
            self.state = State::Unknown;
        }
        self
    }

    /// Parse and validate untrusted bytes (size, schema, id charset).
    pub fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        anyhow::ensure!(bytes.len() <= LIMIT, "status too large");
        let s: Status = serde_json::from_slice(bytes)?;
        anyhow::ensure!(s.schema == SCHEMA, "unknown status schema");
        anyhow::ensure!(s.protected <= s.total && s.total <= 256, "invalid counts");
        anyhow::ensure!(s.attention.len() <= 64, "too many ids");
        anyhow::ensure!(
            s.attention.iter().all(|id| id.len() <= 64
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')),
            "invalid id"
        );
        Ok(s)
    }
}

/// Per-control result used to build a [`Status`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Protected,
    Attention,
    Unknown,
}

/// Classification of one observed control, mirroring the engine's assessment:
/// a preference that differs from the target is not protected; firewall
/// controls additionally need local authority and matching effective evidence.
pub fn classify(id: &str, target: &serde_json::Value, o: &Observation) -> Item {
    if id.starts_with("firewall.") && !crate::hardening::is_hardening(id) {
        if o.authority != Some(Authority::Local) || !o.eligible {
            return Item::Attention;
        }
        let ok = match (o.effective, id.ends_with(".enabled")) {
            (Some(EffectiveFirewall::Enabled(on)), true) => on && o.value.as_bool() == Some(true),
            (Some(EffectiveFirewall::Inbound(a)), false) => {
                a == InboundAction::Block && (o.value == "Block" || o.value == "NotConfigured")
            }
            _ => false,
        };
        return if ok { Item::Protected } else { Item::Attention };
    }
    if id.starts_with("permissions.service.") {
        return match crate::permissions::repair_target(id, &o.value) {
            Ok(t) if o.eligible && t == o.value => Item::Protected,
            _ => Item::Attention,
        };
    }
    if o.value == *target {
        Item::Protected
    } else {
        Item::Attention
    }
}

/// Build a status from per-control results. `complete` is false when the scan
/// was cut short; an incomplete scan never reports "ok".
pub fn summarize(items: &[(String, Item)], complete: bool, now: u64) -> Status {
    let items = &items[..items.len().min(256)];
    let total = items.len() as u32;
    let protected = items.iter().filter(|(_, i)| *i == Item::Protected).count() as u32;
    let attention: Vec<String> = items
        .iter()
        .filter(|(id, i)| {
            *i == Item::Attention
                && id.len() <= 64
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        })
        .take(64)
        .map(|(id, _)| id.clone())
        .collect();
    let any_attention = items.iter().any(|(_, i)| *i == Item::Attention);
    let state = if any_attention {
        State::Attention
    } else if total > 0 && complete && protected == total {
        State::Ok
    } else {
        State::Unknown
    };
    Status {
        schema: SCHEMA,
        t: now,
        protected,
        total,
        attention,
        state,
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Directory holding the shared status file: the trusted
/// `<Program Files>\Secblitz\Status` when running from the installed location,
/// otherwise (portable/dev) the app namespace of the state directory.
fn dir() -> anyhow::Result<std::path::PathBuf> {
    #[cfg(windows)]
    if let Some(d) = crate::service::trusted_status_dir() {
        return Ok(d);
    }
    crate::platform::app_dir()
}

/// Path of the shared status file (`<Program Files>\Secblitz\Status\status.json`).
pub fn path() -> anyhow::Result<std::path::PathBuf> {
    Ok(dir()?.join("status.json"))
}

/// Atomically write `status.json` into `dir` (temp file in the same directory,
/// then rename over the target). Shared by the service and the GUI.
pub fn write_to(dir: &std::path::Path, status: &Status) -> anyhow::Result<()> {
    use std::io::Write;
    let bytes = serde_json::to_vec(status)?;
    anyhow::ensure!(bytes.len() <= LIMIT, "status too large");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let tmp = dir.join(format!("status.{}.{nonce:x}.tmp", std::process::id()));
    let result = (|| -> anyhow::Result<()> {
        // create_new: never follow or truncate a pre-planted file or link.
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, dir.join("status.json"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Atomically replace the shared status file. Requires write access
/// (elevated GUI or the monitor service); failures are non-fatal for callers.
pub fn write(status: &Status) -> anyhow::Result<()> {
    #[cfg(windows)]
    if crate::service::trusted_status_dir().is_some() {
        let d = crate::service::ensure_status_dir()?;
        return write_to(&d, status);
    }
    write_to(&crate::platform::app_dir()?, status)
}

/// Read the shared status file, if present and valid.
pub fn read() -> Option<Status> {
    let p = path().ok()?;
    if std::fs::metadata(&p).ok()?.len() > LIMIT as u64 {
        return None;
    }
    Status::parse(&std::fs::read(p).ok()?)
        .ok()
        .map(|s| s.fresh(now()))
}

/// A status older than this is no longer evidence of anything.
pub const MAX_AGE: u64 = 2 * 60 * 60;
/// Clock skew tolerated before a timestamp counts as forged.
pub const MAX_FUTURE: u64 = 5 * 60;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stale_or_future_status_becomes_unknown() {
        let s = st(State::Ok, 1, 1, &[]);
        assert_eq!(s.clone().fresh(s.t + 60).state, State::Ok);
        assert_eq!(s.clone().fresh(s.t + MAX_AGE + 1).state, State::Unknown);
        let mut f = s.clone();
        f.t = 10_000;
        assert_eq!(f.fresh(10_000 - MAX_FUTURE - 1).state, State::Unknown);
    }

    fn st(state: State, protected: u32, total: u32, ids: &[&str]) -> Status {
        Status {
            schema: SCHEMA,
            t: 1,
            protected,
            total,
            attention: ids.iter().map(|s| s.to_string()).collect(),
            state,
        }
    }
    fn bytes(s: &Status) -> Vec<u8> {
        serde_json::to_vec(s).unwrap()
    }

    #[test]
    fn parse_round_trip_and_limits() {
        let s = st(State::Attention, 3, 5, &["uac.enabled", "a-b_c.d"]);
        assert_eq!(Status::parse(&bytes(&s)).unwrap(), s);
        assert!(Status::parse(&vec![b' '; LIMIT + 1]).is_err());
        assert!(Status::parse(b"{}").is_err());
        let mut bad = s.clone();
        bad.schema = 2;
        assert!(Status::parse(&bytes(&bad)).is_err());
        assert!(Status::parse(&bytes(&st(State::Ok, 6, 5, &[]))).is_err());
        assert!(Status::parse(&bytes(&st(State::Ok, 1, 257, &[]))).is_err());
        assert!(Status::parse(&bytes(&st(State::Attention, 0, 1, &["a b"]))).is_err());
        assert!(Status::parse(&bytes(&st(State::Attention, 0, 1, &["../x"]))).is_err());
        let long = "a".repeat(65);
        assert!(Status::parse(&bytes(&st(State::Attention, 0, 1, &[&long]))).is_err());
        let many: Vec<&str> = vec!["a"; 65];
        assert!(Status::parse(&bytes(&st(State::Attention, 0, 70, &many))).is_err());
    }

    #[test]
    fn summarize_states() {
        let p = |id: &str| (id.to_string(), Item::Protected);
        let a = |id: &str| (id.to_string(), Item::Attention);
        let u = |id: &str| (id.to_string(), Item::Unknown);
        assert_eq!(summarize(&[p("x"), p("y")], true, 5).state, State::Ok);
        assert_eq!(summarize(&[p("x"), p("y")], false, 5).state, State::Unknown);
        assert_eq!(summarize(&[], true, 5).state, State::Unknown);
        assert_eq!(summarize(&[p("x"), u("y")], true, 5).state, State::Unknown);
        let s = summarize(&[p("x"), a("uac.enabled"), a("bad id")], true, 5);
        assert_eq!((s.state, s.protected, s.total), (State::Attention, 1, 3));
        assert_eq!(s.attention, vec!["uac.enabled".to_string()]);
        assert!(Status::parse(&bytes(&s)).is_ok());
    }

    #[test]
    fn classify_matches_target() {
        let obs = |value| Observation {
            value,
            eligible: true,
            reason: String::new(),
            effective: None,
            authority: None,
            labels: Vec::new(),
        };
        assert_eq!(
            classify("defender.realtime", &json!(false), &obs(json!(false))),
            Item::Protected
        );
        assert_eq!(
            classify("defender.realtime", &json!(false), &obs(json!(true))),
            Item::Attention
        );
        let mut fw = obs(json!(true));
        assert_eq!(
            classify("firewall.public.enabled", &json!(true), &fw),
            Item::Attention
        );
        fw.authority = Some(Authority::Local);
        fw.effective = Some(EffectiveFirewall::Enabled(true));
        assert_eq!(
            classify("firewall.public.enabled", &json!(true), &fw),
            Item::Protected
        );
    }

    #[test]
    fn write_to_replaces_atomically() {
        let dir = tempfile::tempdir().unwrap();
        write_to(dir.path(), &st(State::Ok, 1, 1, &[])).unwrap();
        write_to(dir.path(), &st(State::Attention, 0, 1, &["x"])).unwrap();
        let data = std::fs::read(dir.path().join("status.json")).unwrap();
        assert_eq!(Status::parse(&data).unwrap().state, State::Attention);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
