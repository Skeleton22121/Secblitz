//! Background wrappers for the Tools page: Windows repair (operations),
//! Windows updates (patching), PC health tips (diagnostics) and the password
//! generator.
//!
//! OWNER: tools agent. Nothing here draws pixels. The `run_*` / `discover_*`
//! functions are BLOCKING and must be called from `crate::gui::blocking` or
//! `crate::gui::blocking_stream`. Every user-visible string is an English
//! translation source key (the page passes it through `ctx.t`); raw technical
//! evidence is only ever returned in `technical` fields for the collapsed
//! "Technical details" expander.
use anyhow::{bail, ensure, Context as _, Result};
use secblitz::diagnostics as diag;
use secblitz::operations::{self as ops, OperationKind as Op};
use secblitz::patching as patch;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Password generator (ported from the former terminal UI)
// ---------------------------------------------------------------------------

pub const PASSWORD_LENGTH: usize = 24;

/// 64 distinct characters: every six-bit value is equally likely, so there is
/// no modulo bias. No character-class repair, predictable seed or logging.
pub fn encode_password(random: &[u8; PASSWORD_LENGTH]) -> [u8; PASSWORD_LENGTH] {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    random.map(|byte| ALPHABET[(byte & 63) as usize])
}

fn wipe(bytes: &mut [u8]) {
    for byte in bytes {
        // SAFETY: `byte` is a valid, exclusive reference.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
}

/// A generated password. Never printed by `Debug`, wiped when dropped.
pub struct Secret([u8; PASSWORD_LENGTH]);

impl Secret {
    pub fn generate() -> Result<Self> {
        use rand::RngCore;
        let mut random = [0u8; PASSWORD_LENGTH];
        let filled = rand::rngs::OsRng
            .try_fill_bytes(&mut random)
            .map_err(|e| anyhow::anyhow!("{e}"));
        let encoded = encode_password(&random);
        wipe(&mut random);
        filled.context("Password generation failed")?;
        Ok(Self(encoded))
    }
    /// The password text (always printable ASCII).
    pub fn reveal(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<hidden>)")
    }
}

// ---------------------------------------------------------------------------
// Friendly errors
// ---------------------------------------------------------------------------

/// Map a raw engine error to one calm sentence (translation key).
///
/// Short words are matched as whole words, so "lock" does not fire on
/// "blocked" or "clock" and "source" does not fire on "resource". Policy is
/// checked before busy/network because a message such as "blocked by policy"
/// will never succeed on a retry.
pub const ERR_USE_WINDOWS_UPDATE: &str =
    "Updates can't be installed from this account. Open Windows Update to install them.";
pub const ERR_UNAVAILABLE: &str = "This isn't available on this PC.";
pub const ERR_SETTINGS_BLOCK: &str = "Your PC's settings don't allow this.";

/// False when trying again cannot help (the GUI should show a different next
/// step instead of Retry). For `ERR_USE_WINDOWS_UPDATE` the next step is an
/// "Open Windows Update" button (`advice::NextStep::OpenWindowsUpdate`).
#[allow(dead_code)] // consumed by the GUI integration
pub fn is_retryable(note: &str) -> bool {
    !matches!(
        note,
        ERR_USE_WINDOWS_UPDATE | ERR_UNAVAILABLE | ERR_SETTINGS_BLOCK
    )
}

/// True when the friendly note says to finish in Windows Update itself.
#[allow(dead_code)] // consumed by the GUI integration
pub fn suggests_windows_update(note: &str) -> bool {
    note == ERR_USE_WINDOWS_UPDATE
}

pub fn friendly_error(raw: &str) -> &'static str {
    let r = raw.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| r.contains(n));
    let words: Vec<&str> = r
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|w| !w.is_empty())
        .collect();
    let word = |needles: &[&str]| words.iter().any(|w| needles.contains(w));
    // Deliberate security property: only a real, interactive split-token
    // administrator may install updates (the built-in Administrator may not).
    if has(&["split-token"]) {
        ERR_USE_WINDOWS_UPDATE
    } else if has(&["requires windows", "not implemented", "unsupported"]) {
        ERR_UNAVAILABLE
    } else if has(&["reboot", "restart"]) {
        "Restart your PC, then try again."
    } else if has(&["deferred", "readiness", "not ready", "stale", "ac/storage"]) {
        "Your PC isn't ready for this right now. Plug it in, save your work, restart if Windows is waiting, then try again."
    } else if has(&["unresolved", "independent verification", "interrupted"]) {
        "An earlier repair or update still needs to be checked. Restart Secblitz and try again."
    } else if word(&["policy", "opt-in", "managed", "ownership"]) || has(&["not enabled"]) {
        ERR_SETTINGS_BLOCK
    } else if word(&["busy", "lock", "locked", "contention"])
        || has(&[
            "another operation",
            "another install",
            "another update",
            "another instance",
            "already running",
        ])
    {
        "Windows is busy with another task. Try again in a few minutes."
    } else if word(&["network", "offline", "internet", "source"])
        || words
            .iter()
            .any(|w| w.starts_with("0x8024") || w.starts_with("0x8007"))
        || has(&["timed out"])
    {
        "We couldn't reach Windows Update. Check your internet connection and try again."
    } else if has(&["elevation", "elevated", "administrator", "interactive"]) {
        "Reopen Secblitz from its shortcut and try again."
    } else {
        "We couldn't finish this. Try again in a few minutes."
    }
}

fn unix_now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

// ---------------------------------------------------------------------------
// Repair Windows (operations)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairKind {
    /// Look for problems; changes nothing.
    Check,
    /// Repair Windows system files.
    Repair,
}

/// Plan lifetime (seconds). Approval has its own 15 minute maximum.
pub const PLAN_SECONDS: u64 = 3600;
pub const APPROVAL_SECONDS: u64 = 900;
/// How long the short permission granted from the review sheet lasts.
pub const GRANT_SECONDS: u64 = 2 * 3600;

impl RepairKind {
    /// Separate plans, run one after another. The engine lets an approval live
    /// at most 15 minutes and re-checks it before every step, so long jobs get
    /// a fresh, exact approval per part instead of one plan that would expire.
    pub fn chunks(self) -> Vec<Vec<Op>> {
        match self {
            Self::Check => vec![vec![Op::DismCheckHealth], vec![Op::SfcVerify]],
            Self::Repair => vec![vec![Op::DismRestoreHealth], vec![Op::SfcRepair]],
        }
    }
    /// Every operation the job may run, including automatic prerequisites.
    pub fn all_operations(self) -> Vec<Op> {
        let mut all = Vec::new();
        for chunk in self.chunks() {
            for kind in expand(&chunk) {
                if !all.contains(&kind) {
                    all.push(kind);
                }
            }
        }
        all
    }
}

/// The engine adds prerequisites to a plan; mirror its exact ordered result.
pub fn expand(requested: &[Op]) -> Vec<Op> {
    let mut selected = requested.to_vec();
    if selected.contains(&Op::DismRestoreHealth) && !selected.contains(&Op::DismScanHealth) {
        selected.push(Op::DismScanHealth);
    }
    if selected.contains(&Op::SfcRepair) && !selected.contains(&Op::SfcVerify) {
        selected.push(Op::SfcVerify);
    }
    [
        Op::DismCheckHealth,
        Op::DismScanHealth,
        Op::DismRestoreHealth,
        Op::SfcVerify,
        Op::SfcRepair,
        Op::DefenderQuickScan,
    ]
    .into_iter()
    .filter(|k| selected.contains(k))
    .collect()
}

pub fn plan_request(chunk: &[Op]) -> ops::PlanRequest {
    ops::PlanRequest {
        operations: chunk.to_vec(),
        valid_for_seconds: PLAN_SECONDS,
    }
}

