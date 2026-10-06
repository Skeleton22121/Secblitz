use super::*;
use std::collections::BTreeSet;

/// Unlike operations' `Storage`, this also refuses to run while another engine is active.
pub(super) trait Storage {
    fn load(&mut self) -> Result<Option<Vec<u8>>>;
    fn save(&mut self, bytes: &[u8]) -> Result<()>;
    fn other_operations_idle(&self) -> Result<()>;
}
#[derive(Clone, Copy)]
pub(super) enum Phase {
    Download,
    Install,
}
pub(super) enum Event {
    Spawned(ProcessIdentity),
    /// Direct process AND descendants exited; the synchronous WUA phase returned
    /// its exact success acknowledgement. Exit code alone cannot emit this.
    PhaseFinished,
    Timeout,
}
/// Patching backend: Windows Update discovery, then download/install phases and verification.
pub(super) trait Backend {
    fn set_cancel(&mut self, _cancel: Arc<AtomicBool>) {}
    fn binding(&self) -> Result<Binding>;
    fn time(&self) -> Result<u64> {
        now()
    }
    fn discover(&mut self) -> Result<Catalog>;
    /// Implementation must compare ALL metadata and repeat live readiness and
    /// management checks immediately before each irreversible boundary.
    /// Callback Err forbids delivery/submission but NEVER ends supervision.
    fn execute(
        &mut self,
        phase: Phase,
        plan: &Plan,
        approval: &Approval,
        notify: &mut dyn FnMut(Event) -> Result<()>,
    ) -> Result<()>;
    fn verify(&mut self, plan: &Plan, process: Option<&ProcessIdentity>) -> Result<Verification>;
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    schema: u32,
    binding_machine: String,
    records: Vec<Record>,
}

fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn consent_valid(c: &Consent) -> bool {
    c.owner_opt_in
        && c.accept_windows_update_source
        && c.accept_reviewed_eulas
        && c.acknowledge_no_automatic_rollback
}
pub(super) fn identities(updates: &[Update]) -> Vec<UpdateIdentity> {
    let mut result = Vec::new();
    for u in updates {
        result.push(u.identity.clone());
        result.extend(identities(&u.bundled));
    }
    result
}
fn updates_valid(updates: &[Update]) -> Result<()> {
    fn walk(
        updates: &[Update],
        depth: usize,
        count: &mut usize,
        seen: &mut BTreeSet<Uuid>,
    ) -> Result<()> {
        ensure!(
            depth <= 4 && updates.len() <= MAX_UPDATES,
            "Update/bundle depth or count exceeded"
        );
        for u in updates {
            *count += 1;
            ensure!(
                *count <= 128
                    && !u.identity.update_id.is_nil()
                    && u.identity.revision > 0
                    && seen.insert(u.identity.update_id),
                "Invalid/duplicate update identity"
            );
            ensure!(
                !u.title.is_empty()
                    && u.title.len() <= 4096
                    && u.description.len() <= 32768
                    && u.eula.len() <= 65536
                    && u.categories.len() <= 128
                    && u.kb_articles.len() <= 32
                    && u.categories.iter().collect::<BTreeSet<_>>().len() == u.categories.len()
                    && u.kb_articles.iter().all(|s| !s.is_empty()
                        && s.len() <= 32
                        && s.bytes().all(|b| b.is_ascii_digit()))
                    && u.handler.len() <= 1024
                    && u.last_changed.len() <= 64
                    && !u.last_changed.is_empty()
                    && u.last_changed.bytes().all(|b| b.is_ascii_digit())
                    && u.severity.len() <= 64
                    && u.max_download_bytes <= 64 * 1024 * 1024 * 1024
                    && u.reboot_behavior <= 2,
                "Update metadata outside bounds"
            );
            let has = |id: u128| u.categories.contains(&Uuid::from_u128(id));
            let title = u.title.to_ascii_lowercase();
            ensure!(
                has(0x6964aab4_c5b5_43bd_a17d_ffb4346a8e1d)
                    && !has(0xebfc1fc5_71a4_4f7b_9aca_3b9a503104a0)
                    && !has(0x3689bdc8_b205_4af4_8d4a_a63924c5e9d5)
                    && !has(0xb54e7d24_7add_428f_8b75_90a396fa584f)
                    && (depth != 0
                        || has(0x0fa1201d_4330_4fa8_8ae9_b877473b6441)
                        || has(0xe6cf1350_c01b_414d_a61f_263d14d133b4))
                    && ![
                        "preview",
                        "insider",
                        "feature update",
                        "enablement package",
                        "upgrade to"
                    ]
                    .iter()
                    .any(|word| title.contains(word))
                    && !title
                        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                        .any(|word| word == "beta"),
                "Update/bundle is not positively classified as supported Windows quality content"
            );
            walk(&u.bundled, depth + 1, count, seen)?;
        }
        Ok(())
    }
    ensure!(!updates.is_empty(), "Select at least one update");
    walk(updates, 0, &mut 0, &mut BTreeSet::new())?;
    fn bytes(updates: &[Update]) -> Result<u64> {
        let mut sum = 0u64;
        for u in updates {
            sum = sum
                .checked_add(u.max_download_bytes.max(bytes(&u.bundled)?))
                .context("Download size overflow")?;
            ensure!(
                sum <= 64 * 1024 * 1024 * 1024,
                "Combined download size exceeds cap"
            );
        }
        Ok(sum)
    }
    bytes(updates)?;
    Ok(())
}
impl Plan {
    pub fn computed_digest(&self) -> Result<String> {
        let mut p = self.clone();
        p.digest.clear();
        hash(&p)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == SCHEMA
                && self.id.get_version_num() == 4
                && valid_hash(&self.binding.machine)
                && valid_hash(&self.binding.original_user)
                && self.source == SOURCE,
            "Invalid plan binding/source/schema"
        );
        ensure!(
            self.created_at > 0
                && self.expires_at > self.created_at
                && self.expires_at - self.created_at <= MAX_PLAN_SECONDS,
            "Invalid plan lifetime"
        );
        updates_valid(&self.updates)?;
        ensure!(
            valid_hash(&self.digest) && self.digest == self.computed_digest()?,
            "Exact plan digest mismatch"
        );
        Ok(())
    }
}
impl State {
    fn validate(&self, machine: &str) -> Result<()> {
        ensure!(
            self.schema == SCHEMA
                && self.binding_machine == machine
                && valid_hash(machine)
                && self.records.len() <= MAX_RECORDS,
            "Invalid patching store/schema/machine"
        );
        let mut seen = BTreeSet::new();
        for r in &self.records {
            r.plan.validate()?;
            ensure!(
                seen.insert(r.plan.id) && r.plan.binding.machine == machine,
                "Duplicate/foreign plan"
            );
            if let Some(a) = &r.approval {
                ensure!(
                    a.digest == r.plan.digest
                        && consent_valid(&a.consent)
                        && a.approved_at >= r.plan.created_at
                        && a.expires_at > a.approved_at
                        && a.expires_at <= r.plan.expires_at
                        && a.expires_at - a.approved_at <= MAX_APPROVAL_SECONDS,
                    "Invalid approval"
                );
            }
            ensure!(
                r.status == Status::Planned || r.approval.is_some(),
                "Attempt without consent"
            );
            if r.status == Status::Planned {
                ensure!(
                    r.process.is_none() && r.verification.is_none() && !r.uncertain,
                    "Unused plan contains execution"
                );
            }
            if let Some(p) = &r.process {
                ensure!(
                    p.pid > 0 && p.creation_time > 0 && p.boot_id.is_none_or(|id| !id.is_nil()),
                    "Invalid process identity"
                );
            }
            if let Some(v) = &r.verification {
                let expected = identities(&r.plan.updates);
                ensure!(
                    v.checked_at >= r.plan.created_at
                        && v.installed.len() <= expected.len()
                        && v.installed.iter().all(|i| expected.contains(i))
                        && v.installed.iter().collect::<BTreeSet<_>>().len() == v.installed.len(),
                    "Invalid verification identities"
                );
            }
            if matches!(r.status, Status::Succeeded | Status::RebootRequired) {
                ensure!(
                    r.process.is_none(),
                    "Success retains unconfirmed servicing lifetime"
                );
                let v = r
                    .verification
                    .as_ref()
                    .context("Success without independent verification")?;
                ensure!(
                    v.installed.len() == identities(&r.plan.updates).len()
                        && v.reboot_pending == (r.status == Status::RebootRequired),
                    "Incomplete success evidence"
                );
            }
        }
        Ok(())
    }
}
pub(super) fn idle(bytes: &[u8], machine: &str) -> Result<()> {
    ensure!(
        bytes.len() <= MAX_STATE_BYTES,
        "Patching state cap exceeded"
    );
    let s: State = serde_json::from_slice(bytes)?;
    s.validate(machine)?;
    ensure!(
        s.records
            .iter()
            .all(|r| matches!(r.status, Status::Planned | Status::Succeeded)),
        "Deferred: unresolved patching intent/reboot; independent verification required"
    );
    Ok(())
}
pub(super) fn recovery_after_boot(process: &ProcessIdentity, current: Uuid) -> Result<()> {
    ensure!(
        !current.is_nil()
            && process
                .boot_id
                .is_some_and(|id| !id.is_nil() && id != current),
        "Unconfirmed WUA phase requires a subsequent boot before verification"
    );
    Ok(())
}
pub(super) struct Engine<S, B> {
    store: S,
    backend: B,
    state: State,
    poisoned: bool,
    cancel: Arc<AtomicBool>,
}
impl<S: Storage, B: Backend> Engine<S, B> {
    pub fn open(mut store: S, mut backend: B) -> Result<Self> {
        let cancel = Arc::new(AtomicBool::new(false));
        backend.set_cancel(cancel.clone());
        let binding = backend.binding()?;
        let state = match store.load()? {
            Some(bytes) => {
                ensure!(
                    bytes.len() <= MAX_STATE_BYTES,
                    "Patching state cap exceeded"
                );
                serde_json::from_slice(&bytes)?
            }
            None => State {
                schema: SCHEMA,
                binding_machine: binding.machine.clone(),
                records: Vec::new(),
            },
        };
        state.validate(&binding.machine)?;
        Ok(Self {
            store,
            backend,
            state,
            poisoned: false,
            cancel,
        })
    }
    pub fn cancellation(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }
    fn live(&self) -> Result<()> {
        ensure!(
            !self.poisoned,
            "Patching durability lost; reopen and verify"
        );
        ensure!(
            !self.cancel.load(Ordering::SeqCst),
            "Patching cancelled; no later phase submitted"
        );
        Ok(())
    }
    fn save(&mut self) -> Result<()> {
        ensure!(
            !self.poisoned,
            "Patching durability lost; reopen and verify"
        );
        let result = (|| {
            self.state.validate(&self.state.binding_machine)?;
            let bytes = serde_json::to_vec(&self.state)?;
            ensure!(
                bytes.len() <= MAX_STATE_BYTES,
                "Patching state cap exceeded"
            );
            self.store.save(&bytes)
        })();
        self.poisoned |= result.is_err();
        result
    }
    pub fn records(&self) -> &[Record] {
        &self.state.records
    }
    pub fn record(&self, id: Uuid) -> Result<&Record> {
        Ok(&self.state.records[self.index(id)?])
    }
    fn index(&self, id: Uuid) -> Result<usize> {
        self.state
            .records
            .iter()
            .position(|r| r.plan.id == id)
            .context("Unknown patching plan")
    }
    fn bound(&self, i: usize) -> Result<()> {
        ensure!(
            self.backend.binding()? == self.state.records[i].plan.binding,
            "Machine/original-user binding changed"
        );
        Ok(())
    }
    fn authorized(&self, i: usize, digest: &str) -> Result<()> {
        self.bound(i)?;
        let r = &self.state.records[i];
        let a = r
            .approval
            .as_ref()
            .context("Explicit exact-plan approval required")?;
        let t = self.backend.time()?;
        ensure!(
            digest == r.plan.digest
                && a.digest == digest
                && t >= a.approved_at
                && t < a.expires_at
                && t < r.plan.expires_at,
            "Wrong digest, expired consent or clock rollback"
        );
        Ok(())
    }
    pub fn discover(&mut self) -> Result<Catalog> {
        self.live()?;
        ensure!(
            self.state
                .records
                .iter()
                .all(|r| matches!(r.status, Status::Planned | Status::Succeeded)),
            "Unresolved patching: verify before another online search"
        );
        self.store.other_operations_idle()?;
        let started = self.backend.time()?;
        let c = self.backend.discover()?;
        self.live()?;
        ensure!(
            c.binding == self.backend.binding()?
                && c.source == SOURCE
                && c.searched_at >= started
                && c.searched_at <= self.backend.time()?,
            "Invalid discovery binding/source/time"
        );
        if !c.updates.is_empty() {
            updates_valid(&c.updates)?;
        }
        Ok(c)
    }
    pub fn plan(&mut self, req: PlanRequest) -> Result<Plan> {
        self.live()?;
        ensure!(
            (1..=MAX_PLAN_SECONDS).contains(&req.valid_for_seconds)
                && !req.selected.is_empty()
                && req.selected.len() <= MAX_UPDATES
                && req.selected.iter().collect::<BTreeSet<_>>().len() == req.selected.len(),
            "Invalid selected identities/lifetime"
        );
        ensure!(
            self.state.records.len() < MAX_RECORDS,
            "Patching record cap reached; no automatic record deletion"
        );
        let c = self.discover()?;
        let updates = req
            .selected
            .iter()
            .map(|id| {
                c.updates
                    .iter()
                    .find(|u| &u.identity == id)
                    .cloned()
                    .context("Selected update/revision no longer applicable or eligible")
            })
            .collect::<Result<Vec<_>>>()?;
        let created_at = self.backend.time()?;
        let mut p = Plan {
            schema: SCHEMA,
            id: Uuid::new_v4(),
            binding: c.binding,
            created_at,
            expires_at: created_at
                .checked_add(req.valid_for_seconds)
                .context("Clock overflow")?,
            source: SOURCE.into(),
            updates,
            digest: String::new(),
        };
        p.digest = p.computed_digest()?;
        p.validate()?;
        self.state.records.push(Record {
            plan: p.clone(),
            approval: None,
            status: Status::Planned,
            process: None,
            uncertain: false,
            verification: None,
        });
        self.save()?;
        Ok(p)
    }
    pub fn approve(
        &mut self,
        id: Uuid,
        digest: &str,
        consent: Consent,
        seconds: u64,
    ) -> Result<Record> {
        self.live()?;
        ensure!(
            consent_valid(&consent) && (1..=MAX_APPROVAL_SECONDS).contains(&seconds),
            "Explicit owner/source/EULA/rollback consent and bounded lifetime required"
        );
        let i = self.index(id)?;
        self.bound(i)?;
        let t = self.backend.time()?;
        let r = &mut self.state.records[i];
        ensure!(
            r.status == Status::Planned
                && digest == r.plan.digest
                && t >= r.plan.created_at
                && t < r.plan.expires_at,
            "Consumed, undisplayed or expired plan"
        );
        r.approval = Some(Approval {
            digest: digest.into(),
            approved_at: t,
            expires_at: t
                .checked_add(seconds)
                .context("Clock overflow")?
                .min(r.plan.expires_at),
            consent,
        });
        self.save()?;
        Ok(self.state.records[i].clone())
    }
    pub fn run(&mut self, id: Uuid, digest: &str) -> Result<Record> {
        self.live()?;
        let i = self.index(id)?;
        ensure!(
            self.state.records[i].status == Status::Planned,
            "Single-use plan: verify only; never replay"
        );
        ensure!(
            self.state
                .records
                .iter()
                .all(|r| matches!(r.status, Status::Planned | Status::Succeeded)),
            "Unresolved patching blocks new work"
        );
        self.authorized(i, digest)?;
        self.store.other_operations_idle()?;
        self.state.records[i].status = Status::Consumed;
        self.save()?;
        let result = self.attempt(i, digest);
        if result.is_err() {
            self.state.records[i].uncertain = true;
            self.state.records[i].status = Status::NeedsReview;
            self.save()?;
        }
        Ok(self.state.records[i].clone())
    }
    fn attempt(&mut self, i: usize, digest: &str) -> Result<()> {
        for phase in [Phase::Download, Phase::Install] {
            self.live()?;
            self.authorized(i, digest)?;
            self.store.other_operations_idle()?;
            self.state.records[i].status = match phase {
                Phase::Download => Status::Downloading,
                Phase::Install => Status::Installing,
            };
            self.save()?;
            let r = self.state.records[i].clone();
            let mut failed = false;
            let mut completed = false;
            let state = &mut self.state;
            let store = &mut self.store;
            let result = self.backend.execute(
                phase,
                &r.plan,
                r.approval.as_ref().context("Missing approval")?,
                &mut |event| {
                    ensure!(!failed, "Durability failure");
                    let finishing = match event {
                        Event::Spawned(p) => {
                            state.records[i].process = Some(p);
                            false
                        }
                        Event::PhaseFinished => {
                            ensure!(
                                state.records[i].process.is_some(),
                                "Phase completion without recorded process"
                            );
                            state.records[i].process = None;
                            true
                        }
                        Event::Timeout => {
                            state.records[i].uncertain = true;
                            false
                        }
                    };
                    let saved = (|| {
                        state.validate(&state.binding_machine)?;
                        let bytes = serde_json::to_vec(state)?;
                        ensure!(bytes.len() <= MAX_STATE_BYTES, "State cap");
                        store.save(&bytes)
                    })();
                    failed |= saved.is_err();
                    completed |= saved.is_ok() && finishing;
                    saved
                },
            );
            self.poisoned |= failed;
            result?;
            ensure!(
                completed && self.state.records[i].process.is_none(),
                "WUA phase returned without durable completion acknowledgement"
            );
            ensure!(
                !self.state.records[i].uncertain,
                "Timed out; subsequent phases forbidden"
            );
            if matches!(phase, Phase::Download) {
                self.state.records[i].status = Status::Downloaded;
                self.save()?;
            }
        }
        self.verify_index(i)
    }
    pub fn verify(&mut self, id: Uuid) -> Result<Record> {
        self.live()?;
        let i = self.index(id)?;
        self.bound(i)?;
        ensure!(
            self.state.records[i].status != Status::Planned,
            "No attempted patching to verify"
        );
        // Read-only recovery never uses the old approval as an execution permit.
        if !matches!(
            self.state.records[i].status,
            Status::Succeeded | Status::RebootRequired
        ) {
            self.state.records[i].uncertain = true;
        }
        let result = self.verify_index(i);
        if result.is_err() {
            self.state.records[i].status = Status::NeedsReview;
            self.state.records[i].uncertain = true;
            self.save()?;
        }
        Ok(self.state.records[i].clone())
    }
    fn verify_index(&mut self, i: usize) -> Result<()> {
        self.live()?;
        self.bound(i)?;
        self.store.other_operations_idle()?;
        self.state.records[i].status = Status::Verifying;
        self.save()?;
        let r = &self.state.records[i];
        let started = self.backend.time()?;
        let v = self.backend.verify(&r.plan, r.process.as_ref())?;
        let expected = identities(&r.plan.updates);
        let actual: BTreeSet<_> = v.installed.iter().cloned().collect();
        ensure!(
            actual.len() == v.installed.len()
                && actual.iter().all(|id| expected.contains(id))
                && v.checked_at >= started
                && v.checked_at <= self.backend.time()?,
            "Invalid independent evidence"
        );
        self.state.records[i].status = if actual.len() != expected.len() {
            Status::NeedsReview
        } else if v.reboot_pending {
            Status::RebootRequired
        } else {
            Status::Succeeded
        };
        self.state.records[i].verification = Some(v);
        // The native verifier confirmed an abandoned phase cannot survive the
        // current boot, plus exact installed evidence. Retain uncertainty history.
        self.state.records[i].process = None;
        self.save()
    }
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
    fn other_operations_idle(&self) -> Result<()> {
        bail!("Windows only")
    }
}
#[cfg(not(windows))]
impl Backend for Unsupported {
    fn binding(&self) -> Result<Binding> {
        bail!("Windows only")
    }
    fn discover(&mut self) -> Result<Catalog> {
        bail!("Windows only")
    }
    fn execute(
        &mut self,
        _: Phase,
        _: &Plan,
        _: &Approval,
        _: &mut dyn FnMut(Event) -> Result<()>,
    ) -> Result<()> {
        bail!("Windows only")
    }
    fn verify(&mut self, _: &Plan, _: Option<&ProcessIdentity>) -> Result<Verification> {
        bail!("Windows only")
    }
}
