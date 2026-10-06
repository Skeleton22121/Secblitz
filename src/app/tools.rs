//! Background wrappers for the Tools page: repair, Windows updates, PC health tips and the password generator.
use anyhow::{bail, ensure, Context as _, Result};
use secblitz::diagnostics as diag;
use secblitz::operations::{self as ops, OperationKind as Op};
use secblitz::patching as patch;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};


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


/// Map a raw engine error to one calm sentence (translation key).
///
/// Short words are matched as whole words, so "lock" does not fire on
/// "blocked" or "clock" and "source" does not fire on "resource". Policy is
/// checked before busy/network because a message such as "blocked by policy"
/// will never succeed on a retry.
pub const ERR_USE_WINDOWS_UPDATE: &str =
    "Updates can't be installed from this account. Sign in to Windows with a different administrator account, or ask the person who manages this PC, then open Secblitz again. You can also install them in Windows Update.";
pub const ERR_UNAVAILABLE: &str =
    "This isn't available on this PC. Check for a newer version of Secblitz, or use Windows Settings instead.";
pub const ERR_SETTINGS_BLOCK: &str =
    "Your PC's settings don't allow this. If someone else manages this PC, ask them to allow it, then try again.";
pub const ERR_CHANGED: &str =
    "The list of updates changed. Press Check again, look over the updates, then install them.";
pub const ERR_RESTART: &str = "Restart your PC, then try again.";
pub const ERR_BUSY: &str = "Windows is busy with another task. Try again in a few minutes.";
pub const ERR_POWER: &str = "Plug your PC in, then try again.";
pub const ERR_DISK: &str = "Free up at least 5 GB on your system drive, then try again.";
pub const ERR_METERED: &str =
    "You're on a connection with a data limit. Connect to a network without one, then try again.";
pub const ERR_NOT_READY: &str = "Your PC isn't ready for this right now. Plug it in, save your work, restart if Windows is waiting, then try again.";
pub const ERR_EARLIER: &str =
    "An earlier repair or update still needs to be checked. Restart Secblitz and try again.";
pub const ERR_NETWORK: &str =
    "We couldn't reach Windows Update. Check your internet connection and try again.";
pub const ERR_GENERAL: &str = "We couldn't finish this. Try again in a few minutes. If it keeps happening, restart your PC and check for a Secblitz update.";
pub const ERR_REOPEN: &str = "Reopen Secblitz from its shortcut and try again.";

/// False when trying again cannot help (the GUI should show a different next
/// step instead of Retry). For `ERR_USE_WINDOWS_UPDATE` the next step is an
/// "Open Windows Update" button (`advice::NextStep::OpenWindowsUpdate`).
#[allow(dead_code)] // consumed by the GUI integration
pub fn is_retryable(note: &str) -> bool {
    !matches!(
        note,
        ERR_USE_WINDOWS_UPDATE | ERR_UNAVAILABLE | ERR_SETTINGS_BLOCK | ERR_REOPEN
    )
}

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
    } else if has(&["changed since they were reviewed"]) {
        ERR_CHANGED
    } else if has(&["requires windows", "not implemented", "unsupported"]) {
        ERR_UNAVAILABLE
    } else if has(&["reboot", "restart"]) {
        ERR_RESTART
    } else if has(&[
        "servicing is busy",
        "servicing process is active",
        "engine.lock is busy",
    ]) {
        ERR_BUSY
    } else if has(&["not plugged in", "ac power"]) {
        ERR_POWER
    } else if has(&["low disk space", "insufficient system storage"]) {
        ERR_DISK
    } else if has(&["metered"]) {
        ERR_METERED
    } else if has(&["update source", "user update policy"]) {
        ERR_SETTINGS_BLOCK
    } else if has(&["deferred", "readiness", "not ready", "stale", "ac/storage"]) {
        ERR_NOT_READY
    } else if has(&["unresolved", "independent verification", "interrupted"]) {
        ERR_EARLIER
    } else if word(&["policy", "opt-in", "managed", "ownership"]) || has(&["not enabled"]) {
        ERR_SETTINGS_BLOCK
    } else if word(&["busy", "lock", "locked", "contention"])
        || has(&[
            "another operation",
            "another install",
            "another update",
            "another instance",
            "another servicing",
            "already running",
        ])
    {
        ERR_BUSY
    } else if word(&["network", "offline", "internet", "source"])
        || words
            .iter()
            .any(|w| w.starts_with("0x8024") || w.starts_with("0x8007"))
        || has(&["timed out"])
    {
        ERR_NETWORK
    } else if has(&["elevation", "elevated", "administrator", "interactive"])
        || r == "unavailable"
        || has(&["broker", "launcher did not answer"])
    {
        ERR_REOPEN
    } else {
        ERR_GENERAL
    }
}

pub fn why_for_note(note: &str) -> &'static str {
    match note {
        ERR_USE_WINDOWS_UPDATE => "Windows only lets Secblitz install updates from a standard administrator account, and this account can't. Sign in with a different administrator account, or ask the person who manages this PC, then open Secblitz again.",
        ERR_UNAVAILABLE => "This version of Windows doesn't support this feature, so Secblitz can't do it here.",
        ERR_SETTINGS_BLOCK => "A setting on this PC, often set by a workplace or school, stops Secblitz from doing this.",
        ERR_CHANGED => "Windows found different updates from the ones you looked at, so nothing was installed.",
        ERR_REOPEN => "Secblitz was started in a way that doesn't allow this. Opening it from its shortcut fixes that.",
        ERR_RESTART => "Windows has changes waiting that need a restart before it can carry on.",
        ERR_BUSY => "Windows is running its own update or maintenance work. This usually ends within a few minutes.",
        ERR_POWER => "Updates need your PC to be plugged in, so it can't switch off part way through.",
        ERR_DISK => "Windows needs room on your system drive to download and set up updates.",
        ERR_METERED => "Windows treats this connection as one with a data limit, so large downloads are held back.",
        ERR_NOT_READY => "Windows isn't in a state where it can safely make changes yet.",
        ERR_EARLIER => "A job that was running earlier didn't finish cleanly, and Secblitz wants to check it first.",
        ERR_NETWORK => "Secblitz couldn't connect to the internet. Your connection may be off or very slow.",
        _ => "Something unexpected stopped this. Nothing was damaged. Restart your PC and try again, and check for a Secblitz update if it keeps happening.",
    }
}

pub const WHY_BITWARDEN_UNAVAILABLE: &str = "Windows only lets Secblitz install apps from a standard administrator account, and this account can't. Get Bitwarden from bitwarden.com instead.";
pub const WHY_BITWARDEN_OFFLINE: &str =
    "Secblitz couldn't connect to the internet. Check your connection, then press Retry.";

pub fn bitwarden_why(raw: &str) -> &'static str {
    if friendly_error(raw) == ERR_NETWORK {
        WHY_BITWARDEN_OFFLINE
    } else {
        friendly_why(raw)
    }
}

pub fn friendly_why(raw: &str) -> &'static str {
    why_for_note(friendly_error(raw))
}

