//! Exact, single-use Windows quality-update plans. See `capabilities()` before
//! offering actions. All native work is opt-in, supervised, and never reboots.
#![cfg_attr(not(windows), allow(dead_code))]

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

#[path = "patching/core.rs"]
mod core;
#[cfg(windows)]
#[path = "patching/process.rs"]
mod process;
#[cfg(any(windows, test))]
#[path = "patching/script.rs"]
mod script;
#[cfg(windows)]
#[path = "patching/storage.rs"]
mod storage;
#[cfg(test)]
#[path = "patching/tests.rs"]
mod tests;
#[cfg(windows)]
#[path = "patching/windows.rs"]
mod windows;

const SCHEMA: u32 = 1;
const MAX_STATE_BYTES: usize = 8 * 1024 * 1024;
const MAX_UPDATES: usize = 32;
const MAX_RECORDS: usize = 32;
const MAX_PLAN_SECONDS: u64 = 86400;
const MAX_APPROVAL_SECONDS: u64 = 3600;
const SOURCE: &str = "9482f4b4-e343-43b6-b170-9a65bc822c77";

#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    pub windows_quality_updates: bool,
    pub selected_app_upgrades: bool,
    pub quality_update_scope: &'static str,
    pub app_upgrade_unavailable_reason: &'static str,
}
pub fn capabilities() -> Capabilities {
    Capabilities {
        windows_quality_updates: cfg!(all(windows, target_arch = "x86_64")),
        selected_app_upgrades: false,
        quality_update_scope: "Windows x64, unmanaged interactive same-user administrator only. Current Windows Update security/critical Windows-family software updates, including reviewed bundles. Optional, preview, driver, feature/upgrade and interactive/exclusive updates are excluded. No source registration, policy changes, forced servicing termination or automatic reboot.",
        app_upgrade_unavailable_reason: "No trusted original-unelevated-user WinGet broker or independently verified source/version/architecture/scope/pin evidence is available. Application patching is disabled; no WinGet executable, installer, path or arguments are accepted.",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub machine: String,
    pub original_user: String,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateIdentity {
    pub update_id: Uuid,
    pub revision: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    pub identity: UpdateIdentity,
    pub title: String,
    pub description: String,
    pub kb_articles: Vec<String>,
    pub categories: Vec<Uuid>,
    pub max_download_bytes: u64,
    pub last_changed: String,
    pub severity: String,
    pub handler: String,
    pub reboot_behavior: u32,
    pub eula: String,
    pub bundled: Vec<Update>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub binding: Binding,
    pub searched_at: u64,
    pub source: String,
    pub updates: Vec<Update>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    pub selected: Vec<UpdateIdentity>,
    pub valid_for_seconds: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: u32,
    pub id: Uuid,
    pub binding: Binding,
    pub created_at: u64,
    pub expires_at: u64,
    pub source: String,
    pub updates: Vec<Update>,
    pub digest: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Consent {
    pub owner_opt_in: bool,
    pub accept_windows_update_source: bool,
    pub accept_reviewed_eulas: bool,
    pub acknowledge_no_automatic_rollback: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    pub digest: String,
    pub approved_at: u64,
    pub expires_at: u64,
    pub consent: Consent,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Planned,
    Consumed,
    Downloading,
    Downloaded,
    Installing,
    Verifying,
    Succeeded,
    RebootRequired,
    NeedsReview,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub creation_time: u64,
    /// Kernel boot identifier, not a wall-clock estimate. Missing legacy
    /// evidence cannot establish completion of an abandoned WUA service request.
    #[serde(default)]
    pub boot_id: Option<Uuid>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verification {
    pub installed: Vec<UpdateIdentity>,
    pub reboot_pending: bool,
    pub checked_at: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub plan: Plan,
    pub approval: Option<Approval>,
    pub status: Status,
    pub process: Option<ProcessIdentity>,
    /// Never erased by later verification: interruption is part of the audit.
    pub uncertain: bool,
    pub verification: Option<Verification>,
}

/// Cancellation/drop prevents later phases, never terminates an in-flight WUA
/// call. The supervisor retains lock/pins until the process tree exits. Lost
/// clients may leave work in Windows services: unacknowledged phases require a
/// subsequent boot plus independent verification, never automatic replay.
pub struct Task<T> {
    result: mpsc::Receiver<Result<T>>,
    cancel: Arc<AtomicBool>,
}
impl<T> Task<T> {
    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn wait(&self, timeout: Duration) -> Result<Option<T>> {
        match self.result.recv_timeout(timeout) {
            Ok(r) => r.map(Some),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("Patching task ended/result already consumed; inspect protected records")
            }
        }
    }
}
impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        self.request_cancel();
    }
}
fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
fn hash<T: Serialize>(v: &T) -> Result<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(v)?)))
}