/// The owner policy to use while a job the user just confirmed is running.
///
/// The review sheet is the owner's explicit consent. Defaults are respected:
/// existing entries stay, only what the job needs is added, the opt-in and the
/// scoped "this PC is in use" exceptions expire after `GRANT_SECONDS`, and the
/// caller restores the previous policy afterwards.
pub fn policy_for(current: &ops::OwnerPolicy, needed: &[Op], now: u64) -> ops::OwnerPolicy {
    use ops::{ExceptionScope, PolicyException, Risk};
    let mut policy = current.clone();
    for kind in needed {
        if !policy.allowed.contains(kind) {
            policy.allowed.push(*kind);
        }
    }
    let changes_system = needed.iter().any(|k| k.spec().risk != Risk::DiagnosticIo);
    if changes_system {
        let until = now + GRANT_SECONDS;
        if policy.opt_in_until.is_none_or(|t| t < until) {
            policy.opt_in_until = Some(until);
        }
    }
    const SCOPES: [ExceptionScope; 2] =
        [ExceptionScope::ActiveUse, ExceptionScope::MaintenanceWindow];
    policy.exceptions.retain(|e| {
        e.expires_at > now && !(needed.contains(&e.operation) && SCOPES.contains(&e.scope))
    });
    for kind in needed {
        for scope in SCOPES {
            policy.exceptions.push(PolicyException {
                operation: *kind,
                scope,
                expires_at: now + GRANT_SECONDS,
            });
        }
    }
    policy
}

/// Plain name for the part currently running (translation key).
pub fn step_label(kind: Op) -> &'static str {
    match kind {
        Op::DismCheckHealth => "Taking a quick look at Windows",
        Op::DismScanHealth => "Scanning Windows for problems",
        Op::DismRestoreHealth => "Repairing Windows",
        Op::SfcVerify => "Checking your system files",
        Op::SfcRepair => "Repairing your system files",
        Op::DefenderQuickScan => "Scanning for viruses",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairResult {
    NoProblems,
    ProblemsFound,
    Repaired,
    NeedsRestart,
    Stopped,
    CouldNotFinish,
}

impl RepairResult {
    pub fn title(self) -> &'static str {
        match self {
            Self::NoProblems => "No problems found",
            Self::ProblemsFound => "Some problems were found",
            Self::Repaired => "Problems were repaired",
            Self::NeedsRestart => "Almost done. Please restart your PC.",
            Self::Stopped => "You stopped the repair",
            Self::CouldNotFinish => "We couldn't finish",
        }
    }
    pub fn detail(self) -> &'static str {
        match self {
            Self::NoProblems => "Windows looks healthy.",
            Self::ProblemsFound => "You can use Repair system files to fix them.",
            Self::Repaired => "Windows is healthy again.",
            Self::NeedsRestart => "Windows needs a restart to finish the repair.",
            Self::Stopped => "Nothing else was started. You can run it again any time.",
            Self::CouldNotFinish => "Nothing is broken by this. Try again in a few minutes.",
        }
    }
}

fn step_unresolved(step: &ops::StepRecord) -> bool {
    use ops::StepState as S;
    matches!(
        step.state,
        S::Intent | S::Running | S::Monitoring | S::Verifying | S::RebootRequired
    ) || (step.state == S::NeedsReview
        && matches!(step.evidence, None | Some(ops::Evidence::Inconclusive)))
}

/// Turn the finished plan records into one plain outcome.
pub fn classify(kind: RepairKind, records: &[ops::PlanRecord], stopped: bool) -> RepairResult {
    use ops::StepState as S;
    let steps: Vec<(Op, &ops::StepRecord)> = records
        .iter()
        .flat_map(|r| {
            r.plan
                .steps
                .iter()
                .map(|s| s.operation.kind)
                .zip(r.steps.iter())
        })
        .collect();
    if steps.iter().any(|(_, s)| s.state == S::Failed) {
        return RepairResult::CouldNotFinish;
    }
    if steps.iter().any(|(_, s)| s.state == S::RebootRequired) {
        return RepairResult::NeedsRestart;
    }
    if stopped {
        return RepairResult::Stopped;
    }
    let expected = kind.all_operations();
    let ran = |op: Op| steps.iter().any(|(k, _)| *k == op);
    if records.is_empty() || !expected.iter().all(|op| ran(*op)) {
        return RepairResult::CouldNotFinish;
    }
    match kind {
        RepairKind::Check => {
            if steps.iter().any(|(_, s)| s.state != S::Succeeded) {
                return RepairResult::CouldNotFinish;
            }
            let damaged = steps.iter().any(|(_, s)| {
                matches!(
                    s.evidence,
                    Some(ops::Evidence::ComponentStoreRepairable)
                        | Some(ops::Evidence::ComponentStoreNonRepairable)
                )
            });
            if damaged {
                RepairResult::ProblemsFound
            } else {
                RepairResult::NoProblems
            }
        }
        RepairKind::Repair => {
            // The engine cannot prove a system-file repair by itself, so a
            // finished (not failed) repair always ends with a restart prompt.
            let only_unproven_files = steps.iter().all(|(op, s)| {
                s.state == S::Succeeded
                    || (*op == Op::SfcRepair
                        && s.state == S::NeedsReview
                        && s.evidence == Some(ops::Evidence::DiagnosticCompleted))
            });
            if !only_unproven_files {
                RepairResult::CouldNotFinish
            } else if steps.iter().all(|(_, s)| s.state == S::Succeeded) {
                RepairResult::Repaired
            } else {
                RepairResult::NeedsRestart
            }
        }
    }
}