pub fn repair_why(result: RepairResult, note: Option<&'static str>) -> &'static str {
    if let Some(n) = note {
        return why_for_note(n);
    }
    match result {
        RepairResult::NoProblems => "Windows checked itself and found nothing wrong.",
        RepairResult::ProblemsFound => "Windows found files that need repairing. Choose Repair system files to fix them.",
        RepairResult::Repaired => "Windows repaired the problems it found. You don't need to do anything else.",
        RepairResult::NeedsRestart => "Restart your PC to finish the repair.",
        RepairResult::Stopped => "You stopped the repair. Nothing else was started.",
        RepairResult::CouldNotFinish => "The repair didn't finish. Restart your PC and try again.",
    }
}

pub fn install_why(result: InstallResult, note: Option<&'static str>) -> &'static str {
    if let Some(n) = note {
        return why_for_note(n);
    }
    match result {
        InstallResult::Installed => "Windows confirmed that every update you chose is installed.",
        InstallResult::NeedsRestart => "The updates are installed. Restart your PC to finish.",
        InstallResult::NotConfirmed => "Windows didn't confirm every update. Open Windows Update to see what is left.",
        InstallResult::Stopped => "You stopped the update. Nothing else was started.",
        InstallResult::CouldNotFinish => "The update didn't finish. Restart your PC and try again.",
    }
}

fn unix_now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairKind {
    Check,
    Repair,
}

pub const PLAN_SECONDS: u64 = 3600;
pub const APPROVAL_SECONDS: u64 = 900;
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
    pub label: &'static str,
    pub step: usize,
    pub total: usize,
    pub elapsed: u64,
    pub step_elapsed: u64,
}

#[derive(Debug, Clone)]
pub enum RepairEvent {
    Preparing,
    Progress(RepairProgress),
    Done {
        result: RepairResult,
        note: Option<&'static str>,
        technical: String,
    },
}

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
    secblitz::platform::ensure_own_process_tree()?;
    let all = kind.all_operations();
    let original = ops::policy()?;
    let wanted = policy_for(&original, &all, unix_now()?);
    let changed = wanted != original;
    if changed {
        ops::set_policy(wanted)?;
    }
    let result = repair_steps(kind, cancel, emit, records, all.len());
    if changed {
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
    let mut step = (0, Instant::now());
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
            if index != step.0 {
                step = (index, Instant::now());
            }
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
                step_elapsed: step.1.elapsed().as_secs(),
            }));
        }
    }
}


#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub identity: patch::UpdateIdentity,
    pub title: String,
    pub size_bytes: u64,
    pub license: String,
}

#[derive(Debug, Clone, Default)]
pub struct Found {
    pub updates: Vec<UpdateInfo>,
    #[allow(dead_code)] // raw evidence, never shown on screen
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

const SERVICING_WAIT: Duration = Duration::from_secs(90);

pub fn discover_updates() -> Result<Found, (String, &'static str)> {
    // Windows starts its own servicing workers (update orchestrator, Defender
    // maintenance) at any time and they usually finish within a minute. The
    // patching interlock refuses to search meanwhile; wait instead of failing
    // a search the person just asked for.
    let deadline = Instant::now() + SERVICING_WAIT;
    loop {
        match discover_once() {
            Err((raw, _))
                if raw.contains("servicing process is active") && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_secs(5));
            }
            other => return other,
        }
    }
}

fn discover_once() -> Result<Found, (String, &'static str)> {
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
    secblitz::platform::ensure_own_process_tree()?;
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

pub fn probe_page(id: diag::ProbeId) -> Option<crate::guide::Page> {
    use crate::guide::Page;
    use diag::ProbeId as P;
    Some(match id {
        P::UpdateCache | P::UpdateHistory | P::OsSupport | P::SecureBootCerts | P::UpdatePolicy => {
            Page::WindowsUpdate
        }
        P::DefenderHealth | P::DefenderPolicy | P::DefenderProtection => Page::ProtectionHistory,
        P::SecurityProviders => Page::WindowsSecurity,
        P::Management => Page::WorkAccounts,
        P::SecureBoot => Page::Recovery,
        P::Tpm => Page::DeviceSecurity,
        P::BitLocker => Page::Encryption,
        P::Vbs => Page::CoreIsolation,
        P::Accounts | P::AccountHygiene | P::AccountSetup => Page::OtherUsers,
        P::RemoteAccess => Page::RemoteDesktop,
        P::Software => Page::InstalledApps,
        P::Storage | P::Backup => Page::Backup,
        P::Ntfs => Page::Storage,
        P::Adapters | P::Dns | P::Proxy | P::Vpn | P::DnsEncryption => Page::Network,
        P::WifiSecurity => Page::Wifi,
        P::SmartScreen => Page::AppBrowser,
        P::LegacyFeatures => Page::OptionalFeatures,
        P::FirewallRules => Page::Firewall,
        P::WindowsHello => Page::SignIn,
        P::BrowserExtensions
        | P::WinRe
        | P::Permissions
        | P::HostsFile
        | P::Persistence
        | P::Sharing
        | P::Autostart => return None,
    })
}

pub fn probe_guide(id: diag::ProbeId) -> Option<&'static crate::guide::Guide> {
    use diag::ProbeId as P;
    crate::guide::guide(match id {
        P::UpdateCache | P::UpdateHistory => "Windows updates",
        P::OsSupport => "Windows lifecycle",
        P::BitLocker => "Device encryption",
        P::SecureBoot => "Secure Boot",
        P::Management => "Management and mutation eligibility",
        _ => return None,
    })
}

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
        P::Vbs => "Core system protection (Memory integrity) is off. Protection shows whether this PC can turn it on safely.",
        P::WinRe => "Recovery tools are off. They help if Windows ever stops starting.",
        P::Accounts => "Use a normal account every day, and switch off the guest account.",
        P::RemoteAccess => "Switch off remote access if you don't use it.",
        P::Software => "Remove old apps that no longer get safety updates.",
        P::BrowserExtensions => "In your browser's menu, open Extensions or Add-ons and remove the ones you don't use.",
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

