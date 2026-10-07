//! Tiny protection summary for the tray (`status.json`). Written by the monitor service and the elevated GUI, read by the unelevated tray. Holds only counts, ids and a timestamp.
use crate::model::{Authority, EffectiveFirewall, InboundAction, Observation};
use serde::{Deserialize, Serialize};

pub const SCHEMA: u32 = 1;
pub const LIMIT: usize = 8192;

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
    pub t: u64,
    pub protected: u32,
    pub total: u32,
    pub attention: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reverted: Vec<String>,
    pub state: State,
}

fn valid_id(id: &str) -> bool {
    id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

pub fn reverted_of(attention: &[String], changed: &[String]) -> Vec<String> {
    attention
        .iter()
        .filter(|id| changed.contains(id))
        .cloned()
        .collect()
}

impl Status {
    pub fn with_changed(mut self, changed: &[String]) -> Self {
        self.reverted = reverted_of(&self.attention, changed);
        self
    }

    /// A stale or future-dated status becomes `Unknown`, so the tray never shows "protected" from old or forged data.
    pub fn fresh(mut self, now: u64) -> Self {
        let stale = now.saturating_sub(self.t) > MAX_AGE;
        let future = self.t.saturating_sub(now) > MAX_FUTURE;
        if stale || future {
            self.state = State::Unknown;
        }
        self
    }

    pub fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        anyhow::ensure!(bytes.len() <= LIMIT, "status too large");
        let s: Status = serde_json::from_slice(bytes)?;
        anyhow::ensure!(s.schema == SCHEMA, "unknown status schema");
        anyhow::ensure!(s.protected <= s.total && s.total <= 256, "invalid counts");
        anyhow::ensure!(
            s.attention.len() <= 64 && s.reverted.len() <= 64,
            "too many ids"
        );
        anyhow::ensure!(
            s.attention.iter().chain(&s.reverted).all(|id| valid_id(id)),
            "invalid id"
        );
        Ok(s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Protected,
    Attention,
    Unknown,
}

/// Firewall controls also need local authority and matching effective evidence.
pub fn classify(id: &str, target: &serde_json::Value, o: &Observation) -> Item {
    if id.starts_with("firewall.") && !crate::hardening::is_hardening_check_id(id) {
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

/// An incomplete scan never reports "ok".
pub fn summarize(items: &[(String, Item)], complete: bool, now: u64) -> Status {
    let items = &items[..items.len().min(256)];
    let total = items.len() as u32;
    let protected = items.iter().filter(|(_, i)| *i == Item::Protected).count() as u32;
    let attention: Vec<String> = items
        .iter()
        .filter(|(id, i)| *i == Item::Attention && valid_id(id))
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
        reverted: Vec::new(),
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

pub fn path() -> anyhow::Result<std::path::PathBuf> {
    Ok(dir()?.join("status.json"))
}

/// Atomic: temp file in the same directory, then rename.
fn write_file(dir: &std::path::Path, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let stem = name.strip_suffix(".json").unwrap_or(name);
    let tmp = dir.join(format!("{stem}.{}.{nonce:x}.tmp", std::process::id()));
    let result = (|| -> anyhow::Result<()> {
        // create_new: never follow or truncate a pre-planted file or link.
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, dir.join(name))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

pub fn write_to(dir: &std::path::Path, status: &Status) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(status)?;
    anyhow::ensure!(bytes.len() <= LIMIT, "status too large");
    write_file(dir, "status.json", &bytes)
}

fn writable_dir() -> anyhow::Result<std::path::PathBuf> {
    #[cfg(windows)]
    if crate::service::trusted_status_dir().is_some() {
        return crate::service::ensure_status_dir();
    }
    crate::platform::app_dir()
}

pub fn write(status: &Status) -> anyhow::Result<()> {
    write_to(&writable_dir()?, status)
}

pub const CHANGED_SCHEMA: u32 = 1;
pub const CHANGED_LIMIT: usize = 16 * 1024;
pub const CHANGED_MAX_IDS: usize = 256;
const CHANGED_FILE: &str = "changed.json";

#[derive(Serialize, Deserialize)]
struct Changed {
    schema: u32,
    ids: Vec<String>,
}

/// Untrusted input: a bad schema, size, count or id makes the whole file unusable.
pub fn parse_changed(bytes: &[u8]) -> anyhow::Result<Vec<String>> {
    anyhow::ensure!(bytes.len() <= CHANGED_LIMIT, "changed list too large");
    let c: Changed = serde_json::from_slice(bytes)?;
    anyhow::ensure!(c.schema == CHANGED_SCHEMA, "unknown changed schema");
    anyhow::ensure!(c.ids.len() <= CHANGED_MAX_IDS, "too many ids");
    anyhow::ensure!(c.ids.iter().all(|id| valid_id(id)), "invalid id");
    Ok(c.ids)
}

pub fn write_changed_to(dir: &std::path::Path, ids: &[String]) -> anyhow::Result<()> {
    let mut clean: Vec<String> = Vec::new();
    for id in ids.iter().filter(|id| valid_id(id)) {
        if clean.len() < CHANGED_MAX_IDS && !clean.contains(id) {
            clean.push(id.clone());
        }
    }
    let bytes = serde_json::to_vec(&Changed {
        schema: CHANGED_SCHEMA,
        ids: clean,
    })?;
    anyhow::ensure!(bytes.len() <= CHANGED_LIMIT, "changed list too large");
    write_file(dir, CHANGED_FILE, &bytes)
}

pub fn write_changed(ids: &[String]) -> anyhow::Result<()> {
    write_changed_to(&writable_dir()?, ids)
}

/// None when the file is missing or unusable.
pub fn read_changed_from(dir: &std::path::Path) -> Option<Vec<String>> {
    use std::io::Read;
    let file = std::fs::File::open(dir.join(CHANGED_FILE)).ok()?;
    let mut bytes = Vec::new();
    file.take(CHANGED_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    parse_changed(&bytes).ok()
}

/// Written by the app because the tray cannot read the app's preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notify {
    pub schema: u32,
    pub reverted: bool,
    pub dangerous: bool,
}

const NOTIFY_FILE: &str = "notify.json";
const NOTIFY_LIMIT: usize = 1024;

impl Default for Notify {
    fn default() -> Self {
        Notify {
            schema: SCHEMA,
            reverted: true,
            dangerous: true,
        }
    }
}

impl Notify {
    pub fn new(reverted: bool, dangerous: bool) -> Self {
        Notify {
            reverted,
            dangerous,
            ..Notify::default()
        }
    }

    pub fn parse(bytes: &[u8]) -> Self {
        if bytes.len() > NOTIFY_LIMIT {
            return Notify::default();
        }
        serde_json::from_slice::<Notify>(bytes)
            .ok()
            .filter(|n| n.schema == SCHEMA)
            .unwrap_or_default()
    }
}

pub fn write_notify_to(dir: &std::path::Path, notify: &Notify) -> anyhow::Result<()> {
    write_file(dir, NOTIFY_FILE, &serde_json::to_vec(notify)?)
}

pub fn write_notify(notify: &Notify) -> anyhow::Result<()> {
    write_notify_to(&writable_dir()?, notify)
}

pub fn read_notify() -> Notify {
    use std::io::Read;
    let Ok(dir) = dir() else {
        return Notify::default();
    };
    let Ok(file) = std::fs::File::open(dir.join(NOTIFY_FILE)) else {
        return Notify::default();
    };
    let mut bytes = Vec::new();
    if file
        .take(NOTIFY_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Notify::default();
    }
    Notify::parse(&bytes)
}

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
            reverted: Vec::new(),
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
            ..Observation::default()
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

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reverted_is_attention_that_secblitz_changed() {
        let attention = ids(&["a.one", "b.two", "c.three"]);
        assert_eq!(
            reverted_of(&attention, &ids(&["c.three", "x.other", "a.one"])),
            ids(&["a.one", "c.three"])
        );
        assert!(reverted_of(&attention, &[]).is_empty());
        assert!(reverted_of(&[], &ids(&["a.one"])).is_empty());
        let s = st(State::Attention, 1, 4, &["a.one", "b.two"]).with_changed(&ids(&["b.two"]));
        assert_eq!(s.reverted, ids(&["b.two"]));
        assert_eq!(Status::parse(&bytes(&s)).unwrap(), s);
    }

    #[test]
    fn status_files_without_reverted_still_parse() {
        let old = br#"{"schema":1,"t":5,"protected":1,"total":2,"attention":["uac.enabled"],"state":"attention"}"#;
        let s = Status::parse(old).unwrap();
        assert!(s.reverted.is_empty());
        assert_eq!(s.attention, ids(&["uac.enabled"]));
        let quiet = bytes(&st(State::Ok, 1, 1, &[]));
        assert!(!String::from_utf8(quiet).unwrap().contains("reverted"));
        let newer = br#"{"schema":1,"t":5,"protected":1,"total":2,"attention":["a"],"reverted":["a"],"state":"attention","later":1}"#;
        assert_eq!(Status::parse(newer).unwrap().reverted, ids(&["a"]));
    }

    #[test]
    fn reverted_ids_are_validated_like_attention() {
        let mut s = st(State::Attention, 0, 1, &["a"]);
        s.reverted = ids(&["bad id"]);
        assert!(Status::parse(&bytes(&s)).is_err());
        s.reverted = ids(&["../x"]);
        assert!(Status::parse(&bytes(&s)).is_err());
        s.reverted = vec!["a".repeat(65)];
        assert!(Status::parse(&bytes(&s)).is_err());
        s.reverted = vec!["a".to_string(); 65];
        assert!(Status::parse(&bytes(&s)).is_err());
        s.reverted = ids(&["a"]);
        assert!(Status::parse(&bytes(&s)).is_ok());
    }

    #[test]
    fn changed_list_round_trips_and_rejects_bad_input() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_changed_from(dir.path()), None);
        write_changed_to(
            dir.path(),
            &ids(&["defender.pua", "bad id", "defender.pua", "a-b_c"]),
        )
        .unwrap();
        assert_eq!(
            read_changed_from(dir.path()),
            Some(ids(&["defender.pua", "a-b_c"]))
        );
        write_changed_to(dir.path(), &[]).unwrap();
        assert_eq!(read_changed_from(dir.path()), Some(Vec::new()));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);

        assert!(parse_changed(br#"{"schema":2,"ids":[]}"#).is_err());
        assert!(parse_changed(br#"{"ids":[]}"#).is_err());
        assert!(parse_changed(br#"{"schema":1,"ids":["a b"]}"#).is_err());
        assert!(parse_changed(br#"{"schema":1,"ids":[1]}"#).is_err());
        assert!(parse_changed(&vec![b' '; CHANGED_LIMIT + 1]).is_err());
        let many = serde_json::to_vec(&serde_json::json!({
            "schema": 1,
            "ids": vec!["a"; CHANGED_MAX_IDS + 1],
        }))
        .unwrap();
        assert!(parse_changed(&many).is_err());
        std::fs::write(dir.path().join("changed.json"), b"{broken").unwrap();
        assert_eq!(read_changed_from(dir.path()), None);
    }

    #[test]
    fn writing_many_changed_ids_keeps_the_file_within_its_limit() {
        let dir = tempfile::tempdir().unwrap();
        let many: Vec<String> = (0..400).map(|n| format!("some.check.number_{n}")).collect();
        write_changed_to(dir.path(), &many).unwrap();
        let read = read_changed_from(dir.path()).unwrap();
        assert_eq!(read.len(), CHANGED_MAX_IDS);
    }

    #[test]
    fn notify_defaults_to_both_on_and_survives_bad_files() {
        assert_eq!(Notify::default(), Notify::new(true, true));
        assert_eq!(Notify::parse(b""), Notify::default());
        assert_eq!(Notify::parse(b"{nope"), Notify::default());
        assert_eq!(
            Notify::parse(br#"{"schema":9,"reverted":false,"dangerous":false}"#),
            Notify::default()
        );
        assert_eq!(
            Notify::parse(br#"{"schema":1,"reverted":false,"dangerous":true}"#),
            Notify::new(false, true)
        );
        assert_eq!(
            Notify::parse(&vec![b' '; NOTIFY_LIMIT + 1]),
            Notify::default()
        );
        let dir = tempfile::tempdir().unwrap();
        write_notify_to(dir.path(), &Notify::new(false, false)).unwrap();
        let bytes = std::fs::read(dir.path().join("notify.json")).unwrap();
        assert_eq!(Notify::parse(&bytes), Notify::new(false, false));
    }
}