/// Raw evidence for the collapsed "Technical details" expander.
pub fn technical_lines(records: &[ops::PlanRecord]) -> String {
    let mut out = String::new();
    for record in records {
        for (step, state) in record.plan.steps.iter().zip(&record.steps) {
            out.push_str(&format!(
                "{}: {:?}, exit code {}, evidence {}\n",
                step.operation.kind.as_str(),
                state.state,
                state.exit_code.map_or("none".into(), |c| c.to_string()),
                state.evidence.map_or("none".into(), |e| format!("{e:?}")),
            ));
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct RepairProgress {
    /// Translation key describing the part now running.
    pub label: &'static str,
    /// 1-based position and total, for a calm "Step 2 of 4".
    pub step: usize,
    pub total: usize,
    pub elapsed: u64,
}

#[derive(Debug, Clone)]
pub enum RepairEvent {
    /// Getting everything ready (before the first part starts).
    Preparing,
    Progress(RepairProgress),
    Done {
        result: RepairResult,
        /// Friendly reason when the job could not run at all.
        note: Option<&'static str>,
        technical: String,
    },
}

/// Run a whole repair job. Blocking; emits events; always ends with `Done`.
pub fn run_repair(kind: RepairKind, cancel: Arc<AtomicBool>, emit: &dyn Fn(RepairEvent)) {
    emit(RepairEvent::Preparing);
    let mut records = Vec::new();
    let outcome = repair_flow(kind, &cancel, emit, &mut records);
    let stopped = cancel.load(Ordering::SeqCst);
    let mut technical = technical_lines(&records);
    let (result, note) = match outcome {
        Ok(()) => (classify(kind, &records, stopped), None),
        Err(e) => {
            let raw = format!("{e:#}");
            technical.push_str(&format!("error: {raw}\n"));
            let result = if stopped && records.is_empty() {
                RepairResult::Stopped
            } else {
                RepairResult::CouldNotFinish
            };
            (result, Some(friendly_error(&raw)))
        }
    };
    emit(RepairEvent::Done {
        result,
        note,
        technical,
    });
}

fn repair_flow(
    kind: RepairKind,
    cancel: &AtomicBool,
    emit: &dyn Fn(RepairEvent),
    records: &mut Vec<ops::PlanRecord>,
) -> Result<()> {
    let all = kind.all_operations();
    let original = ops::policy()?;
    let wanted = policy_for(&original, &all, unix_now()?);
    let changed = wanted != original;
    if changed {
        ops::set_policy(wanted)?;
    }
    let result = repair_steps(kind, cancel, emit, records, all.len());
    if changed {
        // Back to the owner's own settings. Best effort: the short grant
        // expires by itself anyway.
        let _ = ops::set_policy(original);
    }
    result
}

fn repair_steps(
    kind: RepairKind,
    cancel: &AtomicBool,
    emit: &dyn Fn(RepairEvent),
    records: &mut Vec<ops::PlanRecord>,
    total: usize,
) -> Result<()> {
    // An earlier repair or update that was cut short must be verified before a new one.
    for old in ops::list()? {
        if old.consumed && old.steps.iter().any(step_unresolved) {
            let kinds: Vec<Op> = old.plan.steps.iter().map(|s| s.operation.kind).collect();
            let task = ops::resume(old.plan.id)?;
            supervise_operation(&task, cancel, emit, &kinds, 0, total)?;
        }
    }
    let mut done = 0;
    for chunk in kind.chunks() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let kinds = expand(&chunk);
        let plan = ops::plan(plan_request(&chunk))?;
        let planned: Vec<Op> = plan.steps.iter().map(|s| s.operation.kind).collect();
        ensure!(
            planned == kinds,
            "The prepared plan did not match the request"
        );
        let approved = ops::approve(plan.id, &plan.digest, APPROVAL_SECONDS)?;
        ensure!(
            approved.plan.digest == plan.digest
                && approved
                    .approval
                    .as_ref()
                    .is_some_and(|a| a.digest == plan.digest)
                && !approved.consumed,
            "The approval did not match the prepared plan"
        );
        let task = ops::start(plan.id)?;
        let mut record = supervise_operation(&task, cancel, emit, &kinds, done, total)?;
        if record.steps.iter().any(step_unresolved) && !cancel.load(Ordering::SeqCst) {
            if let Ok(task) = ops::resume(plan.id) {
                if let Ok(verified) = supervise_operation(&task, cancel, emit, &kinds, done, total)
                {
                    record = verified;
                }
            }
        }
        done += record.steps.len();
        let finished = record
            .steps
            .iter()
            .all(|s| s.state == ops::StepState::Succeeded);
        records.push(record);
        if !finished {
            break;
        }
    }
    Ok(())
}

fn supervise_operation(
    task: &ops::Task,
    cancel: &AtomicBool,
    emit: &dyn Fn(RepairEvent),
    kinds: &[Op],
    done: usize,
    total: usize,
) -> Result<ops::PlanRecord> {
    let started = Instant::now();
    let mut last = Instant::now() - Duration::from_secs(1);
    let mut cancelled = false;
    loop {
        if cancel.load(Ordering::SeqCst) && !cancelled {
            task.request_cancel();
            cancelled = true;
        }
        if let Some(record) = task.wait(Duration::from_millis(500))? {
            return Ok(record);
        }
        if last.elapsed() >= Duration::from_secs(1) {
            last = Instant::now();
            let index = task.progress().ok().and_then(|p| p.step).unwrap_or(0);
            let kind = kinds
                .get(index)
                .or(kinds.first())
                .copied()
                .unwrap_or(Op::DismCheckHealth);
            emit(RepairEvent::Progress(RepairProgress {
                label: step_label(kind),
                step: (done + index + 1).min(total.max(1)),
                total: total.max(1),
                elapsed: started.elapsed().as_secs(),
            }));
        }
    }
}

// ---------------------------------------------------------------------------
// Windows updates (patching)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub identity: patch::UpdateIdentity,
    /// Title as published by Microsoft (shown in the review sheet).
    pub title: String,
    pub size_bytes: u64,
    pub license: String,
}

#[derive(Debug, Clone, Default)]
pub struct Found {
    pub updates: Vec<UpdateInfo>,
    pub technical: String,
}

impl Found {
    pub fn identities(&self) -> Vec<patch::UpdateIdentity> {
        self.updates.iter().map(|u| u.identity.clone()).collect()
    }
    pub fn total_bytes(&self) -> u64 {
        self.updates.iter().map(|u| u.size_bytes).sum()
    }
}

/// Reduce a discovered catalog to what the page shows.
pub fn summarize_catalog(catalog: &patch::Catalog) -> Found {
    let mut technical = format!("Source: {}\n", catalog.source);
    let updates = catalog
        .updates
        .iter()
        .map(|u| {
            let kb = u.kb_articles.first().cloned().unwrap_or_default();
            technical.push_str(&format!(
                "{} (KB {}) severity {} bundled {}\n",
                u.title,
                kb,
                if u.severity.is_empty() {
                    "n/a"
                } else {
                    &u.severity
                },
                u.bundled.len()
            ));
            UpdateInfo {
                identity: u.identity.clone(),
                title: u.title.clone(),
                size_bytes: u.max_download_bytes,
                license: u.eula.clone(),
            }
        })
        .collect();
    Found { updates, technical }
}

/// "230 MB" style size in plain words.
pub fn size_phrase(bytes: u64) -> String {
    const MB: u64 = 1024 * 1024;
    if bytes == 0 {
        String::new()
    } else if bytes < 1024 * MB {
        format!("{} MB", bytes.div_ceil(MB).max(1))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * MB as f64))
    }
}

/// Blocking: look for important Windows updates (reads from Windows Update).
pub fn discover_updates() -> Result<Found, (String, &'static str)> {
    let fail = |e: anyhow::Error| {
        let raw = format!("{e:#}");
        let note = friendly_error(&raw);
        (raw, note)
    };
    let task = patch::discover().map_err(fail)?;
    loop {
        if let Some(catalog) = task.wait(Duration::from_millis(500)).map_err(fail)? {
            return Ok(summarize_catalog(&catalog));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallResult {
    Installed,
    NeedsRestart,
    NotConfirmed,
    Stopped,
    CouldNotFinish,
}

impl InstallResult {
    pub fn title(self) -> &'static str {
        match self {
            Self::Installed => "Your updates are installed",
            Self::NeedsRestart => "Almost done. Please restart your PC.",
            Self::NotConfirmed => "We couldn't confirm every update",
            Self::Stopped => "You stopped the update",
            Self::CouldNotFinish => "We couldn't finish",
        }
    }
    pub fn detail(self) -> &'static str {
        match self {
            Self::Installed => "Your PC has the latest important updates.",
            Self::NeedsRestart => "Windows needs a restart to finish installing.",
            Self::NotConfirmed => "Open Windows Update to see what is left.",
            Self::Stopped => "Nothing else was started. You can run it again any time.",
            Self::CouldNotFinish => "Try again in a few minutes.",
        }
    }
}

/// The plan must contain exactly the updates the person reviewed.
pub fn plan_matches(reviewed: &[patch::UpdateIdentity], plan: &patch::Plan) -> bool {
    let mut a: Vec<_> = reviewed.to_vec();
    let mut b: Vec<_> = plan.updates.iter().map(|u| u.identity.clone()).collect();
    a.sort();
    b.sort();
    !a.is_empty() && a == b
}

pub fn classify_install(record: &patch::Record, stopped: bool) -> InstallResult {
    use patch::Status as S;
    match record.status {
        S::RebootRequired => InstallResult::NeedsRestart,
        S::Succeeded => {
            let confirmed = record.verification.as_ref().is_some_and(|v| {
                record
                    .plan
                    .updates
                    .iter()
                    .all(|u| v.installed.contains(&u.identity))
            });
            if confirmed {
                InstallResult::Installed
            } else {
                InstallResult::NotConfirmed
            }
        }
        S::NeedsReview => InstallResult::NotConfirmed,
        _ if stopped => InstallResult::Stopped,
        _ => InstallResult::CouldNotFinish,
    }
}

fn install_in_flight(status: patch::Status) -> bool {
    use patch::Status as S;
    matches!(
        status,
        S::Consumed | S::Downloading | S::Downloaded | S::Installing | S::Verifying
    )
}

pub fn consent_for_sheet() -> patch::Consent {
    // Granted only after the person confirms the review sheet, which states
    // the source, the license terms and that updates cannot be undone.
    patch::Consent {
        owner_opt_in: true,
        accept_windows_update_source: true,
        accept_reviewed_eulas: true,
        acknowledge_no_automatic_rollback: true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStage {
    Preparing,
    Installing,
    Checking,
}

impl InstallStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Preparing => "Getting ready",
            Self::Installing => "Downloading and installing updates",
            Self::Checking => "Making sure everything installed",
        }
    }
}

#[derive(Debug, Clone)]
pub enum InstallEvent {
    Stage {
        stage: InstallStage,
        elapsed: u64,
    },
    Done {
        result: InstallResult,
        note: Option<&'static str>,
        technical: String,
    },
}