/// Online metadata search only. Caller must first obtain consent to contacting
/// the current unmanaged Microsoft Windows Update source. No EULA acceptance,
/// payload download or install. Native source/policy uncertainty is a hard veto.
pub fn discover() -> Result<Task<Catalog>> {
    spawn(|e| e.discover())
}
pub fn plan(request: PlanRequest) -> Result<Task<Plan>> {
    spawn(move |e| e.plan(request))
}
pub fn approve(
    id: Uuid,
    displayed_digest: &str,
    consent: Consent,
    valid_for_seconds: u64,
) -> Result<Record> {
    native(|e| e.approve(id, displayed_digest, consent, valid_for_seconds))
}
pub fn list() -> Result<Vec<Record>> {
    native(|e| Ok(e.records().to_vec()))
}
pub fn get(id: Uuid) -> Result<Record> {
    native(|e| Ok(e.record(id)?.clone()))
}
pub fn start(id: Uuid, displayed_digest: &str) -> Result<Task<Record>> {
    let digest = displayed_digest.to_owned();
    spawn(move |e| e.run(id, &digest))
}
/// Read-only recovery; never accepts EULAs, downloads, installs or replays.
pub fn verify(id: Uuid) -> Result<Task<Record>> {
    spawn(move |e| e.verify(id))
}
pub fn ensure_idle(shared_engine_lock: &std::fs::File) -> Result<()> {
    #[cfg(windows)]
    {
        storage::ensure_idle(shared_engine_lock)
    }
    #[cfg(not(windows))]
    {
        let _ = shared_engine_lock;
        bail!("Patching interlock requires Windows")
    }
}

#[cfg(windows)]
type Native = core::Engine<storage::Store, windows::Backend>;
#[cfg(not(windows))]
type Native = core::Engine<core::Unsupported, core::Unsupported>;
fn native<T>(f: impl FnOnce(&mut Native) -> Result<T>) -> Result<T> {
    #[cfg(windows)]
    {
        let mut e = open()?;
        f(&mut e)
    }
    #[cfg(not(windows))]
    {
        let _ = f;
        bail!("Patching requires elevated interactive Windows x64")
    }
}
#[cfg(windows)]
fn open() -> Result<Native> {
    let store = storage::Store::open()?;
    let backend = windows::Backend::new(store.root())?;
    core::Engine::open(store, backend)
}
fn spawn<T: Send + 'static>(
    f: impl FnOnce(&mut Native) -> Result<T> + Send + 'static,
) -> Result<Task<T>> {
    #[cfg(windows)]
    {
        let mut e = open()?; // contention fails synchronously, before worker creation
        let cancel = e.cancellation();
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("patching-supervisor".into())
            .spawn(move || {
                let _ = tx.send(f(&mut e));
            })?;
        Ok(Task { result, cancel })
    }
    #[cfg(not(windows))]
    {
        let _ = f;
        bail!("Patching requires elevated interactive Windows x64")
    }
}
