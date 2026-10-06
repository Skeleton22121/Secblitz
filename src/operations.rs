//! Durable, opt-in maintenance, independent of reversible hardening controls.
#![cfg_attr(not(windows), allow(dead_code))]

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[cfg(any(windows, test))]
#[path = "operations/commands.rs"]
mod commands;
#[path = "operations/core.rs"]
mod core;
#[cfg(windows)]
#[path = "operations/storage.rs"]
mod storage;
#[cfg(windows)]
pub(crate) use storage::{pin_system_executable, pin_system_module};
#[cfg(windows)]
#[path = "operations/process.rs"]
mod process;
#[cfg(test)]
#[path = "operations/tests.rs"]
mod tests;
#[cfg(windows)]
#[path = "operations/windows.rs"]
mod windows;

const SCHEMA: u32 = 1;
const MAX_PLANS: usize = 128;
const MAX_STATE_BYTES: usize = 1024 * 1024;
const MAX_APPROVAL_SECONDS: u64 = 15 * 60;
const MAX_PLAN_SECONDS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    DismCheckHealth,
    DismScanHealth,
    DismRestoreHealth,
    SfcVerify,
    SfcRepair,
    DefenderQuickScan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    DiagnosticIo,
    SystemRepair,
    AntivirusRemediation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reversibility {
    NoConfigurationChange,
    NoAutomaticRollback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationSpec {
    pub kind: OperationKind,
    pub risk: Risk,
    pub reversibility: Reversibility,
    pub timeout_seconds: u64,
    pub downloads: bool,
    pub network_access: bool,
    pub may_require_reboot: bool,
    pub explanation: String,
}

impl OperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DismCheckHealth => "dism_check_health",
            Self::DismScanHealth => "dism_scan_health",
            Self::DismRestoreHealth => "dism_restore_health",
            Self::SfcVerify => "sfc_verify",
            Self::SfcRepair => "sfc_repair",
            Self::DefenderQuickScan => "defender_quick_scan",
        }
    }
    pub fn spec(self) -> OperationSpec {
        let (risk, timeout_seconds, explanation) = match self {
            Self::DismCheckHealth => (Risk::DiagnosticIo, 120, "Reads cached component-store health; not a full scan."),
            Self::DismScanHealth => (Risk::DiagnosticIo, 3600, "Scans the component store; consumes disk/CPU and writes diagnostic logs."),
            Self::SfcVerify => (Risk::DiagnosticIo, 3600, "Verifies protected system files. Exit zero alone does not prove file integrity; review CBS results."),
            Self::DismRestoreHealth => (Risk::SystemRepair, 7200, "Repairs the online component store using /LimitAccess /NoRestart. No source override or Windows Update download. May fail if local repair content is unavailable; no automatic rollback. Independent /ScanHealth checks component-store health afterward."),
            Self::SfcRepair => (Risk::SystemRepair, 7200, "Runs SFC /scannow; may replace protected system files and require a later owner-initiated reboot. No automatic rollback. Independent /verifyonly is run, but localized CBS results require review."),
            Self::DefenderQuickScan => (Risk::AntivirusRemediation, 1800, "Defender quick scan may quarantine/remediate threats and use cloud protection according to existing Defender preferences. No exclusions or protection settings are changed. Completion is checked independently against quick-scan timestamps; it does not establish threat absence."),
        };
        OperationSpec {
            kind: self,
            risk,
            timeout_seconds,
            downloads: false,
            network_access: self == Self::DefenderQuickScan,
            may_require_reboot: risk != Risk::DiagnosticIo,
            reversibility: if risk == Risk::DiagnosticIo {
                Reversibility::NoConfigurationChange
            } else {
                Reversibility::NoAutomaticRollback
            },
            explanation: explanation.into(),
        }
    }
    fn diagnostic(self) -> bool {
        self.spec().risk == Risk::DiagnosticIo
    }
}

