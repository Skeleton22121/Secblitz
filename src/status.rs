//! Tiny user-readable protection summary for the tray (`status.json`).
//!
//! OWNER: platform agent. Written by the monitor service (LocalService) and by
//! the elevated GUI after each check; read by the unelevated tray. Contains no
//! paths, evidence or details — only counts, ids and a timestamp.
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
    /// Parse and validate untrusted bytes (size, schema, id charset).
    pub fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        anyhow::ensure!(bytes.len() <= LIMIT, "status too large");
        let s: Status = serde_json::from_slice(bytes)?;
        anyhow::ensure!(s.schema == SCHEMA, "unknown status schema");
        anyhow::ensure!(s.protected <= s.total && s.total <= 256, "invalid counts");
        anyhow::ensure!(s.attention.len() <= 64, "too many ids");
        anyhow::ensure!(
            s.attention.iter().all(|id| id.len() <= 64
                && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')),
            "invalid id"
        );
        Ok(s)
    }
}

/// Path of the shared status file (`<Program Files>\Secblitz\Status\status.json`).
pub fn path() -> anyhow::Result<std::path::PathBuf> {
    // TODO(platform): resolve from the trusted install root used by service.rs.
    anyhow::bail!("status path not implemented")
}

/// Atomically replace the shared status file. Requires write access
/// (elevated GUI or the monitor service).
pub fn write(status: &Status) -> anyhow::Result<()> {
    // TODO(platform)
    let _ = status;
    Ok(())
}

/// Read the shared status file, if present and valid.
pub fn read() -> Option<Status> {
    let bytes = std::fs::read(path().ok()?).ok()?;
    Status::parse(&bytes).ok()
}