pub fn rule_advice(rule_id: &str) -> Option<&'static str> {
    Some(match rule_id {
        "os.feature_release_support" => "Your version of Windows is running out of safety updates. Install the newest version in Windows Update.",
        "boot.secure_boot_certs" => "Your PC's startup security needs a renewal. Install all Windows updates, then check your PC maker's website.",
        "defender.tamper_protection" => "Turn on Tamper Protection so malware can't switch off your virus protection.",
        "defender.threats" => "Windows found something harmful. Secblitz can remove it, and Windows Security usually keeps a copy you can restore.",
        "defender.exclusions_risky" => "Your virus protection skips some risky places. Look at the list in Windows Security.",
        "defender.scan_age" => "Your PC hasn't been scanned for a while. Run a quick scan in Windows Security.",
        "smartscreen.apps" => "Turn on warnings for unknown downloads in Windows Security.",
        "smartscreen.browser_policy" => "A setting has switched off your browser's warnings about dangerous sites. Ask whoever set up this PC.",
        "update.paused" => "Updates are paused. Resume them in Windows Update.",
        "update.reboot_overdue" => "Restart your PC to finish installing updates. Save your work first.",
        "ps.v2_engine" => "An old Windows tool that attackers like to use is still installed. Remove it in Windows Features.",
        "net.hosts_file" => "A hidden file is sending trusted websites somewhere else. Ask someone you trust to check it.",
        "persistence.wmi_subscriptions" => "Something is set to run quietly in the background. Ask someone you trust to look at it.",
        "services.unquoted_paths" => "A background program has a risky setup. Run a virus scan from this page, then ask someone you trust to look at it.",
        "remote.rdp" => "Remote access lets someone sign in to this PC from elsewhere. Turn it off in Settings if you don't use it.",
        "smb.v1" => "An old way of sharing files is still on. Turn it off in Windows Features unless an old device needs it.",
        "accounts.stale_enabled" => "Some old accounts are still switched on. Remove the ones nobody uses.",
        "smb.shares_exposed" => "Some folders are shared with everyone on your network. Stop sharing what you don't need.",
        "firewall.user_dir_inbound_allow" => "Apps in your Downloads or Desktop folders are allowed through the firewall. Remove ones you don't know.",
        "accounts.daily_admin" => "You use an administrator account every day. Make a normal account for daily use.",
        "accounts.hello_configured" => "No PIN or Windows Hello is set up. Add one in Sign-in options.",
        "accounts.find_my_device" => "Find my device is off. Turn it on in Settings so you can find a lost laptop.",
        "vbs.memory_integrity" => "Core system protection (Memory integrity) is off. Protection shows whether this PC can turn it on safely.",
        "vbs.kernel_stack_protection" => "An extra shield for the core of Windows is off. Protection shows whether this PC can turn it on safely.",
        "net.dns_encryption" => "Your internet lookups aren't private. Turn on encrypted lookups in your network settings.",
        "net.wifi_security" => "Your Wi-Fi has weak or no protection. Switch to the newest security option on your router.",
        "persistence.run_and_tasks" => "Open Task Manager, Startup apps, and switch off ones you don't know.",
        "winre.enabled" => "Recovery tools are off. They help if Windows stops starting. Ask someone you trust to turn them back on.",
        _ => return None,
    })
}

pub fn rule_open(rule_id: &str) -> Option<secblitz::actions::Action> {
    use secblitz::actions::Action;
    if let Some(g) = crate::guide::guide(rule_id) {
        return Some(g.page.action());
    }
    match rule_id {
        "os.feature_release_support"
        | "boot.secure_boot_certs"
        | "update.paused" => Some(Action::OpenWindowsUpdate),
        "defender.tamper_protection" => Some(Action::OpenTamperProtection),
        "defender.threats" | "defender.scan_age" => Some(Action::OpenProtectionHistory),
        "defender.exclusions_risky" => Some(Action::OpenWindowsSecurity),
        id if id.starts_with("smartscreen.") => Some(Action::OpenAppBrowserControl),
        "ps.v2_engine" => Some(Action::OpenOptionalFeatures),
        "accounts.stale_enabled" => Some(Action::OpenAccounts),
        "accounts.hello_configured" => Some(Action::OpenSignInSettings),
        "firewall.user_dir_inbound_allow" => Some(Action::OpenFirewall),
        _ => None,
    }
}

pub fn rule_fix(rule_id: &str) -> Option<&'static str> {
    let id = match rule_id {
        "remote.rdp" => "remote_desktop.disabled",
        "smb.v1" => "smb1.disabled",
        "winre.enabled" => "recovery.winre_enabled",
        other => other,
    };
    secblitz::hardening::spec(id).map(|spec| spec.id)
}

pub fn rule_fix_advice(rule_id: &str) -> &'static str {
    match rule_id {
        "remote.rdp" => "Remote access is on. If you don't use it, Secblitz can turn it off for you.",
        "smb.v1" => "An old way of sharing files is still on. Secblitz can turn it off for you, unless an old device needs it.",
        "services.unquoted_paths" => "A background program has a risky setup. We can fix this for you.",
        "firewall.user_dir_inbound_allow" => "Apps in your Downloads or Desktop folders are allowed through the firewall. We can fix this for you.",
        "net.hosts_file" => "A hidden file is sending trusted websites somewhere else. We can fix this for you.",
        "persistence.run_and_tasks" => "A risky program starts by itself with Windows. We can switch it off for you.",
        "accounts.stale_enabled" => "Some old accounts are still switched on. Secblitz can switch them off, and you can undo it.",
        "smb.shares_exposed" => "Some folders are shared with everyone on your network. Secblitz can limit them, and you can undo it.",
        "smartscreen.browser_policy" => "A setting has switched off your browser's warnings about dangerous sites. Secblitz can remove it, and you can undo it.",
        "winre.enabled" => "Recovery tools are off. Secblitz can turn them back on for you, and you can undo it.",
        _ => "Secblitz can fix this for you, and you can undo it. Look it over first.",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipFix<'r> {
    Offered(&'static str),
    NotOffered {
        control: &'static str,
        reason: &'r str,
    },
    Restart(&'static str),
    Unchecked,
    NoFix,
    Manual,
}

pub fn tip_fix<'r>(
    tip: &Tip,
    report: Option<&'r secblitz::engine::Report>,
    available: &[String],
) -> TipFix<'r> {
    let Some(control) = tip.fix else {
        return TipFix::Manual;
    };
    let Some(report) = report else {
        return TipFix::Unchecked;
    };
    let Some(row) = report.results.iter().find(|r| r.id == control) else {
        return if available.iter().any(|id| id == control) {
            TipFix::Unchecked
        } else {
            TipFix::NoFix
        };
    };
    if crate::app::flow::candidates(report, available)
        .iter()
        .any(|id| id == control)
    {
        return TipFix::Offered(control);
    }
    let a = crate::advice::for_outcome(row);
    if a.status == "Not offered" {
        TipFix::NotOffered {
            control,
            reason: &row.detail,
        }
    } else if a.step == crate::advice::NextStep::Restart {
        TipFix::Restart(a.next)
    } else if secblitz::vbs::is_vbs_check_id(control) && row.status == "compliant" {
        TipFix::Restart(core_restart_advice(control))
    } else {
        TipFix::Manual
    }
}

pub fn core_restart_advice(control: &str) -> &'static str {
    if control == secblitz::vbs::STACK_PROTECTION {
        "Extra core protection is on but is not running. Restart your PC (choose Restart, not Shut down)."
    } else {
        crate::app::score::RESTART_TO_START
    }
}