pub fn run_install(
    reviewed: Vec<patch::UpdateIdentity>,
    cancel: Arc<AtomicBool>,
    emit: &dyn Fn(InstallEvent),
) {
    let mut technical = String::new();
    let outcome = install_flow(&reviewed, &cancel, emit, &mut technical);
    let stopped = cancel.load(Ordering::SeqCst);
    let (result, note) = match outcome {
        Ok(record) => {
            technical.push_str(&format!("status: {:?}\n", record.status));
            (classify_install(&record, stopped), None)
        }
        Err(e) => {
            let raw = format!("{e:#}");
            technical.push_str(&format!("error: {raw}\n"));
            (
                if stopped {
                    InstallResult::Stopped
                } else {
                    InstallResult::CouldNotFinish
                },
                Some(friendly_error(&raw)),
            )
        }
    };
    emit(InstallEvent::Done {
        result,
        note,
        technical,
    });
}

fn install_flow(
    reviewed: &[patch::UpdateIdentity],
    cancel: &AtomicBool,
    emit: &dyn Fn(InstallEvent),
    technical: &mut String,
) -> Result<patch::Record> {
    ensure!(
        (1..=32).contains(&reviewed.len()),
        "Select between 1 and 32 updates"
    );
    // Verify anything an earlier, interrupted job left behind (read-only).
    for old in patch::list()? {
        if install_in_flight(old.status) {
            let task = patch::verify(old.plan.id)?;
            wait_patch(&task, cancel, emit, InstallStage::Checking)?;
        }
    }
    let plan = {
        let task = patch::plan(patch::PlanRequest {
            selected: reviewed.to_vec(),
            valid_for_seconds: PLAN_SECONDS,
        })?;
        wait_patch(&task, cancel, emit, InstallStage::Preparing)?
    };
    technical.push_str(&format!("plan: {} updates\n", plan.updates.len()));
    if !plan_matches(reviewed, &plan) {
        bail!("The updates changed since they were reviewed; look again");
    }
    patch::approve(plan.id, &plan.digest, consent_for_sheet(), APPROVAL_SECONDS)?;
    let task = patch::start(plan.id, &plan.digest)?;
    let mut record = wait_patch(&task, cancel, emit, InstallStage::Installing)?;
    if install_in_flight(record.status) || record.uncertain && record.verification.is_none() {
        if let Ok(task) = patch::verify(plan.id) {
            if let Ok(verified) = wait_patch(&task, cancel, emit, InstallStage::Checking) {
                record = verified;
            }
        }
    }
    Ok(record)
}

fn wait_patch<T>(
    task: &patch::Task<T>,
    cancel: &AtomicBool,
    emit: &dyn Fn(InstallEvent),
    stage: InstallStage,
) -> Result<T> {
    let started = Instant::now();
    let mut last = Instant::now() - Duration::from_secs(1);
    let mut cancelled = false;
    loop {
        if cancel.load(Ordering::SeqCst) && !cancelled {
            task.request_cancel();
            cancelled = true;
        }
        if let Some(value) = task.wait(Duration::from_millis(500))? {
            return Ok(value);
        }
        if last.elapsed() >= Duration::from_secs(1) {
            last = Instant::now();
            emit(InstallEvent::Stage {
                stage,
                elapsed: started.elapsed().as_secs(),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// PC health tips (diagnostics)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipProfile {
    Everyday,
    Gaming,
    Work,
    Extra,
}

impl TipProfile {
    pub const ALL: [TipProfile; 4] = [Self::Everyday, Self::Gaming, Self::Work, Self::Extra];
    pub fn title(self) -> &'static str {
        match self {
            Self::Everyday => "Everyday use",
            Self::Gaming => "Gaming",
            Self::Work => "Work & development",
            Self::Extra => "Extra security",
        }
    }
    pub fn blurb(self) -> &'static str {
        match self {
            Self::Everyday => "Browsing, email and video calls",
            Self::Gaming => "Smooth play and a healthy PC",
            Self::Work => "Coding, remote work and shared files",
            Self::Extra => "The strongest protection Windows offers",
        }
    }
    pub fn profile(self) -> diag::Profile {
        match self {
            Self::Everyday => diag::Profile::Everyday,
            Self::Gaming => diag::Profile::Gaming,
            Self::Work => diag::Profile::Development,
            Self::Extra => diag::Profile::HigherSecurity,
        }
    }
    fn probes(self) -> &'static [diag::ProbeId] {
        use diag::ProbeId as P;
        match self {
            Self::Everyday => &[
                P::UpdateCache,
                P::DefenderHealth,
                P::SecurityProviders,
                P::BitLocker,
                P::Backup,
                P::Storage,
                P::Ntfs,
                P::Accounts,
                P::OsSupport,
                P::SecureBootCerts,
                P::DefenderProtection,
                P::SmartScreen,
                P::UpdatePolicy,
                P::HostsFile,
                P::Autostart,
                P::AccountSetup,
            ],
            Self::Gaming => &[
                P::UpdateCache,
                P::DefenderHealth,
                P::SecurityProviders,
                P::Storage,
                P::Ntfs,
                P::Adapters,
                P::Software,
                P::OsSupport,
                P::SecureBootCerts,
                P::DefenderProtection,
                P::UpdatePolicy,
                P::Autostart,
                P::WifiSecurity,
            ],
            Self::Work => &[
                P::UpdateCache,
                P::DefenderHealth,
                P::SecurityProviders,
                P::BitLocker,
                P::Backup,
                P::RemoteAccess,
                P::Accounts,
                P::Software,
                P::Vpn,
                P::OsSupport,
                P::SecureBootCerts,
                P::DefenderProtection,
                P::SmartScreen,
                P::UpdatePolicy,
                P::HostsFile,
                P::LegacyFeatures,
                P::Sharing,
                P::AccountHygiene,
                P::FirewallRules,
                P::Autostart,
                P::AccountSetup,
                P::WifiSecurity,
                P::DnsEncryption,
            ],
            Self::Extra => &[
                P::DefenderHealth,
                P::SecureBoot,
                P::Tpm,
                P::BitLocker,
                P::Vbs,
                P::WinRe,
                P::RemoteAccess,
                P::Accounts,
                P::Permissions,
                P::UpdateCache,
                P::OsSupport,
                P::SecureBootCerts,
                P::DefenderProtection,
                P::SmartScreen,
                P::UpdatePolicy,
                P::HostsFile,
                P::LegacyFeatures,
                P::Persistence,
                P::AccountHygiene,
                P::Sharing,
                P::FirewallRules,
                P::Autostart,
                P::AccountSetup,
                P::WindowsHello,
                P::WifiSecurity,
                P::DnsEncryption,
            ],
        }
    }
}

/// Plain title of one thing that was looked at (translation key).
pub fn tip_title(id: diag::ProbeId) -> &'static str {
    use diag::ProbeId as P;
    match id {
        P::UpdateCache => "Windows updates",
        P::UpdateHistory => "Recent updates",
        P::DefenderHealth => "Virus protection",
        P::DefenderPolicy => "Extra virus shields",
        P::SecurityProviders => "Security apps and firewall",
        P::Management => "Who manages this PC",
        P::SecureBoot => "Startup protection",
        P::Tpm => "Security chip",
        P::BitLocker => "Disk encryption",
        P::Vbs => "Core system protection",
        P::WinRe => "Recovery tools",
        P::Accounts => "Sign-in accounts",
        P::RemoteAccess => "Access from other PCs",
        P::Software => "Old apps",
        P::BrowserExtensions => "Browser add-ons",
        P::Storage => "Drive health",
        P::Ntfs => "Disk space and errors",
        P::Backup => "Backups",
        P::Adapters => "Network connection",
        P::Dns => "Network settings",
        P::Proxy => "Internet route",
        P::Vpn => "VPN",
        P::Permissions => "Protected services",
        P::OsSupport => "Windows version support",
        P::SecureBootCerts => "Startup security renewal",
        P::DefenderProtection => "Virus protection safeguards",
        P::SmartScreen => "Download and website warnings",
        P::UpdatePolicy => "Automatic updates",
        P::LegacyFeatures => "Old Windows tools",
        P::HostsFile => "Website redirects",
        P::Persistence => "Hidden background tasks",
        P::AccountHygiene => "Old and hidden accounts",
        P::Sharing => "Shared folders",
        P::FirewallRules => "Apps allowed through the firewall",
        P::AccountSetup => "Your everyday account",
        P::WindowsHello => "PIN and Windows Hello",
        P::DnsEncryption => "Private internet lookups",
        P::WifiSecurity => "Wi-Fi protection",
        P::Autostart => "Programs that start by themselves",
    }
}

