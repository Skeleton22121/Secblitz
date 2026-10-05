use super::*;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    pub schema: u32,
    pub machine: String,
    pub revision: u64,
    pub policy: OwnerPolicy,
    pub plans: Vec<PlanRecord>,
}

pub(super) trait Storage {
    fn load(&mut self) -> Result<Option<Vec<u8>>>;
    /// Atomic replace, including durable flush; failure poisons the engine.
    fn save(&mut self, bytes: &[u8]) -> Result<()>;
}

#[derive(Clone, Debug)]
pub(super) struct Facts {
    pub captured_at: u64,
    pub elevated: bool,
    pub unmanaged: bool,
    pub ac: bool,
    pub storage_ready: bool,
    pub reboot_pending: bool,
    pub busy: bool,
    pub idle_seconds: Option<u32>,
    pub unmetered: Option<bool>,
    pub boot_time: u64,
    pub defender_scan_end: Option<u64>,
}

pub(super) enum Event {
    Spawned(ProcessIdentity),
    Tick(u64),
    Stop(StopReason),
}
pub(super) struct Execution {
    pub code: u32,
    pub evidence: Evidence,
}
/// Checked again by the native launcher AFTER executable/module pinning and
/// immediately before CreateProcess. A slow fsync/trust check cannot stretch an
/// approval, readiness observation or soft-gate exception beyond its lifetime.
pub(super) struct LaunchPermit {
    pub not_before: u64,
    pub expires_at: u64,
}
pub(super) trait Backend {
    fn machine(&self) -> Result<String>;
    fn time(&self) -> Result<u64> {
        now()
    }
    fn facts(&mut self, kind: OperationKind, process: Option<&ProcessIdentity>) -> Result<Facts>;
    /// Must supervise the child until it exits, including if notify fails to
    /// persist, cancellation, timeout, or output overflow. Never force-kill.
    fn execute(
        &mut self,
        kind: OperationKind,
        permit: &LaunchPermit,
        control: &Control,
        notify: &mut dyn FnMut(Event),
    ) -> Result<Execution>;
    fn verify(
        &mut self,
        kind: OperationKind,
        baseline: &Baseline,
        permit: &LaunchPermit,
        control: &Control,
        notify: &mut dyn FnMut(Event),
    ) -> Result<Evidence>;
}

pub(super) struct Control {
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<Mutex<Progress>>,
}
impl Control {
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
    fn publish(&self, index: usize, record: &StepRecord, elapsed: u64) {
        if let Ok(mut p) = self.progress.lock() {
            p.step = Some(index);
            p.state = Some(record.state);
            p.stop_reason = record.stop_reason;
            p.elapsed_seconds = elapsed;
        }
    }
}

pub(super) struct Engine<S, B> {
    pub state: State,
    store: S,
    backend: B,
    poisoned: bool,
}