impl std::str::FromStr for OperationKind {
    type Err = anyhow::Error;
    fn from_str(text: &str) -> Result<Self> {
        match text {
            "dism_check_health" => Ok(Self::DismCheckHealth),
            "dism_scan_health" => Ok(Self::DismScanHealth),
            "dism_restore_health" => Ok(Self::DismRestoreHealth),
            "sfc_verify" => Ok(Self::SfcVerify),
            "sfc_repair" => Ok(Self::SfcRepair),
            "defender_quick_scan" => Ok(Self::DefenderQuickScan),
            _ => bail!("Unknown allowlisted maintenance operation"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub windows_execution: bool,
    pub operations: Vec<OperationSpec>,
    pub selected_app_upgrades: bool,
    pub windows_quality_updates: bool,
    pub unsupported_reason: String,
}

pub fn capabilities() -> Capabilities {
    Capabilities {
        windows_execution: cfg!(all(windows, target_arch = "x86_64")),
        operations: [OperationKind::DismCheckHealth, OperationKind::DismScanHealth,
            OperationKind::DismRestoreHealth, OperationKind::SfcVerify,
            OperationKind::SfcRepair, OperationKind::DefenderQuickScan]
            .into_iter().map(OperationKind::spec).collect(),
        selected_app_upgrades: false, windows_quality_updates: false,
        unsupported_reason: "Exact-version/source-bound app upgrades with original-user execution and exact quality-update identity/ownership verification are not implemented. No package action, upgrade --all, update install or reboot is accepted.".into(),
    }
}

/// UTC minutes since midnight, half-open; a wrapping interval crosses midnight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceWindow {
    pub start_minute_utc: u16,
    pub end_minute_utc: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExceptionScope {
    ActiveUse,
    MaintenanceWindow,
    MeteredNetwork,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyException {
    pub operation: OperationKind,
    pub scope: ExceptionScope,
    pub expires_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerPolicy {
    pub allowed: Vec<OperationKind>,
    pub opt_in_until: Option<u64>,
    pub window: MaintenanceWindow,
    pub idle_seconds: u32,
    pub exceptions: Vec<PolicyException>,
}

impl Default for OwnerPolicy {
    fn default() -> Self {
        Self {
            allowed: vec![
                OperationKind::DismCheckHealth,
                OperationKind::DismScanHealth,
                OperationKind::SfcVerify,
            ],
            opt_in_until: None,
            window: MaintenanceWindow {
                start_minute_utc: 60,
                end_minute_utc: 300,
            },
            idle_seconds: 300,
            exceptions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    pub operations: Vec<OperationKind>,
    pub valid_for_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanStep {
    pub operation: OperationSpec,
    /// Indices into the exact ordered plan; all dependencies must complete first.
    pub depends_on: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: u32,
    pub id: Uuid,
    pub machine: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub policy: OwnerPolicy,
    pub steps: Vec<PlanStep>,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    pub digest: String,
    pub approved_at: u64,
    pub expires_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Pending,
    Intent,
    Running,
    Monitoring,
    Verifying,
    Succeeded,
    RebootRequired,
    NeedsReview,
    Failed,
    Cancelled,
}
impl StepState {
    fn uncertain(self) -> bool {
        matches!(
            self,
            Self::Intent
                | Self::Running
                | Self::Monitoring
                | Self::Verifying
                | Self::RebootRequired
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    CancelRequested,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    DiagnosticCompleted,
    ComponentStoreHealthy,
    ComponentStoreRepairable,
    ComponentStoreNonRepairable,
    DefenderScanCompleted,
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub creation_time: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    pub captured_at: u64,
    pub boot_time: u64,
    pub defender_scan_end: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepRecord {
    pub state: StepState,
    pub baseline: Option<Baseline>,
    pub process: Option<ProcessIdentity>,
    pub exit_code: Option<u32>,
    pub evidence: Option<Evidence>,
    pub stop_reason: Option<StopReason>,
}
impl Default for StepRecord {
    fn default() -> Self {
        Self {
            state: StepState::Pending,
            baseline: None,
            process: None,
            exit_code: None,
            evidence: None,
            stop_reason: None,
        }
    }
}
impl StepRecord {
    fn unresolved(&self) -> bool {
        self.state.uncertain()
            || (self.state == StepState::NeedsReview
                && matches!(self.evidence, None | Some(Evidence::Inconclusive)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRecord {
    pub plan: Plan,
    pub approval: Option<Approval>,
    /// Once execution begins the plan is single-use, even if it is interrupted.
    pub consumed: bool,
    pub steps: Vec<StepRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub plan_id: Uuid,
    pub step: Option<usize>,
    pub state: Option<StepState>,
    pub elapsed_seconds: u64,
    pub stop_reason: Option<StopReason>,
}

/// The worker retains engine.lock and protected namespace pins while any process
/// in its servicing job is alive. A wait timeout only detaches the caller.
/// Dropping the Task requests cooperative cancellation of subsequent steps.
pub struct Task {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
    result: mpsc::Receiver<Result<PlanRecord>>,
}
impl Task {
    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn progress(&self) -> Result<Progress> {
        Ok(self
            .progress
            .lock()
            .map_err(|_| anyhow::anyhow!("Progress lock poisoned"))?
            .clone())
    }
    pub fn wait(&self, timeout: Duration) -> Result<Option<PlanRecord>> {
        match self.result.recv_timeout(timeout) {
            Ok(result) => result.map(Some),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("Operations worker ended or result already consumed; inspect durable state and resume verification"),
        }
    }
}
impl Drop for Task {
    fn drop(&mut self) {
        self.request_cancel();
    }
}

fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
fn digest<T: Serialize>(value: &T) -> Result<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(value)?)))
}

/// Reading protected state also requires elevation. Busy lock errors are
/// deferrals, not authorization to use a second store or bypass engine.lock.
pub fn policy() -> Result<OwnerPolicy> {
    native(|e| Ok(e.state.policy.clone()))
}
pub fn set_policy(policy: OwnerPolicy) -> Result<()> {
    native(|e| e.set_policy(policy))
}
pub fn plan(request: PlanRequest) -> Result<Plan> {
    native(|e| e.plan(request))
}
pub fn approve(id: Uuid, displayed_digest: &str, valid_for_seconds: u64) -> Result<PlanRecord> {
    native(|e| e.approve(id, displayed_digest, valid_for_seconds))
}
pub fn list() -> Result<Vec<PlanRecord>> {
    native(|e| Ok(e.state.plans.clone()))
}
pub fn get(id: Uuid) -> Result<PlanRecord> {
    native(|e| Ok(e.record(id)?.clone()))
}

/// Updater integration: call AFTER acquiring the shared root engine.lock, and
/// retain that lock throughout installation. This read-only check does not
/// acquire another lock. It rejects unresolved durable operations after a lost
/// supervisor as well as independently observed servicing processes. The file
/// argument must be the updater's locked engine.lock handle, not a path.
pub fn ensure_update_idle(shared_engine_lock: &std::fs::File) -> Result<()> {
    #[cfg(windows)]
    {
        storage::ensure_update_idle(shared_engine_lock)
    }
    #[cfg(not(windows))]
    {
        let _ = shared_engine_lock;
        bail!("Maintenance/update interlock requires Windows")
    }
}

#[cfg(windows)]
fn native<T>(
    f: impl FnOnce(&mut core::Engine<storage::Store, windows::Backend>) -> Result<T>,
) -> Result<T> {
    let store = storage::Store::open()?;
    let backend = windows::Backend::new(store.root())?;
    f(&mut core::Engine::open(store, backend)?)
}
#[cfg(not(windows))]
fn native<T>(
    _f: impl FnOnce(&mut core::Engine<core::Unsupported, core::Unsupported>) -> Result<T>,
) -> Result<T> {
    bail!("Maintenance execution and protected records require Windows x64")
}

pub fn start(id: Uuid) -> Result<Task> {
    spawn(id, false)
}
/// Independent read-only verification of previously attempted steps, even when
/// approval expired. Never executes pending steps or repeats the original repair.
/// Failed verification retains a blocking `Verifying` state. An unacknowledged
/// launch retains Intent/Running/Monitoring until independent recovery succeeds.
pub fn resume(id: Uuid) -> Result<Task> {
    spawn(id, true)
}
fn spawn(id: Uuid, recovery: bool) -> Result<Task> {
    #[cfg(windows)]
    {
        let store = storage::Store::open()?;
        let backend = windows::Backend::new(store.root())?;
        let mut engine = core::Engine::open(store, backend)?;
        engine.record(id)?;
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new(Progress {
            plan_id: id,
            step: None,
            state: None,
            elapsed_seconds: 0,
            stop_reason: None,
        }));
        let (tx, result) = mpsc::sync_channel(1);
        let control = core::Control {
            cancel: cancel.clone(),
            progress: progress.clone(),
        };
        std::thread::Builder::new()
            .name("maintenance-supervisor".into())
            .spawn(move || {
                let outcome = engine.run(id, recovery, &control);
                let _ = tx.send(outcome);
            })?;
        Ok(Task {
            cancel,
            progress,
            result,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = (id, recovery);
        bail!("Maintenance execution requires Windows x64")
    }
}