/// One friendly suggestion shown when something needs a look.
pub fn tip_advice(id: diag::ProbeId) -> &'static str {
    use diag::ProbeId as P;
    match id {
        P::UpdateCache | P::UpdateHistory => {
            "Your PC may be missing security updates. Open Windows Update to install them."
        }
        P::DefenderHealth | P::DefenderPolicy => "Turn on and update Windows virus protection.",
        P::SecurityProviders => "Make sure one virus protection and the firewall are on.",
        P::Management => "Your PC is managed by an organization. Ask them before changing it.",
        P::SecureBoot => "Turn on Secure Boot (startup protection) in your PC's start-up settings.",
        P::Tpm => "Your security chip is off or not ready. Check your PC's start-up settings.",
        P::BitLocker => "Turn on disk encryption so your files stay private if the PC is lost.",
        P::Vbs => "Turn on Memory integrity (core system protection) in Windows Security.",
        P::WinRe => "Recovery tools are off. They help if Windows ever stops starting.",
        P::Accounts => "Use a normal account every day, and switch off the guest account.",
        P::RemoteAccess => "Switch off remote access if you don't use it.",
        P::Software => "Remove old apps that no longer get safety updates.",
        P::BrowserExtensions => "Remove browser add-ons you don't use.",
        P::Storage => "A drive is showing signs of wear. Back up your files soon.",
        P::Ntfs => "Free up disk space or check your drive for errors.",
        P::Backup => "No backup found. Set up a regular backup of your files.",
        P::Adapters | P::Dns | P::Proxy => "Check your internet connection settings.",
        P::Vpn => "Check your VPN settings.",
        P::Permissions => "Some protected services have loose settings. A fix may be available.",
        P::OsSupport => "Your Windows version is running out of safety updates. Install the newest version in Windows Update.",
        P::SecureBootCerts => "Your PC's startup security needs a renewal. Install all Windows updates and check your PC maker's website.",
        P::DefenderProtection => "Open Windows Security and check your virus protection settings.",
        P::SmartScreen => "Turn on warnings for risky downloads and websites in Windows Security.",
        P::UpdatePolicy => "Turn automatic Windows updates back on and restart your PC when asked.",
        P::LegacyFeatures => "Remove an old Windows tool that attackers like to use.",
        P::HostsFile => "A hidden file may be sending trusted websites somewhere else. Ask someone you trust to check it.",
        P::Persistence => "Something is set up to run quietly in the background. Ask someone you trust to look at it.",
        P::AccountHygiene => "Turn off hidden or unused accounts on this PC.",
        P::Sharing => "Stop sharing folders you don't need.",
        P::FirewallRules => "Some apps in your personal folders are allowed through the firewall. Remove ones you don't know.",
        P::AccountSetup => "Use a normal account every day, and turn on Find my device on a laptop.",
        P::WindowsHello => "Add a PIN or Windows Hello in Sign-in options for faster, safer sign-in.",
        P::DnsEncryption => "Your internet lookups aren't private. Turn on encrypted lookups in your network settings.",
        P::WifiSecurity => "Your Wi-Fi has weak or no protection. Switch to the newest security option on your router.",
        P::Autostart => "A risky program starts by itself with Windows. Ask someone you trust to look at it.",
    }
}

/// A more exact one-line next step for a single check, when one exists.
/// Short, calm and jargon-free; the probe-level line is the fallback.
pub fn rule_advice(rule_id: &str) -> Option<&'static str> {
    Some(match rule_id {
        "os.feature_release_support" => "Your version of Windows is running out of safety updates. Install the newest version in Windows Update.",
        "boot.secure_boot_certs" => "Your PC's startup security needs a renewal. Install all Windows updates, then check your PC maker's website.",
        "defender.tamper_protection" => "Turn on Tamper Protection so malware can't switch off your virus protection.",
        "defender.threats" => "Windows found something harmful. Open Windows Security and follow the steps.",
        "defender.exclusions_risky" => "Your virus protection skips some risky places. Look at the list in Windows Security.",
        "defender.scan_age" => "Your PC hasn't been scanned for a while. Run a quick scan in Windows Security.",
        "smartscreen.apps" => "Turn on warnings for unknown downloads in Windows Security.",
        "smartscreen.browser_policy" => "A setting has switched off your browser's warnings about dangerous sites. Ask whoever set up this PC.",
        "update.paused" => "Updates are paused. Resume them in Windows Update.",
        "update.reboot_overdue" => "Restart your PC to finish installing updates.",
        "ps.v2_engine" => "An old Windows tool that attackers like to use is still installed. Remove it in Windows Features.",
        "net.hosts_file" => "A hidden file is sending trusted websites somewhere else. Ask someone you trust to check it.",
        "persistence.wmi_subscriptions" => "Something is set to run quietly in the background. Ask someone you trust to look at it.",
        "services.unquoted_paths" => "A background program has a risky setup. Ask someone you trust to look at it.",
        "accounts.stale_enabled" => "Some old accounts are still switched on. Remove the ones nobody uses.",
        "smb.shares_exposed" => "Some folders are shared with everyone on your network. Stop sharing what you don't need.",
        "firewall.user_dir_inbound_allow" => "Apps in your Downloads or Desktop folders are allowed through the firewall. Remove ones you don't know.",
        "accounts.daily_admin" => "You use an administrator account every day. Make a normal account for daily use.",
        "accounts.hello_configured" => "No PIN or Windows Hello is set up. Add one in Sign-in options.",
        "accounts.find_my_device" => "Find my device is off. Turn it on in Settings so you can find a lost laptop.",
        "vbs.kernel_stack_protection" => "An extra shield for the core of Windows is off. Look in Core isolation in Windows Security.",
        "net.dns_encryption" => "Your internet lookups aren't private. Turn on encrypted lookups in your network settings.",
        "net.wifi_security" => "Your Wi-Fi has weak or no protection. Switch to the newest security option on your router.",
        "persistence.run_and_tasks" => "A risky program starts by itself with Windows. Ask someone you trust to look at it.",
        _ => return None,
    })
}

/// Which existing Settings page helps with this check (no new links are added here).
pub fn rule_open(rule_id: &str) -> Option<secblitz::actions::Action> {
    use secblitz::actions::Action;
    match rule_id {
        "os.feature_release_support"
        | "boot.secure_boot_certs"
        | "update.paused"
        | "update.reboot_overdue" => Some(Action::OpenWindowsUpdate),
        "defender.tamper_protection"
        | "defender.threats"
        | "defender.exclusions_risky"
        | "defender.scan_age"
        | "smartscreen.apps"
        | "smartscreen.browser_policy"
        | "vbs.kernel_stack_protection" => Some(Action::OpenWindowsSecurity),
        "accounts.stale_enabled" | "accounts.hello_configured" => Some(Action::OpenSignInSettings),
        _ => None,
    }
}