fn unique<T: PartialEq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .all(|(i, v)| !values[..i].contains(v))
}
fn hash_valid(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn transition_allowed(from: StepState, to: StepState) -> bool {
    use StepState::*;
    matches!(
        (from, to),
        (Pending, Intent | Cancelled)
            | (Intent, Running | NeedsReview | Verifying)
            | (
                Running,
                Monitoring | Verifying | Succeeded | Failed | RebootRequired | NeedsReview
            )
            | (
                Monitoring,
                Verifying | Succeeded | Failed | RebootRequired | NeedsReview
            )
            | (Verifying, Verifying | Succeeded | NeedsReview)
            | (RebootRequired | NeedsReview | Failed, Verifying)
    )
}

fn policy_valid(p: &OwnerPolicy) -> Result<()> {
    ensure!(
        p.allowed.len() <= 6 && unique(&p.allowed),
        "Duplicate/excess allowed operations"
    );
    ensure!(
        p.window.start_minute_utc < 1440
            && p.window.end_minute_utc < 1440
            && p.window.start_minute_utc != p.window.end_minute_utc,
        "Invalid UTC maintenance window"
    );
    ensure!(
        (60..=86400).contains(&p.idle_seconds),
        "Idle threshold outside bounds"
    );
    ensure!(
        p.allowed.iter().all(|k| k.diagnostic()) || p.opt_in_until.is_some(),
        "Non-diagnostic maintenance needs expiring owner opt-in"
    );
    ensure!(p.exceptions.len() <= 18, "Too many exceptions");
    let scopes: Vec<_> = p
        .exceptions
        .iter()
        .map(|e| (e.operation, e.scope))
        .collect();
    ensure!(
        unique(&scopes)
            && p.exceptions
                .iter()
                .all(|e| p.allowed.contains(&e.operation) && e.expires_at > 0),
        "Invalid scoped exception"
    );
    Ok(())
}

fn steps(requested: &[OperationKind]) -> Result<Vec<PlanStep>> {
    use OperationKind::*;
    ensure!(
        !requested.is_empty() && requested.len() <= 6 && unique(requested),
        "Select distinct allowlisted operations"
    );
    let mut selected = requested.to_vec();
    if selected.contains(&DismRestoreHealth) && !selected.contains(&DismScanHealth) {
        selected.push(DismScanHealth);
    }
    if selected.contains(&SfcRepair) && !selected.contains(&SfcVerify) {
        selected.push(SfcVerify);
    }
    let order = [
        DismCheckHealth,
        DismScanHealth,
        DismRestoreHealth,
        SfcVerify,
        SfcRepair,
        DefenderQuickScan,
    ];
    let mut out: Vec<PlanStep> = Vec::new();
    for kind in order.into_iter().filter(|k| selected.contains(k)) {
        let depends_on = out
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                ((kind == DismRestoreHealth && s.operation.kind == DismScanHealth)
                    || (kind == SfcRepair
                        && matches!(s.operation.kind, SfcVerify | DismRestoreHealth)))
                .then_some(i)
            })
            .collect();
        out.push(PlanStep {
            operation: kind.spec(),
            depends_on,
        });
    }
    Ok(out)
}

impl Plan {
    fn computed_digest(&self) -> Result<String> {
        let mut copy = self.clone();
        copy.digest.clear();
        digest(&copy)
    }
    fn validate(&self, machine: &str) -> Result<()> {
        ensure!(
            self.schema == SCHEMA && self.machine == machine && self.id.get_version_num() == 4,
            "Plan identity/schema mismatch"
        );
        ensure!(
            self.created_at > 0
                && self.expires_at > self.created_at
                && self.expires_at - self.created_at <= MAX_PLAN_SECONDS,
            "Invalid plan lifetime"
        );
        policy_valid(&self.policy)?;
        let kinds: Vec<_> = self.steps.iter().map(|s| s.operation.kind).collect();
        ensure!(
            self.steps == steps(&kinds)?,
            "Plan specifications/dependencies differ from compiled allowlist"
        );
        ensure!(
            self.steps
                .iter()
                .all(|s| self.policy.allowed.contains(&s.operation.kind)),
            "Plan outside policy"
        );
        ensure!(
            hash_valid(&self.digest) && self.digest == self.computed_digest()?,
            "Exact plan digest mismatch"
        );
        Ok(())
    }
}