pub fn tip_words(tip: &Tip, fix: TipFix<'_>) -> (&'static str, Option<&'static crate::guide::Guide>) {
    let other = tip
        .guide
        .filter(|g| tip.fix.and_then(crate::guide::guide) != Some(*g));
    match fix {
        TipFix::Offered(_) => (tip.fix_advice, other),
        TipFix::NotOffered { control, reason } => (
            tip.advice,
            crate::guide::guide_not_offered(control, reason).or(other),
        ),
        TipFix::Restart(line) => (line, None),
        TipFix::Unchecked | TipFix::NoFix | TipFix::Manual => (tip.advice, tip.guide),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipAction {
    ReviewFix(&'static str),
    SeeWhy,
    CheckNow,
    RestartNow,
    RemoveThreats,
    Scan,
    Steps,
    Open(secblitz::actions::Action),
    None,
}

pub fn tip_action(tip: &Tip, fix: TipFix<'_>, can_open: bool) -> TipAction {
    let (_, guide) = tip_words(tip, fix);
    match (fix, tip.open) {
        _ if tip.state != TipState::Look => TipAction::None,
        (TipFix::Offered(id), _) => TipAction::ReviewFix(id),
        (TipFix::NotOffered { .. }, _) => TipAction::SeeWhy,
        (TipFix::Restart(_), _) => TipAction::None,
        _ if tip.restart => TipAction::RestartNow,
        _ if tip.remove_threats => TipAction::RemoveThreats,
        _ if tip.scan => TipAction::Scan,
        _ if guide.is_some() => TipAction::Steps,
        (_, Some(open)) if can_open => TipAction::Open(open),
        (TipFix::Unchecked, _) => TipAction::CheckNow,
        (TipFix::Manual, _) if tip.fix.is_some() => TipAction::SeeWhy,
        _ => TipAction::None,
    }
}

pub fn rule_restart(rule_id: &str) -> bool {
    rule_id == "update.reboot_overdue"
}

pub fn rule_scan(rule_id: &str) -> bool {
    rule_id == "defender.scan_age"
}

pub fn rule_remove_threats(rule_id: &str) -> bool {
    rule_id == "defender.threats"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreatsResult {
    Nothing,
    Removed,
    Partly,
    Stuck,
}

pub fn threats_result(r: &secblitz::actions::ThreatRemoval) -> ThreatsResult {
    match (r.found, r.removed, r.left) {
        (0, _, 0) => ThreatsResult::Nothing,
        (_, removed, 0) if removed > 0 => ThreatsResult::Removed,
        (_, removed, _) if removed > 0 => ThreatsResult::Partly,
        _ => ThreatsResult::Stuck,
    }
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
    pub open: Option<secblitz::actions::Action>,
    pub scan: bool,
    pub guide: Option<&'static crate::guide::Guide>,
    pub fix: Option<&'static str>,
    pub fix_advice: &'static str,
    pub restart: bool,
    pub remove_threats: bool,
    pub explain: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TipsReport {
    pub profile: TipProfile,
    pub tips: Vec<Tip>,
    #[allow(dead_code)] // raw evidence, never shown on screen
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

pub fn summarize_tips(profile: TipProfile, report: &diag::Report) -> TipsReport {
    let mut tips = Vec::new();
    let mut technical = String::new();
    for &id in profile.probes() {
        let Some(probe) = report.probes.iter().find(|p| p.id == id) else {
            continue;
        };
        let mut state = tip_state(probe.status);
        if id == diag::ProbeId::UpdateCache && probe.status != diag::Status::Attention {
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
        let attention: Vec<&str> = probe
            .assessments
            .iter()
            .filter(|a| a.status == diag::Status::Attention)
            .map(|a| a.rule.id.as_str())
            .filter(|id| rule_advice(id).is_some())
            .collect();
        let lead = attention
            .iter()
            .copied()
            .find(|id| rule_fix(id).is_some())
            .or_else(|| attention.iter().copied().find(|id| rule_restart(id)))
            .or_else(|| attention.first().copied());
        let remove_threats = probe
            .assessments
            .iter()
            .any(|a| a.status == diag::Status::Attention && rule_remove_threats(&a.rule.id));
        let scan = probe
            .assessments
            .iter()
            .any(|a| a.status == diag::Status::Attention && rule_scan(&a.rule.id));
        let look = state == TipState::Look;
        let explain = lead
            .into_iter()
            .chain(
                probe
                    .assessments
                    .iter()
                    .filter(|a| a.status == diag::Status::Attention)
                    .chain(probe.assessments.iter())
                    .map(|a| a.rule.id.as_str()),
            )
            .find(|rule| crate::explain::for_check(rule).is_some())
            .map(str::to_owned);
        let lead = lead.filter(|_| look);
        let restart = lead.is_some_and(rule_restart);
        tips.push(Tip {
            explain,
            title: tip_title(id),
            state,
            advice: match (look, lead.and_then(rule_advice)) {
                (false, _) => "",
                (true, Some(text)) => text,
                (true, None) => tip_advice(id),
            },
            open: match (look, lead.and_then(rule_open)) {
                (false, _) => None,
                (true, _) if restart => None,
                (true, Some(open)) => Some(open),
                (true, None) => probe_page(id).map(crate::guide::Page::action),
            },
            scan: look && scan,
            // The lead's own steps, else the first check that needs a look
            // and has steps (a fix that is not offered must not hide them).
            guide: lead
                .filter(|_| !restart)
                .and_then(|lead| {
                    crate::guide::guide(lead)
                        .or_else(|| attention.iter().copied().find_map(crate::guide::guide))
                })
                .or_else(|| probe_guide(id).filter(|_| look && !restart)),
            fix: lead.and_then(rule_fix),
            fix_advice: lead.map_or("", rule_fix_advice),
            restart,
            remove_threats: look && remove_threats,
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
    fn update_check_reasons_get_their_own_next_step() {
        let wrap = |why: &str| {
            format!("Patching subprocess failed; verification only (exit 0x1): Exact patching stopped (0x80131501): {why}; inspect protected record and verify, never replay")
        };
        for (why, want) in [
            ("Another servicing worker is active", ERR_BUSY),
            ("AC power not confirmed", ERR_POWER),
            ("Insufficient system storage", ERR_DISK),
            ("Metered/unknown network", ERR_METERED),
            ("Default update source is not unmanaged Windows Update", ERR_SETTINGS_BLOCK),
            ("Pending reboot; owner action required", ERR_RESTART),
        ] {
            assert_eq!(friendly_error(&wrap(why)), want, "{why}");
        }
    }

    #[test]
    fn friendly_errors_hide_developer_text() {
        for raw in [
            "Deferred: AC/storage/reboot/servicing readiness not confirmed",
            "Deferred: Windows is waiting for a restart",
            "Deferred: Windows servicing is busy",
            "Deferred: not plugged in",
            "Deferred: low disk space",
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
            ERR_RESTART
        );
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
            ERR_SETTINGS_BLOCK
        );
        for (raw, text) in [
            ("Deferred: Windows is waiting for a restart", ERR_RESTART),
            ("Deferred: Windows servicing is busy", ERR_BUSY),
            ("Deferred: a Windows servicing process is active", ERR_BUSY),
            ("Deferred: shared engine.lock is busy", ERR_BUSY),
            ("Deferred: not plugged in", ERR_POWER),
            ("Deferred: low disk space", ERR_DISK),
        ] {
            assert_eq!(friendly_error(raw), text);
        }
        let raw = "Started inside another program's process job; reopen Secblitz interactively";
        assert_eq!(friendly_error(raw), ERR_REOPEN);
        assert!(!is_retryable(ERR_REOPEN));
        assert_eq!(
            friendly_error("Not enough resource on the clock"),
            ERR_GENERAL
        );
        assert_eq!(
            friendly_error("The file is locked by another operation"),
            ERR_BUSY
        );
    }

    #[test]
    fn split_token_text_tells_the_person_what_to_do() {
        let raw = "Interactive split-token administrator required; service/over-the-shoulder elevation unsupported";
        let note = friendly_error(raw);
        assert!(note.contains("administrator account"));
        assert!(note.contains("open Secblitz again"));
        assert_no_dev_terms(note);
        assert_no_dev_terms(friendly_why(raw));
        assert!(!friendly_why(raw).to_ascii_lowercase().contains("token"));
    }

    #[test]
    fn more_details_never_echo_raw_text() {
        for raw in [
            "Interactive split-token administrator required; service/over-the-shoulder elevation unsupported",
            "HRESULT 0x80131501 from C:\\ProgramData\\x.json",
            "something unexpected",
            "The updates changed since they were reviewed; look again",
        ] {
            let why = friendly_why(raw);
            assert!(why.len() > 20);
            assert!(!why.contains("0x") && !why.contains("HRESULT") && !why.contains("C:\\"));
        }
        assert_eq!(
            friendly_why("something unexpected"),
            why_for_note(ERR_GENERAL)
        );
        assert_eq!(friendly_error("The updates changed since they were reviewed; look again"), ERR_CHANGED);
        for why in [bitwarden_why("Offline"), WHY_BITWARDEN_OFFLINE, WHY_BITWARDEN_UNAVAILABLE] {
            assert!(!why.contains("Windows Update"));
            assert!(!why.contains("unexpected"));
        }
        assert_eq!(bitwarden_why("Offline"), WHY_BITWARDEN_OFFLINE);
        assert_eq!(bitwarden_why("WinGet timed out"), WHY_BITWARDEN_OFFLINE);
        assert_eq!(bitwarden_why("something odd"), why_for_note(ERR_GENERAL));
        assert!(WHY_BITWARDEN_UNAVAILABLE.contains("bitwarden.com"));
        assert_eq!(friendly_why("Scan failed: network unreachable"), why_for_note(ERR_NETWORK));
        assert_eq!(friendly_why("scan blocked by policy"), why_for_note(ERR_SETTINGS_BLOCK));
        assert_eq!(friendly_why("Defender update: restart required"), why_for_note(ERR_RESTART));
        assert!(why_for_note(ERR_USE_WINDOWS_UPDATE).contains("different administrator"));
        assert!(repair_why(RepairResult::CouldNotFinish, Some(ERR_BUSY)).contains("maintenance"));
        assert!(install_why(InstallResult::NeedsRestart, None).contains("Restart"));
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

    fn report_with(probe_id: diag::ProbeId, rules: &[&str]) -> diag::Report {
        let mut report = diag::collect(TipProfile::Extra.profile(), &diag::Context::default());
        let probe = report
            .probes
            .iter_mut()
            .find(|p| p.id == probe_id)
            .expect("probe is part of the profile");
        probe.status = diag::Status::Attention;
        probe.assessments = rules
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
        report
    }

    fn tip_in(report: &diag::Report, probe_id: diag::ProbeId) -> Tip {
        summarize_tips(TipProfile::Extra, report)
            .tips
            .into_iter()
            .find(|t| t.title == tip_title(probe_id))
            .expect("tip for the probe")
    }

    #[test]
    fn checks_with_a_protection_fix_point_to_it_instead_of_manual_steps() {
        for id in [
            "accounts.stale_enabled",
            "smb.shares_exposed",
            "smartscreen.browser_policy",
            "smartscreen.apps",
            "update.paused",
            "ps.v2_engine",
        ] {
            assert_eq!(rule_fix(id), Some(id), "{id}");
            assert_no_dev_terms(rule_fix_advice(id));
            assert!(rule_fix_advice(id).len() <= 130, "{id}: one short line");
        }
        for id in [
            "defender.threats",
            "defender.scan_age",
            "persistence.wmi_subscriptions",
            "unknown.rule",
        ] {
            assert_eq!(rule_fix(id), None, "{id}");
        }
        for (probe, rule) in [
            (diag::ProbeId::AccountHygiene, "accounts.stale_enabled"),
            (diag::ProbeId::Sharing, "smb.shares_exposed"),
        ] {
            let tip = tip_in(&report_with(probe, &[rule]), probe);
            assert_eq!(tip.state, TipState::Look, "{rule}");
            assert_eq!(tip.fix, Some(rule), "{rule}");
            assert_eq!(tip.fix_advice, rule_fix_advice(rule), "{rule}");
            assert_eq!(tip.advice, rule_advice(rule).unwrap(), "{rule}");
            assert_eq!(tip.open, rule_open(rule), "{rule}");
            assert!(!tip.scan && !tip.remove_threats, "{rule}");
        }
        let mut report = report_with(diag::ProbeId::AccountHygiene, &["accounts.stale_enabled"]);
        let probe = report
            .probes
            .iter_mut()
            .find(|p| p.id == diag::ProbeId::AccountHygiene)
            .unwrap();
        probe.status = diag::Status::Healthy;
        for a in &mut probe.assessments {
            a.status = diag::Status::Healthy;
        }
        let tip = tip_in(&report, diag::ProbeId::AccountHygiene);
        assert_eq!((tip.fix, tip.remove_threats, tip.open), (None, false, None));
        let tip = tip_for(diag::ProbeId::Persistence, &["persistence.wmi_subscriptions"]);
        assert_eq!(tip.fix, None);
        assert_eq!(tip.advice, rule_advice("persistence.wmi_subscriptions").unwrap());
    }

    #[test]
    fn found_threats_offer_removal_and_a_scan_stays_for_the_scan_check() {
        assert!(rule_remove_threats("defender.threats"));
        assert!(!rule_remove_threats("defender.scan_age"));
        assert!(rule_scan("defender.scan_age") && !rule_scan("defender.threats"));
        let tip = tip_in(
            &report_with(diag::ProbeId::DefenderProtection, &["defender.threats"]),
            diag::ProbeId::DefenderProtection,
        );
        assert!(tip.remove_threats && !tip.scan && tip.fix.is_none());
        assert_eq!(tip.advice, rule_advice("defender.threats").unwrap());
        let tip = tip_in(
            &report_with(diag::ProbeId::DefenderProtection, &["defender.scan_age"]),
            diag::ProbeId::DefenderProtection,
        );
        assert!(tip.scan && !tip.remove_threats);
    }

    #[test]
    fn threat_removal_is_judged_only_by_what_defender_reports() {
        use secblitz::actions::ThreatRemoval as R;
        let judge = |found, removed, left| threats_result(&R { found, removed, left });
        assert_eq!(judge(0, 0, 0), ThreatsResult::Nothing);
        assert_eq!(judge(2, 2, 0), ThreatsResult::Removed);
        assert_eq!(judge(3, 1, 2), ThreatsResult::Partly);
        assert_eq!(judge(2, 0, 2), ThreatsResult::Stuck);
        assert_eq!(judge(1, 0, 0), ThreatsResult::Stuck);
        assert_eq!(judge(0, 0, 1), ThreatsResult::Stuck);
    }

    #[test]
    fn a_tip_for_something_secblitz_can_fix_goes_to_the_fix() {
        let mut report = diag::collect(TipProfile::Extra.profile(), &diag::Context::default());
        let probe = report
            .probes
            .iter_mut()
            .find(|p| p.id == diag::ProbeId::Vbs)
            .unwrap();
        probe.status = diag::Status::Attention;
        probe.assessments = vec![diag::Assessment {
            status: diag::Status::Attention,
            detail: String::new(),
            rule: diag::RuleReference {
                id: "vbs.memory_integrity".into(),
                revision: 1,
                mapping_version: String::new(),
                documentation: vec![],
            },
        }];
        let tips = summarize_tips(TipProfile::Extra, &report);
        let tip = tips
            .tips
            .iter()
            .find(|t| t.title == tip_title(diag::ProbeId::Vbs))
            .expect("the core protection tip is in the extra profile");
        assert_eq!(tip.state, TipState::Look);
        assert_eq!(tip.fix, Some("vbs.memory_integrity"));
        assert_eq!(tip.advice, rule_advice("vbs.memory_integrity").unwrap());
        assert_eq!(tip.guide, crate::guide::guide("vbs.memory_integrity"));
        assert!(!tip.fix_advice.is_empty());
        assert!(tips.tips.iter().filter(|t| t.fix.is_some()).count() == 1);
    }

    fn look_tip(rule: &str) -> Tip {
        Tip {
            title: "Test",
            state: TipState::Look,
            advice: rule_advice(rule).unwrap_or(""),
            open: rule_open(rule),
            scan: false,
            guide: crate::guide::guide(rule),
            fix: rule_fix(rule),
            fix_advice: rule_fix_advice(rule),
            restart: false,
            remove_threats: false,
            explain: None,
        }
    }

    fn protection(id: &str, status: &str, detail: &str) -> secblitz::engine::Report {
        secblitz::engine::Report {
            transaction: None,
            results: vec![secblitz::engine::Outcome {
                id: id.into(),
                status: status.into(),
                detail: detail.into(),
                ..secblitz::engine::Outcome::default()
            }],
            findings: vec![],
            readiness: None,
        }
    }

    #[test]
    fn a_tip_promises_a_fix_only_when_protection_offers_it() {
        let mi = "vbs.memory_integrity";
        let tip = look_tip(mi);
        let all = vec![mi.to_owned()];
        let r = protection(mi, "attention", "Eligible");
        assert_eq!(tip_fix(&tip, Some(&r), &all), TipFix::Offered(mi));
        assert_eq!(tip_fix(&tip, Some(&r), &[]), TipFix::Manual);
        let reason = format!("{}: a.sys", secblitz::vbs::DRIVER);
        let r = protection(mi, "skipped", &reason);
        assert_eq!(
            tip_fix(&tip, Some(&r), &all),
            TipFix::NotOffered {
                control: mi,
                reason: &reason
            }
        );
        assert!(crate::guide::guide_not_offered(mi, &reason).is_some());
        let r = protection(mi, "skipped", secblitz::vbs::NOT_SUPPORTED);
        assert!(matches!(tip_fix(&tip, Some(&r), &all), TipFix::NotOffered { .. }));
        assert!(crate::guide::guide_not_offered(mi, secblitz::vbs::NOT_SUPPORTED).is_none());
        assert_eq!(tip_fix(&tip, None, &all), TipFix::Unchecked);
        let other = protection("uac.enabled", "attention", "");
        assert_eq!(tip_fix(&tip, Some(&other), &all), TipFix::Unchecked);
        assert_eq!(tip_fix(&tip, Some(&other), &[]), TipFix::NoFix);
        let r = protection(mi, "compliant", "");
        assert_eq!(
            tip_fix(&tip, Some(&r), &all),
            TipFix::Restart(crate::app::score::RESTART_TO_START)
        );
        let r = protection(mi, "applied", "Preference applied; restart required");
        assert!(matches!(tip_fix(&tip, Some(&r), &all), TipFix::Restart(_)));
        let stack = look_tip("vbs.kernel_stack_protection");
        let r = protection("vbs.kernel_stack_protection", "compliant", "");
        assert_eq!(
            tip_fix(&stack, Some(&r), &["vbs.kernel_stack_protection".to_owned()]),
            TipFix::Restart(core_restart_advice("vbs.kernel_stack_protection"))
        );
        for (status, detail) in [
            ("skipped", "Relevant policy is configured: assessment only"),
            ("unknown", ""),
            ("skipped", secblitz::vbs::ALREADY_ON),
        ] {
            let r = protection(mi, status, detail);
            assert_eq!(tip_fix(&tip, Some(&r), &all), TipFix::Manual, "{status} {detail}");
        }
        let manual = look_tip("defender.tamper_protection");
        assert_eq!(manual.fix, None);
        let r = protection("defender.tamper_protection", "attention", "");
        assert_eq!(tip_fix(&manual, Some(&r), &all), TipFix::Manual);
        assert_eq!(rule_fix("remote.rdp"), Some("remote_desktop.disabled"));
        assert_eq!(rule_fix("smb.v1"), Some("smb1.disabled"));
        let rdp = look_tip("remote.rdp");
        let r = protection("remote_desktop.disabled", "attention", "Eligible");
        assert_eq!(
            tip_fix(&rdp, Some(&r), &["remote_desktop.disabled".to_owned()]),
            TipFix::Offered("remote_desktop.disabled")
        );
        assert_eq!(rule_fix("winre.enabled"), Some("recovery.winre_enabled"));
        let winre = tip_for(diag::ProbeId::WinRe, &["winre.enabled"]);
        assert_eq!(winre.fix, Some("recovery.winre_enabled"));
        assert_eq!(winre.advice, rule_advice("winre.enabled").unwrap());
        assert_eq!(winre.fix_advice, rule_fix_advice("winre.enabled"));
        assert_eq!(winre.explain.as_deref(), Some("winre.enabled"));
        let fixes = ["recovery.winre_enabled".to_owned()];
        let r = protection("recovery.winre_enabled", "attention", "Eligible");
        assert_eq!(tip_fix(&winre, Some(&r), &fixes), TipFix::Offered("recovery.winre_enabled"));
        assert_eq!(
            tip_action(&winre, tip_fix(&winre, Some(&r), &fixes), true),
            TipAction::ReviewFix("recovery.winre_enabled")
        );
        assert_eq!(tip_words(&winre, TipFix::Offered("recovery.winre_enabled")).0, winre.fix_advice);
        let reason = "Not offered: the recovery tools are missing from this PC";
        let r = protection("recovery.winre_enabled", "skipped", reason);
        assert_eq!(
            tip_fix(&winre, Some(&r), &fixes),
            TipFix::NotOffered { control: "recovery.winre_enabled", reason }
        );
        assert_eq!(tip_action(&winre, tip_fix(&winre, Some(&r), &fixes), true), TipAction::SeeWhy);
        assert_eq!(tip_fix(&winre, None, &fixes), TipFix::Unchecked);
        let r = protection("recovery.winre_enabled", "compliant", "");
        assert_eq!(tip_fix(&winre, Some(&r), &fixes), TipFix::Manual);
        assert_eq!(tip_action(&winre, TipFix::Manual, true), TipAction::SeeWhy);
        assert!(!rule_advice("winre.enabled").unwrap().contains("Secblitz can"));
        for rule in ["remote.rdp", "smb.v1", mi, "vbs.kernel_stack_protection"] {
            let text = rule_advice(rule).unwrap();
            assert!(!text.contains("Secblitz can"), "{rule}: {text}");
            assert!(crate::guide::guide(rule).is_some(), "{rule}: steps for doing it by hand");
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

    fn tip_for(probe_id: diag::ProbeId, rules: &[&str]) -> Tip {
        let profile = TipProfile::ALL
            .into_iter()
            .find(|p| p.probes().contains(&probe_id))
            .expect("a profile lists the probe");
        let mut report = diag::collect(profile.profile(), &diag::Context::default());
        let probe = report
            .probes
            .iter_mut()
            .find(|p| p.id == probe_id)
            .unwrap();
        probe.status = diag::Status::Attention;
        probe.assessments = rules
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
        summarize_tips(profile, &report)
            .tips
            .into_iter()
            .find(|t| t.title == tip_title(probe_id))
            .unwrap()
    }

    #[test]
    fn tips_for_checks_we_can_fix_point_to_the_fix_and_keep_the_manual_way() {
        for id in [
            "services.unquoted_paths",
            "firewall.user_dir_inbound_allow",
            "net.hosts_file",
            "persistence.run_and_tasks",
        ] {
            assert_eq!(rule_fix(id), Some(id), "{id} must be a real fix on the Protection page");
            let fix = rule_fix_advice(id);
            assert!(fix.contains("We can"), "{id}: {fix}");
            let manual = rule_advice(id).unwrap();
            assert!(!manual.contains("We can") && !manual.contains("Secblitz can"), "{id}: {manual}");
            assert!(!manual.contains('\u{2014}'), "{id}");
        }
        assert!(rule_advice("persistence.run_and_tasks").unwrap().contains("Task Manager"));
        let tip = tip_for(
            diag::ProbeId::Persistence,
            &["persistence.wmi_subscriptions", "services.unquoted_paths"],
        );
        assert_eq!(tip.fix, Some("services.unquoted_paths"));
        assert!(!tip.restart && !tip.scan);
        assert_eq!(tip.advice, rule_advice("services.unquoted_paths").unwrap());
        assert_eq!(tip.fix_advice, rule_fix_advice("services.unquoted_paths"));
        assert_eq!(tip.explain.as_deref(), Some("services.unquoted_paths"));
        let tip = tip_for(diag::ProbeId::Persistence, &["persistence.wmi_subscriptions"]);
        assert!(tip.fix.is_none() && !tip.restart);
        assert_eq!(tip.guide, crate::guide::guide("persistence.wmi_subscriptions"));
    }

    #[test]
    fn review_fix_is_only_said_when_the_protection_page_offers_it() {
        let id = "net.hosts_file";
        let tip = tip_for(diag::ProbeId::HostsFile, &[id]);
        let all = vec![id.to_owned()];
        assert_eq!(tip_fix(&tip, Some(&protection(id, "attention", "Eligible")), &all), TipFix::Offered(id));
        for status in ["ok", "conflict", "compliant", "unknown"] {
            assert_eq!(tip_fix(&tip, Some(&protection(id, status, "")), &all), TipFix::Manual, "{status}");
        }
        let managed = protection(id, "skipped", "Domain-managed machine: assessment only");
        assert_eq!(tip_fix(&tip, Some(&managed), &all), TipFix::Manual);
        assert_eq!(tip_fix(&tip, None, &all), TipFix::Unchecked);
        let reason = "Not offered: the hosts file uses a format we cannot keep exactly";
        assert_eq!(
            tip_fix(&tip, Some(&protection(id, "skipped", reason)), &all),
            TipFix::NotOffered { control: id, reason }
        );
        let mut pending = protection(id, "attention", "Eligible");
        pending.findings.push(secblitz::model::Finding {
            title: "x".into(),
            status: "pending".into(),
            detail: String::new(),
        });
        assert_eq!(tip_fix(&tip, Some(&pending), &all), TipFix::Manual);
        assert_eq!(
            tip_fix(&tip, Some(&protection("services.unquoted_paths", "attention", "")), &all),
            TipFix::Unchecked
        );
    }

    #[test]
    fn overdue_restart_offers_restart_now_instead_of_opening_windows_update() {
        assert!(rule_restart("update.reboot_overdue") && !rule_restart("update.paused"));
        let tip = tip_for(diag::ProbeId::UpdatePolicy, &["update.reboot_overdue"]);
        assert!(tip.restart && tip.fix.is_none() && tip.open.is_none() && tip.guide.is_none());
        assert!(tip.advice.contains("Save your work"));
        let tip = tip_for(diag::ProbeId::UpdatePolicy, &["update.reboot_overdue", "update.paused"]);
        assert_eq!((tip.fix, tip.restart), (Some("update.paused"), false));
        let mut report = diag::collect(diag::Profile::Everyday, &diag::Context::default());
        for probe in &mut report.probes {
            probe.status = diag::Status::Healthy;
        }
        let tips = summarize_tips(TipProfile::Everyday, &report);
        assert!(tips.tips.iter().all(|t| t.fix.is_none() && !t.restart));
    }

    #[test]
    fn every_area_tip_without_its_own_check_opens_a_page_or_shows_steps() {
        use diag::ProbeId as P;
        let none = [
            P::BrowserExtensions,
            P::WinRe,
            P::Permissions,
            P::HostsFile,
            P::Persistence,
            P::Sharing,
            P::Autostart,
        ];
        for &id in P::ALL {
            let tip = Tip {
                title: tip_title(id),
                state: TipState::Look,
                advice: tip_advice(id),
                open: probe_page(id).map(crate::guide::Page::action),
                scan: false,
                guide: probe_guide(id),
                fix: None,
                fix_advice: "",
                restart: false,
                remove_threats: false,
                explain: None,
            };
            let action = tip_action(&tip, TipFix::Manual, true);
            assert_eq!(action == TipAction::None, none.contains(&id), "{id:?}: {action:?}");
            if let (Some(g), Some(page)) = (probe_guide(id), probe_page(id)) {
                assert_eq!(g.page, page, "{id:?}");
            }
        }
    }

    #[test]
    fn every_tip_that_needs_the_person_has_a_fix_an_action_or_steps() {
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
            "vbs.memory_integrity",
            "vbs.kernel_stack_protection",
            "net.dns_encryption",
            "net.wifi_security",
            "persistence.run_and_tasks",
            "remote.rdp",
            "smb.v1",
            "winre.enabled",
        ] {
            let mut tip = look_tip(rule);
            tip.scan = rule_scan(rule);
            tip.remove_threats = rule_remove_threats(rule);
            tip.restart = rule_restart(rule);
            let mut states = vec![TipFix::Manual];
            if let Some(control) = tip.fix {
                states.extend([
                    TipFix::Unchecked,
                    TipFix::Offered(control),
                    TipFix::NotOffered {
                        control,
                        reason: "Not offered: some reason",
                    },
                ]);
            }
            for fix in states {
                assert_ne!(
                    tip_action(&tip, fix, true),
                    TipAction::None,
                    "{rule} {fix:?}: needs a fix, an action, a page or steps"
                );
            }
        }
        let tip = look_tip("vbs.memory_integrity");
        let fix = TipFix::Restart(crate::app::score::RESTART_TO_START);
        assert_eq!(tip_action(&tip, fix, true), TipAction::None);
        assert_eq!(tip_words(&tip, fix), (crate::app::score::RESTART_TO_START, None));
        let tip = look_tip("net.hosts_file");
        assert_eq!(tip_action(&tip, TipFix::Unchecked, true), TipAction::CheckNow);
        assert_eq!(tip_action(&tip, TipFix::Manual, true), TipAction::SeeWhy);
        assert_eq!(tip_action(&tip, TipFix::Offered("net.hosts_file"), true), TipAction::ReviewFix("net.hosts_file"));
        let tip = look_tip("firewall.user_dir_inbound_allow");
        assert_eq!(
            tip_action(&tip, TipFix::Unchecked, true),
            TipAction::Open(secblitz::actions::Action::OpenFirewall)
        );
        let wmi = "persistence.wmi_subscriptions";
        assert!(rule_fix(wmi).is_none());
        let guide = crate::guide::guide(wmi).expect("hidden tasks have steps");
        assert!(guide.steps[0].contains("Don't remove anything yourself"));
        let tip = tip_for(diag::ProbeId::Persistence, &[wmi]);
        assert_eq!(tip.guide, Some(guide));
        let tip = tip_for(diag::ProbeId::Persistence, &["services.unquoted_paths", wmi]);
        assert_eq!(tip.fix, Some("services.unquoted_paths"));
        assert_eq!(tip.guide, Some(guide));
        for fix in [
            TipFix::Manual,
            TipFix::Unchecked,
            TipFix::Offered("services.unquoted_paths"),
            TipFix::NotOffered {
                control: "services.unquoted_paths",
                reason: "Not offered: some reason",
            },
        ] {
            assert_eq!(tip_words(&tip, fix).1, Some(guide), "{fix:?}");
        }
        let rdp = look_tip("remote.rdp");
        assert_eq!(tip_words(&rdp, TipFix::Offered("remote_desktop.disabled")).1, None);
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
            "vbs.memory_integrity",
            "vbs.kernel_stack_protection",
            "net.dns_encryption",
            "net.wifi_security",
            "persistence.run_and_tasks",
            "winre.enabled",
        ] {
            let text = rule_advice(rule).expect(rule);
            assert_no_dev_terms(text);
            assert!(text.len() <= 130, "{rule}: keep it to one short line");
        }
        assert_eq!(rule_advice("update.freshness"), None);
        for id in ["vbs.memory_integrity", "vbs.kernel_stack_protection"] {
            assert_eq!(rule_fix(id), Some(id), "{id}");
            assert_eq!(rule_open(id), Some(secblitz::actions::Action::OpenCoreIsolation), "{id}");
            assert!(rule_advice(id).unwrap().contains("Protection"), "{id}");
        }
        assert_eq!(rule_fix("defender.exclusions_risky"), Some("defender.exclusions_risky"));
        assert!(rule_fix("defender.tamper_protection").is_none() && rule_fix("update.freshness").is_none());
        assert_eq!(
            rule_open("os.feature_release_support"),
            Some(secblitz::actions::Action::OpenWindowsUpdate)
        );
        assert_eq!(rule_open("net.hosts_file"), None);
        use secblitz::actions::Action as A;
        assert_eq!(
            rule_open("defender.tamper_protection"),
            Some(A::OpenTamperProtection)
        );
        assert_eq!(
            rule_open("defender.threats"),
            Some(A::OpenProtectionHistoryList)
        );
        assert_eq!(
            rule_open("defender.scan_age"),
            Some(A::OpenProtectionHistory)
        );
        assert_eq!(
            rule_open("smartscreen.apps"),
            Some(A::OpenAppBrowserControl)
        );
        assert_eq!(
            rule_open("smartscreen.browser_policy"),
            Some(A::OpenAppBrowserControl)
        );
        assert_eq!(rule_open("ps.v2_engine"), Some(A::OpenOptionalFeatures));
        assert_eq!(rule_open("accounts.stale_enabled"), Some(A::OpenAccounts));

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
            Some(secblitz::actions::Action::OpenTamperProtection)
        );
    }

    #[test]
    fn a_tip_for_something_only_the_person_can_do_carries_numbered_steps() {
        let mut report = diag::collect(diag::Profile::Everyday, &diag::Context::default());
        let probe = report
            .probes
            .iter_mut()
            .find(|p| p.id == diag::ProbeId::DefenderProtection)
            .unwrap();
        probe.status = diag::Status::Attention;
        probe.assessments = vec![diag::Assessment {
            status: diag::Status::Attention,
            detail: String::new(),
            rule: diag::RuleReference {
                id: "defender.tamper_protection".into(),
                revision: 1,
                mapping_version: String::new(),
                documentation: vec![],
            },
        }];
        let tips = summarize_tips(TipProfile::Everyday, &report);
        let tip = tips
            .tips
            .iter()
            .find(|t| t.title == tip_title(diag::ProbeId::DefenderProtection))
            .unwrap();
        let guide = tip.guide.expect("guide");
        assert_eq!(guide.page.action(), tip.open.unwrap());
        assert_eq!(tip.fix, None);
        for rule in ["accounts.find_my_device", "net.wifi_security", "vbs.kernel_stack_protection"] {
            let g = crate::guide::guide(rule).expect(rule);
            assert_eq!(rule_open(rule), Some(g.page.action()), "{rule}");
        }
        assert_eq!(rule_fix("update.paused"), Some("update.paused"));
        assert_eq!(rule_fix("defender.tamper_protection"), None);
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
        let mut report = diag::collect(diag::Profile::Everyday, &diag::Context::default());
        let plain = summarize_tips(TipProfile::Everyday, &report);
        assert!(!plain.tips.is_empty());
        assert!(plain.tips.iter().all(|t| t.state == TipState::Unknown));
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
