//! Score log (`checks.jsonl`) and the merged History timeline.
//! OWNER: app-core agent.
//!
//! Contract:
//! - `record(dir, entry)` appends one line, keeps at most 500 lines.
//! - `load(dir)` returns entries oldest→newest, skipping malformed lines.
//! - `timeline(entries, engine_history, debloat_batches)` merges for display.
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FILE: &str = "checks.jsonl";
pub const MAX_LINES: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Check,
    Fix,
    Undo,
    Debloat,
    Restore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Unix seconds.
    pub t: u64,
    pub kind: Kind,
    pub protected: usize,
    pub total: usize,
    /// Items changed by this event (fixes applied, apps removed, …).
    #[serde(default)]
    pub n: usize,
}

pub fn record(dir: &Path, entry: &Entry) -> anyhow::Result<()> {
    // TODO(app-core): append + trim to MAX_LINES atomically.
    let _ = (dir, entry);
    Ok(())
}

pub fn load(dir: &Path) -> Vec<Entry> {
    // TODO(app-core)
    let _ = dir;
    Vec::new()
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