impl State {
    fn validate(&self, machine: &str) -> Result<()> {
        ensure!(
            self.schema == SCHEMA && self.machine == machine && hash_valid(machine),
            "Operations state belongs to another machine/schema"
        );
        ensure!(
            self.plans.len() <= MAX_PLANS,
            "Operations record cap reached"
        );
        policy_valid(&self.policy)?;
        let ids: Vec<_> = self.plans.iter().map(|p| p.plan.id).collect();
        ensure!(unique(&ids), "Duplicate plans");
        for r in &self.plans {
            r.plan.validate(machine)?;
            ensure!(r.steps.len() == r.plan.steps.len(), "Step count mismatch");
            if let Some(a) = &r.approval {
                ensure!(
                    a.digest == r.plan.digest
                        && a.approved_at >= r.plan.created_at
                        && a.expires_at > a.approved_at
                        && a.expires_at <= r.plan.expires_at
                        && a.expires_at - a.approved_at <= MAX_APPROVAL_SECONDS,
                    "Invalid approval"
                );
            }
            ensure!(
                !r.consumed || r.approval.is_some(),
                "Execution without approval"
            );
            for (index, s) in r.steps.iter().enumerate() {
                ensure!(
                    r.consumed || *s == StepRecord::default(),
                    "Unconsumed plan contains execution"
                );
                if s.state == StepState::Pending {
                    ensure!(
                        *s == StepRecord::default(),
                        "Pending step has execution fields"
                    );
                }
                if s.state == StepState::Cancelled {
                    ensure!(
                        *s == StepRecord {
                            state: StepState::Cancelled,
                            ..StepRecord::default()
                        },
                        "Cancelled step contains execution fields"
                    );
                }
                if let Some(evidence) = s.evidence {
                    let compatible = evidence == Evidence::Inconclusive
                        || match r.plan.steps[index].operation.kind {
                            OperationKind::DismCheckHealth
                            | OperationKind::DismScanHealth
                            | OperationKind::DismRestoreHealth => matches!(
                                evidence,
                                Evidence::ComponentStoreHealthy
                                    | Evidence::ComponentStoreRepairable
                                    | Evidence::ComponentStoreNonRepairable
                            ),
                            OperationKind::SfcVerify | OperationKind::SfcRepair => {
                                evidence == Evidence::DiagnosticCompleted
                            }
                            OperationKind::DefenderQuickScan => {
                                evidence == Evidence::DefenderScanCompleted
                            }
                        };
                    ensure!(compatible, "Evidence belongs to another operation");
                }
                if !matches!(s.state, StepState::Pending | StepState::Cancelled) {
                    let b = s
                        .baseline
                        .as_ref()
                        .context("Missing pre-execution baseline")?;
                    ensure!(
                        b.captured_at >= r.plan.created_at
                            && b.boot_time > 0
                            && b.boot_time <= b.captured_at,
                        "Invalid baseline"
                    );
                    let approval = r.approval.as_ref().context("Baseline without approval")?;
                    ensure!(
                        b.captured_at >= approval.approved_at
                            && b.captured_at < approval.expires_at,
                        "Baseline outside approval window"
                    );
                }
                if let Some(p) = &s.process {
                    ensure!(p.pid > 0 && p.creation_time > 0, "Invalid process identity");
                }
                if matches!(s.state, StepState::Running | StepState::Monitoring) {
                    ensure!(s.process.is_some(), "Missing process identity");
                }
                if s.state == StepState::Monitoring {
                    ensure!(s.stop_reason.is_some(), "Missing monitor reason");
                }
                if s.state == StepState::Succeeded {
                    let valid = match r.plan.steps[index].operation.kind {
                        OperationKind::DismCheckHealth | OperationKind::DismScanHealth => matches!(
                            s.evidence,
                            Some(
                                Evidence::ComponentStoreHealthy
                                    | Evidence::ComponentStoreRepairable
                                    | Evidence::ComponentStoreNonRepairable
                            )
                        ),
                        OperationKind::DismRestoreHealth => {
                            s.evidence == Some(Evidence::ComponentStoreHealthy)
                        }
                        OperationKind::SfcVerify => {
                            s.evidence == Some(Evidence::DiagnosticCompleted)
                        }
                        OperationKind::SfcRepair => false,
                        OperationKind::DefenderQuickScan => {
                            s.evidence == Some(Evidence::DefenderScanCompleted)
                        }
                    };
                    ensure!(valid, "Success without operation-specific evidence");
                }
                if s.state == StepState::RebootRequired {
                    ensure!(s.exit_code == Some(3010), "Reboot without OS evidence");
                }
            }
        }
        Ok(())
    }
}