/// Checks where the Tools page can offer its existing "scan for viruses" job
/// right in the tip. Nothing new is started: the person confirms the usual sheet.
pub fn rule_scan(rule_id: &str) -> bool {
    matches!(rule_id, "defender.scan_age" | "defender.threats")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipState {
    Good,
    Look,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Tip {
    pub title: &'static str,
    pub state: TipState,
    pub advice: &'static str,
    /// Windows page that helps with a `Look` tip, shown as a button.
    pub open: Option<secblitz::actions::Action>,
    /// A `Look` tip that the Tools page's own quick scan can help with.
    pub scan: bool,
    /// Check id whose plain-language explanation the row can open: the first
    /// check that needs a look, else the first check with an explanation.
    pub explain: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TipsReport {
    pub profile: TipProfile,
    pub tips: Vec<Tip>,
    pub technical: String,
}

impl TipsReport {
    pub fn count(&self, state: TipState) -> usize {
        self.tips.iter().filter(|t| t.state == state).count()
    }
}

fn tip_state(status: diag::Status) -> TipState {
    match status {
        diag::Status::Healthy | diag::Status::Informational => TipState::Good,
        diag::Status::Attention => TipState::Look,
        diag::Status::Unknown | diag::Status::Unsupported => TipState::Unknown,
    }
}

/// Reduce a diagnostics report to a plain checklist; things to look at first.
pub fn summarize_tips(profile: TipProfile, report: &diag::Report) -> TipsReport {
    let mut tips = Vec::new();
    let mut technical = String::new();
    for &id in profile.probes() {
        let Some(probe) = report.probes.iter().find(|p| p.id == id) else {
            continue;
        };
        let mut state = tip_state(probe.status);
        if id == diag::ProbeId::UpdateCache && probe.status != diag::Status::Attention {
            // The offline cache can only add problems; whether updates are
            // current comes from the install history.
            if let Some(h) = report
                .probes
                .iter()
                .find(|p| p.id == diag::ProbeId::UpdateHistory)
            {
                state = tip_state(h.status);
                for a in &h.assessments {
                    technical.push_str(&format!("  history {:?}: {}\n", a.status, a.detail));
                }
            }
        }
        technical.push_str(&format!("{id:?}: {:?} ({})\n", probe.status, probe.source));
        for a in &probe.assessments {
            technical.push_str(&format!("  {:?}: {}\n", a.status, a.detail));
        }
        // The first check that needs a look decides the exact next step.
        let first = probe
            .assessments
            .iter()
            .filter(|a| a.status == diag::Status::Attention)
            .find_map(|a| rule_advice(&a.rule.id).map(|text| (text, rule_open(&a.rule.id))));
        let scan = probe
            .assessments
            .iter()
            .any(|a| a.status == diag::Status::Attention && rule_scan(&a.rule.id));
        let look = state == TipState::Look;
        let explain = probe
            .assessments
            .iter()
            .filter(|a| a.status == diag::Status::Attention)
            .chain(probe.assessments.iter())
            .map(|a| a.rule.id.as_str())
            .find(|rule| crate::explain::for_check(rule).is_some())
            .map(str::to_owned);
        tips.push(Tip {
            explain,
            title: tip_title(id),
            state,
            advice: match (look, first) {
                (false, _) => "",
                (true, Some((text, _))) => text,
                (true, None) => tip_advice(id),
            },
            open: match (look, first.and_then(|(_, open)| open), id) {
                (false, _, _) => None,
                (true, Some(open), _) => Some(open),
                (true, None, diag::ProbeId::UpdateCache | diag::ProbeId::UpdateHistory) => {
                    Some(secblitz::actions::Action::OpenWindowsUpdate)
                }
                (true, None, _) => None,
            },
            scan: look && scan,
        });
    }
    let rank = |s: TipState| match s {
        TipState::Look => 0,
        TipState::Unknown => 1,
        TipState::Good => 2,
    };
    tips.sort_by_key(|t| rank(t.state));
    for r in report.recommendations.iter().take(24) {
        technical.push_str(&format!("{}: {}\n", r.rule.id, r.reason));
    }
    if technical.len() > 8000 {
        technical.truncate(technical.floor_char_boundary(8000));
    }
    TipsReport {
        profile,
        tips,
        technical,
    }
}

/// Blocking: read-only collection (no changes, nothing leaves the PC).
pub fn run_tips(profile: TipProfile) -> TipsReport {
    let report = diag::collect(profile.profile(), &diag::Context::default());
    summarize_tips(profile, &report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn password_encoding_has_at_least_24_unbiased_characters() {
        const { assert!(PASSWORD_LENGTH >= 24) };
        let mut counts = std::collections::HashMap::new();
        for byte in 0..=u8::MAX {
            let encoded = encode_password(&[byte; PASSWORD_LENGTH]);
            assert_eq!(encoded.len(), PASSWORD_LENGTH);
            assert!(encoded
                .iter()
                .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(c)));
            *counts.entry(encoded[0]).or_insert(0) += 1;
        }
        assert_eq!(counts.len(), 64);
        assert!(counts.values().all(|count| *count == 4));
    }

    #[test]
    fn generated_passwords_differ_and_never_debug_print() {
        let a = Secret::generate().unwrap();
        let b = Secret::generate().unwrap();
        assert_eq!(a.reveal().len(), PASSWORD_LENGTH);
        assert_ne!(a.reveal(), b.reveal());
        assert!(!format!("{a:?}").contains(a.reveal()));
    }

    #[test]
    fn plan_requests_use_allowlisted_operations_and_valid_lifetimes() {
        for kind in [RepairKind::Check, RepairKind::Repair] {
            for chunk in kind.chunks() {
                let request = plan_request(&chunk);
                assert_eq!(request.operations, chunk);
                assert!((1..=86400).contains(&request.valid_for_seconds));
            }
        }
        const { assert!(APPROVAL_SECONDS <= 15 * 60) };
        const { assert!(GRANT_SECONDS <= 24 * 3600) };
    }

    #[test]
    fn dependencies_are_expanded_like_the_engine() {
        assert_eq!(
            expand(&[Op::DismRestoreHealth]),
            vec![Op::DismScanHealth, Op::DismRestoreHealth]
        );
        assert_eq!(expand(&[Op::SfcRepair]), vec![Op::SfcVerify, Op::SfcRepair]);
        assert_eq!(
            RepairKind::Repair.all_operations(),
            vec![
                Op::DismScanHealth,
                Op::DismRestoreHealth,
                Op::SfcVerify,
                Op::SfcRepair
            ]
        );
        assert_eq!(
            RepairKind::Check.all_operations(),
            vec![Op::DismCheckHealth, Op::SfcVerify]
        );
    }

    #[test]
    fn repair_policy_adds_only_what_is_needed_and_expires() {
        let now = 1_000_000;
        let current = ops::OwnerPolicy::default();
        let check = policy_for(&current, &RepairKind::Check.all_operations(), now);
        assert_eq!(
            check.allowed, current.allowed,
            "diagnostics already allowed"
        );
        assert_eq!(check.opt_in_until, None, "a check needs no opt-in");
        assert!(check
            .exceptions
            .iter()
            .all(|e| e.expires_at == now + GRANT_SECONDS));

        let repair = policy_for(&current, &RepairKind::Repair.all_operations(), now);
        assert!(repair.allowed.contains(&Op::DismRestoreHealth));
        assert!(repair.allowed.contains(&Op::SfcRepair));
        assert!(!repair.allowed.contains(&Op::DefenderQuickScan));
        assert_eq!(repair.opt_in_until, Some(now + GRANT_SECONDS));
        assert!(repair.allowed.len() <= 6);
        assert_eq!(repair.window, current.window);
        assert_eq!(repair.idle_seconds, current.idle_seconds);
        // Every exception is for an allowed operation and short lived.
        assert!(repair
            .exceptions
            .iter()
            .all(|e| { repair.allowed.contains(&e.operation) && e.expires_at - now <= 24 * 3600 }));
        let scopes: Vec<_> = repair
            .exceptions
            .iter()
            .map(|e| (e.operation, e.scope))
            .collect();
        assert!(
            scopes
                .iter()
                .enumerate()
                .all(|(i, s)| !scopes[..i].contains(s)),
            "no duplicate scoped exceptions"
        );
        // Running again does not stack duplicates.
        let again = policy_for(&repair, &RepairKind::Repair.all_operations(), now + 5);
        assert_eq!(again.exceptions.len(), repair.exceptions.len());
    }

    #[test]
    fn expired_exceptions_are_dropped_and_longer_opt_in_is_kept() {
        let now = 5_000_000;
        let mut current = ops::OwnerPolicy::default();
        current.allowed.push(Op::SfcRepair);
        current.opt_in_until = Some(now + 10 * 3600);
        current.exceptions.push(ops::PolicyException {
            operation: Op::SfcRepair,
            scope: ops::ExceptionScope::MeteredNetwork,
            expires_at: now - 1,
        });
        let p = policy_for(&current, &[Op::SfcVerify], now);
        assert_eq!(p.opt_in_until, Some(now + 10 * 3600));
        assert!(p.exceptions.iter().all(|e| e.expires_at > now));
    }

    fn record(
        kind: RepairKind,
        states: &[(ops::StepState, Option<ops::Evidence>)],
    ) -> Vec<ops::PlanRecord> {
        // One record per chunk, in order.
        let mut next = states.iter();
        let mut out = Vec::new();
        for chunk in kind.chunks() {
            let kinds = expand(&chunk);
            let mut steps = Vec::new();
            let mut recs = Vec::new();
            for k in kinds {
                let (state, evidence) = *next.next().unwrap();
                steps.push(ops::PlanStep {
                    operation: k.spec(),
                    depends_on: vec![],
                });
                recs.push(ops::StepRecord {
                    state,
                    evidence,
                    ..Default::default()
                });
            }
            out.push(ops::PlanRecord {
                plan: ops::Plan {
                    schema: 1,
                    id: Uuid::nil(),
                    machine: String::new(),
                    created_at: 0,
                    expires_at: 0,
                    policy: ops::OwnerPolicy::default(),
                    steps,
                    digest: String::new(),
                },
                approval: None,
                consumed: true,
                steps: recs,
            });
        }
        out
    }

    #[test]
    fn check_results_are_plain() {
        use ops::{Evidence as E, StepState as S};
        let ok = record(
            RepairKind::Check,
            &[
                (S::Succeeded, Some(E::ComponentStoreHealthy)),
                (S::Succeeded, Some(E::DiagnosticCompleted)),
            ],
        );
        assert_eq!(
            classify(RepairKind::Check, &ok, false),
            RepairResult::NoProblems
        );
        let bad = record(
            RepairKind::Check,
            &[
                (S::Succeeded, Some(E::ComponentStoreRepairable)),
                (S::Succeeded, Some(E::DiagnosticCompleted)),
            ],
        );
        assert_eq!(
            classify(RepairKind::Check, &bad, false),
            RepairResult::ProblemsFound
        );
        let failed = record(
            RepairKind::Check,
            &[(S::Failed, None), (S::Cancelled, None)],
        );
        assert_eq!(
            classify(RepairKind::Check, &failed, false),
            RepairResult::CouldNotFinish
        );
        assert_eq!(
            classify(RepairKind::Check, &[], false),
            RepairResult::CouldNotFinish
        );
        let half = record(
            RepairKind::Check,
            &[
                (S::Succeeded, Some(E::ComponentStoreHealthy)),
                (S::Cancelled, None),
            ],
        );
        assert_eq!(
            classify(RepairKind::Check, &half, true),
            RepairResult::Stopped
        );
    }

    #[test]
    fn repair_results_are_honest() {
        use ops::{Evidence as E, StepState as S};
        let finished = record(
            RepairKind::Repair,
            &[
                (S::Succeeded, Some(E::ComponentStoreHealthy)),
                (S::Succeeded, Some(E::ComponentStoreHealthy)),
                (S::Succeeded, Some(E::DiagnosticCompleted)),
                (S::NeedsReview, Some(E::DiagnosticCompleted)),
            ],
        );
        // Unproven system-file repair always asks for a restart.
        assert_eq!(
            classify(RepairKind::Repair, &finished, false),
            RepairResult::NeedsRestart
        );
        let reboot = record(
            RepairKind::Repair,
            &[
                (S::Succeeded, Some(E::ComponentStoreHealthy)),
                (S::RebootRequired, None),
                (S::Pending, None),
                (S::Pending, None),
            ],
        );
        assert_eq!(
            classify(RepairKind::Repair, &reboot, false),
            RepairResult::NeedsRestart
        );
        let unproven = record(
            RepairKind::Repair,
            &[
                (S::Succeeded, Some(E::ComponentStoreHealthy)),
                (S::NeedsReview, Some(E::Inconclusive)),
                (S::Cancelled, None),
                (S::Cancelled, None),
            ],
        );
        assert_eq!(
            classify(RepairKind::Repair, &unproven, false),
            RepairResult::CouldNotFinish
        );
    }

    #[test]
    fn technical_details_keep_the_raw_evidence() {
        use ops::{Evidence as E, StepState as S};
        let r = record(
            RepairKind::Check,
            &[
                (S::Succeeded, Some(E::ComponentStoreHealthy)),
                (S::Succeeded, Some(E::DiagnosticCompleted)),
            ],
        );
        let text = technical_lines(&r);
        assert!(text.contains("dism_check_health") && text.contains("sfc_verify"));
    }

    #[test]
    fn friendly_errors_hide_developer_text() {
        for raw in [
            "Deferred: AC/storage/reboot/servicing readiness not confirmed",
            "Owner opt-in expired",
            "Operations worker ended or result already consumed",
            "Owner-initiated reboot has not occurred",
            "Maintenance execution requires Windows x64",
            "The remote name could not be resolved: network offline",
            "something unexpected",
            "Request blocked by policy",
            "Not enough resource on the clock",
        ] {
            let text = friendly_error(raw);
            assert!(text.len() > 10);
            assert_no_dev_terms(text);
        }
        assert_eq!(
            friendly_error("Owner-initiated reboot has not occurred"),
            "Restart your PC, then try again."
        );
        // Whole-word matching and policy first: no busy or network advice.
        // Built-in Administrator: explain, and do not offer a useless Retry.
        let raw = "Interactive split-token administrator required; service/over-the-shoulder elevation unsupported";
        assert_eq!(friendly_error(raw), ERR_USE_WINDOWS_UPDATE);
        assert_no_dev_terms(friendly_error(raw));
        assert!(!is_retryable(friendly_error(raw)));
        assert!(suggests_windows_update(friendly_error(raw)));
        assert!(is_retryable(friendly_error(
            "The file is locked by another operation"
        )));
        assert_eq!(
            friendly_error("Request blocked by policy"),
            "Your PC's settings don't allow this."
        );
        assert_eq!(
            friendly_error("Not enough resource on the clock"),
            "We couldn't finish this. Try again in a few minutes."
        );
        assert_eq!(
            friendly_error("The file is locked by another operation"),
            "Windows is busy with another task. Try again in a few minutes."
        );
    }

    fn assert_no_dev_terms(text: &str) {
        let lower = text.to_ascii_lowercase();
        for banned in [
            "dism",
            "sfc",
            "registry",
            "digest",
            "journal",
            "transaction",
            "provisioned",
            "exit code",
            "elevated",
            "broker",
            "powershell",
            "control",
            "attention",
            "compliant",
        ] {
            assert!(!lower.contains(banned), "{text:?} contains {banned:?}");
        }
    }

    #[test]
    fn primary_text_has_no_developer_terms() {
        for kind in [
            Op::DismCheckHealth,
            Op::DismScanHealth,
            Op::DismRestoreHealth,
            Op::SfcVerify,
            Op::SfcRepair,
            Op::DefenderQuickScan,
        ] {
            assert_no_dev_terms(step_label(kind));
        }
        for r in [
            RepairResult::NoProblems,
            RepairResult::ProblemsFound,
            RepairResult::Repaired,
            RepairResult::NeedsRestart,
            RepairResult::Stopped,
            RepairResult::CouldNotFinish,
        ] {
            assert_no_dev_terms(r.title());
            assert_no_dev_terms(r.detail());
        }
        for r in [
            InstallResult::Installed,
            InstallResult::NeedsRestart,
            InstallResult::NotConfirmed,
            InstallResult::Stopped,
            InstallResult::CouldNotFinish,
        ] {
            assert_no_dev_terms(r.title());
            assert_no_dev_terms(r.detail());
        }
        for s in [
            InstallStage::Preparing,
            InstallStage::Installing,
            InstallStage::Checking,
        ] {
            assert_no_dev_terms(s.label());
        }
        for p in TipProfile::ALL {
            assert_no_dev_terms(p.title());
            assert_no_dev_terms(p.blurb());
        }
        for id in diag::ProbeId::ALL {
            assert_no_dev_terms(tip_title(*id));
            assert_no_dev_terms(tip_advice(*id));
        }
    }

    #[test]
    fn every_profile_lists_known_probes_with_unique_titles() {
        for p in TipProfile::ALL {
            let ids = p.probes();
            assert!(ids.len() >= 5);
            for (i, id) in ids.iter().enumerate() {
                assert!(!ids[..i].contains(id));
            }
        }
    }

    #[test]
    fn exact_check_advice_is_plain_and_picks_the_first_problem() {
        for rule in [
            "os.feature_release_support",
            "boot.secure_boot_certs",
            "defender.tamper_protection",
            "defender.threats",
            "defender.exclusions_risky",
            "defender.scan_age",
            "smartscreen.apps",
            "smartscreen.browser_policy",
            "update.paused",
            "update.reboot_overdue",
            "ps.v2_engine",
            "net.hosts_file",
            "persistence.wmi_subscriptions",
            "services.unquoted_paths",
            "accounts.stale_enabled",
            "smb.shares_exposed",
            "firewall.user_dir_inbound_allow",
            "accounts.daily_admin",
            "accounts.hello_configured",
            "accounts.find_my_device",
            "vbs.kernel_stack_protection",
            "net.dns_encryption",
            "net.wifi_security",
            "persistence.run_and_tasks",
        ] {
            let text = rule_advice(rule).expect(rule);
            assert_no_dev_terms(text);
            assert!(text.len() <= 130, "{rule}: keep it to one short line");
        }
        assert_eq!(rule_advice("update.freshness"), None);
        assert_eq!(
            rule_open("os.feature_release_support"),
            Some(secblitz::actions::Action::OpenWindowsUpdate)
        );
        assert_eq!(rule_open("net.hosts_file"), None);

        // A probe with two problems shows the first one that has an exact step.
        let mut report = diag::collect(diag::Profile::Everyday, &diag::Context::default());
        let probe = report
            .probes
            .iter_mut()
            .find(|p| p.id == diag::ProbeId::DefenderProtection)
            .unwrap();
        probe.status = diag::Status::Attention;
        probe.assessments = ["defender.tamper_protection", "defender.scan_age"]
            .iter()
            .map(|id| diag::Assessment {
                status: diag::Status::Attention,
                detail: String::new(),
                rule: diag::RuleReference {
                    id: (*id).into(),
                    revision: 1,
                    mapping_version: String::new(),
                    documentation: vec![],
                },
            })
            .collect();
        let tips = summarize_tips(TipProfile::Everyday, &report);
        let tip = tips
            .tips
            .iter()
            .find(|t| t.title == tip_title(diag::ProbeId::DefenderProtection))
            .unwrap();
        assert_eq!(tip.state, TipState::Look);
        assert_eq!(
            tip.advice,
            rule_advice("defender.tamper_protection").unwrap()
        );
        assert_eq!(
            tip.open,
            Some(secblitz::actions::Action::OpenWindowsSecurity)
        );
    }

    #[test]
    fn new_checks_are_in_the_expected_profiles() {
        use diag::ProbeId as P;
        for p in TipProfile::ALL {
            for id in [
                P::OsSupport,
                P::SecureBootCerts,
                P::DefenderProtection,
                P::UpdatePolicy,
            ] {
                assert!(p.probes().contains(&id), "{p:?} {id:?}");
            }
        }
        assert!(TipProfile::Extra.probes().contains(&P::Persistence));
        assert!(!TipProfile::Everyday.probes().contains(&P::Persistence));
    }

    #[test]
    fn tips_report_puts_problems_first() {
        // Off Windows every probe is "unsupported", which must read as unknown.
        let mut report = diag::collect(diag::Profile::Everyday, &diag::Context::default());
        let plain = summarize_tips(TipProfile::Everyday, &report);
        assert!(!plain.tips.is_empty());
        assert!(plain.tips.iter().all(|t| t.state == TipState::Unknown));
        // Make the last listed probe healthy and the first one need attention.
        let ids = TipProfile::Everyday.probes();
        for probe in &mut report.probes {
            if probe.id == ids[ids.len() - 1] {
                probe.status = diag::Status::Healthy;
            }
            if probe.id == ids[1] {
                probe.status = diag::Status::Attention;
            }
        }
        let tips = summarize_tips(TipProfile::Everyday, &report);
        assert_eq!(tips.tips[0].state, TipState::Look);
        assert_eq!(tips.tips[0].title, tip_title(ids[1]));
        assert_eq!(tips.tips[0].advice, tip_advice(ids[1]));
        assert_eq!(tips.tips.last().unwrap().state, TipState::Good);
        assert_eq!(tips.count(TipState::Look), 1);
        assert_eq!(tips.count(TipState::Good), 1);
        assert!(tips
            .tips
            .iter()
            .filter(|t| t.state == TipState::Good)
            .all(|t| t.advice.is_empty()));
    }

    #[test]
    fn size_phrases_are_short() {
        assert_eq!(size_phrase(0), "");
        assert_eq!(size_phrase(1), "1 MB");
        assert_eq!(size_phrase(300 * 1024 * 1024), "300 MB");
        assert_eq!(size_phrase(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    fn update(n: u128) -> patch::Update {
        patch::Update {
            identity: patch::UpdateIdentity {
                update_id: Uuid::from_u128(n),
                revision: 1,
            },
            title: format!("2026-10 Cumulative Update {n}"),
            description: String::new(),
            kb_articles: vec![format!("50{n}")],
            categories: vec![],
            max_download_bytes: 10 * 1024 * 1024,
            last_changed: String::new(),
            severity: "Critical".into(),
            handler: String::new(),
            reboot_behavior: 1,
            eula: "Terms".into(),
            bundled: vec![],
        }
    }

    fn plan_of(updates: Vec<patch::Update>) -> patch::Plan {
        patch::Plan {
            schema: 1,
            id: Uuid::nil(),
            binding: patch::Binding {
                machine: String::new(),
                original_user: String::new(),
            },
            created_at: 0,
            expires_at: 0,
            source: String::new(),
            updates,
            digest: String::new(),
        }
    }

    #[test]
    fn plan_must_match_the_reviewed_updates() {
        let a = update(1);
        let b = update(2);
        let reviewed = vec![a.identity.clone(), b.identity.clone()];
        assert!(plan_matches(
            &reviewed,
            &plan_of(vec![b.clone(), a.clone()])
        ));
        assert!(!plan_matches(&reviewed, &plan_of(vec![a.clone()])));
        assert!(!plan_matches(&reviewed, &plan_of(vec![a, b, update(3)])));
        assert!(!plan_matches(&[], &plan_of(vec![])));
    }

    #[test]
    fn install_results_require_independent_confirmation() {
        let a = update(1);
        let record = |status, installed: Vec<patch::UpdateIdentity>| patch::Record {
            plan: plan_of(vec![a.clone()]),
            approval: None,
            status,
            process: None,
            uncertain: false,
            verification: Some(patch::Verification {
                installed,
                reboot_pending: false,
                checked_at: 0,
            }),
        };
        use patch::Status as S;
        assert_eq!(
            classify_install(&record(S::Succeeded, vec![a.identity.clone()]), false),
            InstallResult::Installed
        );
        assert_eq!(
            classify_install(&record(S::Succeeded, vec![]), false),
            InstallResult::NotConfirmed
        );
        assert_eq!(
            classify_install(&record(S::RebootRequired, vec![]), false),
            InstallResult::NeedsRestart
        );
        assert_eq!(
            classify_install(&record(S::NeedsReview, vec![]), false),
            InstallResult::NotConfirmed
        );
        assert_eq!(
            classify_install(&record(S::Installing, vec![]), true),
            InstallResult::Stopped
        );
        assert_eq!(
            classify_install(&record(S::Installing, vec![]), false),
            InstallResult::CouldNotFinish
        );
    }

    #[test]
    fn catalog_summary_counts_and_keeps_details_technical() {
        let catalog = patch::Catalog {
            binding: patch::Binding {
                machine: String::new(),
                original_user: String::new(),
            },
            searched_at: 0,
            source: "Microsoft Update".into(),
            updates: vec![update(1), update(2)],
        };
        let found = summarize_catalog(&catalog);
        assert_eq!(found.updates.len(), 2);
        assert_eq!(found.identities().len(), 2);
        assert_eq!(found.total_bytes(), 20 * 1024 * 1024);
        assert!(found.technical.contains("Microsoft Update"));
    }

    #[test]
    fn password_and_repair_calls_fail_cleanly_off_windows() {
        #[cfg(not(windows))]
        {
            let seen = std::sync::Mutex::new(Vec::new());
            run_repair(RepairKind::Check, Arc::new(AtomicBool::new(false)), &|e| {
                seen.lock().unwrap().push(e);
            });
            let events = seen.into_inner().unwrap();
            assert!(matches!(
                events.last(),
                Some(RepairEvent::Done {
                    result: RepairResult::CouldNotFinish,
                    note: Some(_),
                    ..
                })
            ));
            assert!(discover_updates().is_err());
        }
    }
}