pub(super) fn update_idle(bytes: &[u8], machine: &str) -> Result<()> {
    ensure!(
        bytes.len() <= MAX_STATE_BYTES,
        "Operations state size cap exceeded"
    );
    let state: State = serde_json::from_slice(bytes)?;
    state.validate(machine)?;
    ensure!(!state.plans.iter().any(|r| r.steps.iter().any(StepRecord::unresolved)), "Deferred: interrupted maintenance requires independent verification before update installation");
    Ok(())
}

impl<S: Storage, B: Backend> Engine<S, B> {
    pub fn open(mut store: S, backend: B) -> Result<Self> {
        let machine = backend.machine()?;
        ensure!(hash_valid(&machine), "Invalid machine binding");
        let state = match store.load()? {
            Some(bytes) => {
                ensure!(
                    bytes.len() <= MAX_STATE_BYTES,
                    "Operations state size cap exceeded"
                );
                let state: State = serde_json::from_slice(&bytes)
                    .context("Invalid operations state; no repair/replay attempted")?;
                state.validate(&machine)?;
                state
            }
            None => State {
                schema: SCHEMA,
                machine,
                revision: 0,
                policy: OwnerPolicy::default(),
                plans: Vec::new(),
            },
        };
        Ok(Self {
            state,
            store,
            backend,
            poisoned: false,
        })
    }
    fn save(&mut self) -> Result<()> {
        ensure!(
            !self.poisoned,
            "Operations storage failed; reopen and verify"
        );
        let result = (|| {
            self.state.revision = self
                .state
                .revision
                .checked_add(1)
                .context("Revision exhausted")?;
            self.state.validate(&self.state.machine)?;
            let bytes = serde_json::to_vec(&self.state)?;
            ensure!(
                bytes.len() <= MAX_STATE_BYTES,
                "Operations state size cap exceeded"
            );
            self.store.save(&bytes)
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub fn record(&self, id: Uuid) -> Result<&PlanRecord> {
        self.state
            .plans
            .iter()
            .find(|r| r.plan.id == id)
            .context("Unknown operation plan")
    }
    fn index(&self, id: Uuid) -> Result<usize> {
        self.state
            .plans
            .iter()
            .position(|r| r.plan.id == id)
            .context("Unknown operation plan")
    }
    pub fn set_policy(&mut self, policy: OwnerPolicy) -> Result<()> {
        policy_valid(&policy)?;
        let now = self.backend.time()?;
        if let Some(until) = policy.opt_in_until {
            ensure!(
                until > now && until - now <= 30 * 86400,
                "Owner opt-in must expire within 30 days"
            );
        }
        ensure!(
            policy
                .exceptions
                .iter()
                .all(|e| e.expires_at > now && e.expires_at - now <= 86400),
            "Exceptions must expire within 24 hours"
        );
        self.state.policy = policy;
        self.save()
    }
    pub fn plan(&mut self, request: PlanRequest) -> Result<Plan> {
        ensure!(
            self.state.plans.len() < MAX_PLANS,
            "Record cap reached; archive through a future verified retention workflow"
        );
        ensure!(
            (1..=MAX_PLAN_SECONDS).contains(&request.valid_for_seconds),
            "Invalid plan lifetime"
        );
        let time = self.backend.time()?;
        let steps = steps(&request.operations)?;
        for step in &steps {
            permission(&self.state.policy, step.operation.kind, time)?;
        }
        let mut plan = Plan {
            schema: SCHEMA,
            id: Uuid::new_v4(),
            machine: self.state.machine.clone(),
            created_at: time,
            expires_at: time
                .checked_add(request.valid_for_seconds)
                .context("Clock overflow")?,
            policy: self.state.policy.clone(),
            steps,
            digest: String::new(),
        };
        plan.digest = plan.computed_digest()?;
        self.state.plans.push(PlanRecord {
            steps: vec![StepRecord::default(); plan.steps.len()],
            plan: plan.clone(),
            approval: None,
            consumed: false,
        });
        self.save()?;
        Ok(plan)
    }
    pub fn approve(&mut self, id: Uuid, expected: &str, seconds: u64) -> Result<PlanRecord> {
        ensure!(
            (1..=MAX_APPROVAL_SECONDS).contains(&seconds),
            "Approval must expire within 15 minutes"
        );
        let time = self.backend.time()?;
        let i = self.index(id)?;
        let record = &mut self.state.plans[i];
        ensure!(
            !record.consumed
                && expected == record.plan.digest
                && record.plan.policy == self.state.policy,
            "Consumed, changed, or undisplayed plan"
        );
        ensure!(
            time >= record.plan.created_at && time < record.plan.expires_at,
            "Plan expired or clock moved backwards"
        );
        for step in &record.plan.steps {
            permission(&self.state.policy, step.operation.kind, time)?;
        }
        record.approval = Some(Approval {
            digest: expected.into(),
            approved_at: time,
            expires_at: time
                .checked_add(seconds)
                .context("Clock overflow")?
                .min(record.plan.expires_at),
        });
        self.save()?;
        Ok(self.state.plans[i].clone())
    }

    pub fn run(&mut self, id: Uuid, recovery: bool, control: &Control) -> Result<PlanRecord> {
        ensure!(
            !self.poisoned,
            "Operations storage failed; reopen and verify"
        );
        let i = self.index(id)?;
        if recovery {
            ensure!(
                self.state.plans[i].consumed,
                "No attempted operation to verify"
            );
        } else {
            ensure!(
                !self.state.plans[i].consumed,
                "Single-use plan: resume verification, never replay"
            );
            ensure!(
                !self
                    .state
                    .plans
                    .iter()
                    .any(|r| r.steps.iter().any(StepRecord::unresolved)),
                "Unresolved operation: independent verification required first"
            );
            self.authorized(i)?;
            if control.cancelled() {
                bail!("Cancelled before execution");
            }
            // Durable consume BEFORE any preflight/intent/launch. A crash here
            // wastes the approval but can never turn it into a replay token.
            self.state.plans[i].consumed = true;
            self.save()?;
        }
        for j in 0..self.state.plans[i].steps.len() {
            if control.cancelled() {
                break;
            }
            let step = self.state.plans[i].plan.steps[j].clone();
            let old = self.state.plans[i].steps[j].clone();
            if recovery {
                if !old.state.uncertain()
                    && !matches!(old.state, StepState::NeedsReview | StepState::Failed)
                {
                    continue;
                }
                let facts = self
                    .backend
                    .facts(step.operation.kind, old.process.as_ref())?;
                gate(
                    &self.state.policy,
                    step.operation.kind,
                    &facts,
                    self.backend.time()?,
                    true,
                )?;
                let baseline = old.baseline.as_ref().context("Missing recovery baseline")?;
                if old.state == StepState::RebootRequired {
                    ensure!(
                        facts.boot_time != baseline.boot_time,
                        "Owner-initiated reboot has not occurred"
                    );
                }
                self.verify(i, j, &facts, control)?;
                continue;
            }
            self.authorized(i)?; // expiry/policy checked before EACH new operation
            ensure!(
                step.depends_on
                    .iter()
                    .all(|d| self.state.plans[i].steps[*d].state == StepState::Succeeded),
                "Operation dependency did not verify successfully"
            );
            let facts = self.backend.facts(step.operation.kind, None)?;
            gate(
                &self.state.policy,
                step.operation.kind,
                &facts,
                self.backend.time()?,
                false,
            )?;
            self.authorized(i)?; // preflight itself may have consumed the approval lifetime
            if control.cancelled() {
                break;
            }
            self.state.plans[i].steps[j].baseline = Some(Baseline {
                captured_at: facts.captured_at,
                boot_time: facts.boot_time,
                defender_scan_end: facts.defender_scan_end,
            });
            self.transition(i, j, StepState::Intent, control)?;
            self.authorized(i)?;
            gate(
                &self.state.policy,
                step.operation.kind,
                &facts,
                self.backend.time()?,
                false,
            )?;
            let execution = self.invoke(i, j, false, &facts, control);
            if self.poisoned {
                bail!("Durability lost after launch; child supervised to exit; reopen and verify");
            }
            let execution = match execution {
                Ok(result) => result,
                Err(_) => {
                    // A failed launcher/acknowledgement is not proof that an
                    // asynchronous OS action ended. Keep durable uncertainty so
                    // another plan/updater cannot bypass verify-only recovery.
                    break;
                }
            };
            self.state.plans[i].steps[j].exit_code = Some(execution.code);
            if execution.code == 3010 {
                self.transition(i, j, StepState::RebootRequired, control)?;
                break;
            }
            if execution.code != 0 {
                self.transition(i, j, StepState::Failed, control)?;
                break;
            }
            if step.operation.kind.diagnostic() {
                self.state.plans[i].steps[j].evidence = Some(execution.evidence);
                let state = if execution.evidence == Evidence::Inconclusive {
                    StepState::NeedsReview
                } else {
                    StepState::Succeeded
                };
                self.transition(i, j, state, control)?;
            } else {
                // Persist completed command before verifier begins. Cancellation
                // and timeout leave a durable verify-only recovery requirement.
                self.transition(i, j, StepState::Verifying, control)?;
                if control.cancelled() || self.state.plans[i].steps[j].stop_reason.is_some() {
                    break;
                }
                let facts = self.backend.facts(step.operation.kind, None)?;
                gate(
                    &self.state.policy,
                    step.operation.kind,
                    &facts,
                    self.backend.time()?,
                    true,
                )?;
                self.verify(i, j, &facts, control)?;
            }
            if self.state.plans[i].steps[j].state != StepState::Succeeded
                || self.state.plans[i].steps[j].stop_reason.is_some()
            {
                break;
            }
        }
        // Pending steps are not resumable execution: a new exact plan is needed.
        for j in 0..self.state.plans[i].steps.len() {
            if self.state.plans[i].steps[j].state == StepState::Pending {
                self.transition(i, j, StepState::Cancelled, control)?;
            }
        }
        Ok(self.state.plans[i].clone())
    }
    fn authorized(&self, i: usize) -> Result<()> {
        let r = &self.state.plans[i];
        let a = r
            .approval
            .as_ref()
            .context("Exact plan approval required")?;
        let now = self.backend.time()?;
        ensure!(
            now >= a.approved_at
                && now < a.expires_at
                && now < r.plan.expires_at
                && self.state.policy == r.plan.policy,
            "Approval expired, policy changed, or clock moved backwards"
        );
        for s in &r.plan.steps {
            permission(&self.state.policy, s.operation.kind, now)?;
        }
        Ok(())
    }
    fn transition(&mut self, i: usize, j: usize, to: StepState, control: &Control) -> Result<()> {
        ensure!(
            transition_allowed(self.state.plans[i].steps[j].state, to),
            "Invalid durable operation transition"
        );
        self.state.plans[i].steps[j].state = to;
        self.save()?;
        control.publish(j, &self.state.plans[i].steps[j], 0);
        Ok(())
    }
    fn invoke(
        &mut self,
        i: usize,
        j: usize,
        verify: bool,
        facts: &Facts,
        control: &Control,
    ) -> Result<Execution> {
        let kind = self.state.plans[i].plan.steps[j].operation.kind;
        let baseline = self.state.plans[i].steps[j]
            .baseline
            .clone()
            .context("Missing baseline")?;
        let approval = self.state.plans[i]
            .approval
            .as_ref()
            .context("Missing approval")?;
        let permit = launch_permit(
            &self.state.policy,
            kind,
            facts,
            self.backend.time()?,
            (!verify).then_some(approval.expires_at),
        )?;
        let mut failure = false;
        // Split borrows; callback storage failures stop future work but never
        // unwind out of the native supervisor while a servicing child is alive.
        let state = &mut self.state;
        let store = &mut self.store;
        let mut notify = |event| {
            if failure {
                return;
            }
            let record = &mut state.plans[i].steps[j];
            let before = record.state;
            match event {
                Event::Tick(elapsed) => {
                    control.publish(j, record, elapsed);
                    return;
                }
                Event::Spawned(process) => {
                    record.process = Some(process);
                    record.state = if verify {
                        StepState::Verifying
                    } else {
                        StepState::Running
                    };
                }
                Event::Stop(reason) => {
                    record.stop_reason = Some(reason);
                    record.state = if verify {
                        StepState::Verifying
                    } else {
                        StepState::Monitoring
                    };
                }
            }
            let result = (|| {
                ensure!(
                    transition_allowed(before, state.plans[i].steps[j].state),
                    "Invalid native event transition"
                );
                state.revision = state
                    .revision
                    .checked_add(1)
                    .context("Revision exhausted")?;
                state.validate(&state.machine)?;
                let bytes = serde_json::to_vec(state)?;
                ensure!(bytes.len() <= MAX_STATE_BYTES, "State cap exceeded");
                store.save(&bytes)
            })();
            if result.is_err() {
                failure = true;
                control.cancel.store(true, Ordering::SeqCst);
            }
            control.publish(j, &state.plans[i].steps[j], 0);
        };
        let result = if verify {
            self.backend
                .verify(kind, &baseline, &permit, control, &mut notify)
                .map(|evidence| Execution { code: 0, evidence })
        } else {
            self.backend.execute(kind, &permit, control, &mut notify)
        };
        self.poisoned |= failure;
        result
    }
    fn verify(&mut self, i: usize, j: usize, facts: &Facts, control: &Control) -> Result<()> {
        self.transition(i, j, StepState::Verifying, control)?;
        let result = self.invoke(i, j, true, facts, control);
        ensure!(
            !self.poisoned,
            "Verification durability failed; reopen and verify"
        );
        let Ok(result) = result else {
            // A failed verifier must not clear the updater/operation interlock.
            return Ok(());
        };
        let evidence = result.evidence;
        self.state.plans[i].steps[j].evidence = Some(evidence);
        let kind = self.state.plans[i].plan.steps[j].operation.kind;
        if kind == OperationKind::DefenderQuickScan && evidence != Evidence::DefenderScanCompleted {
            // The service may still own a scan after its submitting client exits.
            return self.save();
        }
        let success = match kind {
            OperationKind::DismRestoreHealth => evidence == Evidence::ComponentStoreHealthy,
            OperationKind::SfcRepair => false, // no locale-independent CBS integrity proof yet
            OperationKind::DefenderQuickScan => evidence == Evidence::DefenderScanCompleted,
            _ => evidence != Evidence::Inconclusive,
        };
        self.transition(
            i,
            j,
            if success {
                StepState::Succeeded
            } else {
                StepState::NeedsReview
            },
            control,
        )
    }
}

fn permission(policy: &OwnerPolicy, kind: OperationKind, now: u64) -> Result<()> {
    ensure!(
        policy.allowed.contains(&kind),
        "Operation not enabled by owner policy"
    );
    ensure!(
        kind.diagnostic() || policy.opt_in_until.is_some_and(|t| now < t),
        "Owner opt-in expired"
    );
    Ok(())
}

fn launch_permit(
    policy: &OwnerPolicy,
    kind: OperationKind,
    facts: &Facts,
    time: u64,
    approval_expires: Option<u64>,
) -> Result<LaunchPermit> {
    let verification = approval_expires.is_none();
    gate(policy, kind, facts, time, verification)?;
    let mut expires_at = facts.captured_at.saturating_add(31);
    if let Some(approval) = approval_expires {
        expires_at = expires_at.min(approval);
        if !kind.diagnostic() {
            expires_at = expires_at.min(policy.opt_in_until.unwrap_or(0));
        }
    }
    let excepted_window = policy.exceptions.iter().any(|e| {
        e.operation == kind && e.scope == ExceptionScope::MaintenanceWindow && e.expires_at > time
    });
    if !excepted_window {
        // A freshness budget must not extend the half-open maintenance window.
        let end_today = (time / 86400) * 86400 + u64::from(policy.window.end_minute_utc) * 60;
        let end = if end_today <= time {
            end_today.saturating_add(86400)
        } else {
            end_today
        };
        expires_at = expires_at.min(end);
    }
    for exception in &policy.exceptions {
        if exception.operation == kind && exception.expires_at > time {
            expires_at = expires_at.min(exception.expires_at);
        }
    }
    Ok(LaunchPermit {
        not_before: time,
        expires_at,
    })
}

fn gate(
    policy: &OwnerPolicy,
    kind: OperationKind,
    f: &Facts,
    now: u64,
    verification: bool,
) -> Result<()> {
    ensure!(
        now >= f.captured_at && now - f.captured_at <= 30,
        "Preconditions are stale or clock moved backwards"
    );
    ensure!(
        f.elevated && f.unmanaged,
        "Elevation and confirmed unmanaged ownership required"
    );
    // One named reason each, so the person is told the step that actually helps.
    ensure!(!f.reboot_pending, "Deferred: Windows is waiting for a restart");
    ensure!(!f.busy, "Deferred: Windows servicing is busy");
    ensure!(f.ac, "Deferred: not plugged in");
    ensure!(f.storage_ready, "Deferred: low disk space");
    if !verification {
        permission(policy, kind, now)?;
    }
    let excepted = |scope| {
        policy
            .exceptions
            .iter()
            .any(|e| e.operation == kind && e.scope == scope && now < e.expires_at)
    };
    ensure!(
        f.idle_seconds
            .is_some_and(|idle| idle >= policy.idle_seconds)
            || excepted(ExceptionScope::ActiveUse),
        "Deferred: active use or unknown desktop idle state"
    );
    let minute = ((now / 60) % 1440) as u16;
    let w = &policy.window;
    let inside = if w.start_minute_utc < w.end_minute_utc {
        minute >= w.start_minute_utc && minute < w.end_minute_utc
    } else {
        minute >= w.start_minute_utc || minute < w.end_minute_utc
    };
    ensure!(
        inside || excepted(ExceptionScope::MaintenanceWindow),
        "Deferred: outside UTC maintenance window"
    );
    ensure!(
        verification
            || !kind.spec().network_access
            || f.unmetered == Some(true)
            || excepted(ExceptionScope::MeteredNetwork),
        "Deferred: metered/unknown network"
    );
    Ok(())
}

#[cfg(not(windows))]
pub(super) struct Unsupported;
#[cfg(not(windows))]
impl Storage for Unsupported {
    fn load(&mut self) -> Result<Option<Vec<u8>>> {
        bail!("Windows only")
    }
    fn save(&mut self, _: &[u8]) -> Result<()> {
        bail!("Windows only")
    }
}
#[cfg(not(windows))]
impl Backend for Unsupported {
    fn machine(&self) -> Result<String> {
        bail!("Windows only")
    }
    fn facts(&mut self, _: OperationKind, _: Option<&ProcessIdentity>) -> Result<Facts> {
        bail!("Windows only")
    }
    fn execute(
        &mut self,
        _: OperationKind,
        _: &LaunchPermit,
        _: &Control,
        _: &mut dyn FnMut(Event),
    ) -> Result<Execution> {
        bail!("Windows only")
    }
    fn verify(
        &mut self,
        _: OperationKind,
        _: &Baseline,
        _: &LaunchPermit,
        _: &Control,
        _: &mut dyn FnMut(Event),
    ) -> Result<Evidence> {
        bail!("Windows only")
    }
}
