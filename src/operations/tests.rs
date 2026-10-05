use super::{core::*, *};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Disk {
    bytes: Option<Vec<u8>>,
    writes: usize,
    fail: Option<usize>,
    after_publication: bool,
    snapshots: Vec<Vec<u8>>,
}
#[derive(Clone, Default)]
struct Memory(Rc<RefCell<Disk>>);
impl Storage for Memory {
    fn load(&mut self) -> Result<Option<Vec<u8>>> {
        Ok(self.0.borrow().bytes.clone())
    }
    fn save(&mut self, bytes: &[u8]) -> Result<()> {
        let mut disk = self.0.borrow_mut();
        disk.writes += 1;
        let failed = disk.fail == Some(disk.writes);
        if !failed || disk.after_publication {
            disk.bytes = Some(bytes.to_vec());
            disk.snapshots.push(bytes.to_vec());
        }
        ensure!(!failed, "injected durable publication fault");
        Ok(())
    }
}

#[derive(Clone)]
struct Fake(Rc<RefCell<World>>);
struct World {
    time: u64,
    facts: Facts,
    execute: Vec<OperationKind>,
    verify: Vec<OperationKind>,
    supervised: usize,
    exit: u32,
    stop: Option<StopReason>,
    evidence: Evidence,
    error: bool,
    verify_error: bool,
    launch_delay: u64,
    verify_delay: u64,
    disk: Memory,
}
impl Fake {
    fn new(disk: Memory) -> Self {
        Self(Rc::new(RefCell::new(World {
            time: 7200,
            facts: Facts {
                captured_at: 7200,
                elevated: true,
                unmanaged: true,
                ac: true,
                storage_ready: true,
                reboot_pending: false,
                busy: false,
                idle_seconds: Some(600),
                unmetered: Some(true),
                boot_time: 1,
                defender_scan_end: Some(100),
            },
            execute: Vec::new(),
            verify: Vec::new(),
            supervised: 0,
            exit: 0,
            stop: None,
            evidence: Evidence::ComponentStoreHealthy,
            error: false,
            verify_error: false,
            launch_delay: 0,
            verify_delay: 0,
            disk,
        })))
    }
}
impl Backend for Fake {
    fn machine(&self) -> Result<String> {
        Ok("a".repeat(64))
    }
    fn time(&self) -> Result<u64> {
        Ok(self.0.borrow().time)
    }
    fn facts(&mut self, _: OperationKind, _: Option<&ProcessIdentity>) -> Result<Facts> {
        Ok(self.0.borrow().facts.clone())
    }
    fn execute(
        &mut self,
        kind: OperationKind,
        permit: &LaunchPermit,
        _: &Control,
        notify: &mut dyn FnMut(Event),
    ) -> Result<Execution> {
        let mut world = self.0.borrow_mut();
        world.time += world.launch_delay;
        ensure!(
            world.time >= permit.not_before && world.time < permit.expires_at,
            "Launch permit expired"
        );
        let saved: State =
            serde_json::from_slice(world.disk.0.borrow().bytes.as_ref().unwrap()).unwrap();
        assert!(
            saved
                .plans
                .iter()
                .any(|r| r.steps.iter().any(|s| s.state == StepState::Intent)),
            "No durable intent before side effect"
        );
        world.execute.push(kind);
        notify(Event::Spawned(ProcessIdentity {
            pid: 42,
            creation_time: 123,
        }));
        notify(Event::Tick(7));
        if let Some(reason) = world.stop {
            notify(Event::Stop(reason));
        }
        // Even a callback persistence failure must reach supervision completion.
        world.supervised += 1;
        ensure!(!world.error, "injected native uncertainty");
        Ok(Execution {
            code: world.exit,
            evidence: if kind == OperationKind::SfcVerify {
                Evidence::DiagnosticCompleted
            } else {
                world.evidence
            },
        })
    }
    fn verify(
        &mut self,
        kind: OperationKind,
        _: &Baseline,
        permit: &LaunchPermit,
        _: &Control,
        notify: &mut dyn FnMut(Event),
    ) -> Result<Evidence> {
        let mut world = self.0.borrow_mut();
        world.time += world.verify_delay;
        ensure!(
            world.time >= permit.not_before && world.time < permit.expires_at,
            "Verification permit expired"
        );
        let saved: State =
            serde_json::from_slice(world.disk.0.borrow().bytes.as_ref().unwrap()).unwrap();
        assert!(
            saved
                .plans
                .iter()
                .any(|r| r.steps.iter().any(|s| s.state == StepState::Verifying)),
            "No durable verifying boundary"
        );
        world.verify.push(kind);
        notify(Event::Spawned(ProcessIdentity {
            pid: 43,
            creation_time: 456,
        }));
        notify(Event::Tick(3));
        if let Some(reason) = world.stop {
            notify(Event::Stop(reason));
        }
        world.supervised += 1;
        ensure!(
            !world.error && !world.verify_error,
            "injected verifier failure"
        );
        Ok(
            if matches!(kind, OperationKind::SfcVerify | OperationKind::SfcRepair) {
                Evidence::DiagnosticCompleted
            } else {
                world.evidence
            },
        )
    }
}
fn control(id: Uuid) -> Control {
    Control {
        cancel: Arc::new(AtomicBool::new(false)),
        progress: Arc::new(Mutex::new(Progress {
            plan_id: id,
            step: None,
            state: None,
            elapsed_seconds: 0,
            stop_reason: None,
        })),
    }
}
fn open() -> (Engine<Memory, Fake>, Memory, Fake) {
    let disk = Memory::default();
    let backend = Fake::new(disk.clone());
    (
        Engine::open(disk.clone(), backend.clone()).unwrap(),
        disk,
        backend,
    )
}
fn approved(kinds: Vec<OperationKind>) -> (Engine<Memory, Fake>, Memory, Fake, Plan) {
    let (mut e, disk, backend) = open();
    let policy = OwnerPolicy {
        allowed: capabilities()
            .operations
            .into_iter()
            .map(|s| s.kind)
            .collect(),
        opt_in_until: Some(7200 + 86400),
        ..OwnerPolicy::default()
    };
    e.set_policy(policy).unwrap();
    let plan = e
        .plan(PlanRequest {
            operations: kinds,
            valid_for_seconds: 3600,
        })
        .unwrap();
    e.approve(plan.id, &plan.digest, 600).unwrap();
    (e, disk, backend, plan)
}

#[test]
fn default_is_diagnostics_only_and_unsupported_paths_are_honest() {
    let (mut e, _, _) = open();
    for kind in [
        OperationKind::DismRestoreHealth,
        OperationKind::SfcRepair,
        OperationKind::DefenderQuickScan,
    ] {
        assert!(e
            .plan(PlanRequest {
                operations: vec![kind],
                valid_for_seconds: 60
            })
            .is_err());
    }
    assert!(!capabilities().selected_app_upgrades && !capabilities().windows_quality_updates);
    assert_eq!(
        OperationKind::SfcRepair.spec().reversibility,
        Reversibility::NoAutomaticRollback
    );
    assert_eq!(
        OperationKind::DefenderQuickScan.spec().risk,
        Risk::AntivirusRemediation
    );
}

#[test]
fn exact_approval_binds_order_dependencies_risk_policy_machine_and_expiry() {
    let (mut e, _, _, plan) = approved(vec![
        OperationKind::SfcRepair,
        OperationKind::DismRestoreHealth,
    ]);
    assert_eq!(
        plan.steps
            .iter()
            .map(|s| s.operation.kind)
            .collect::<Vec<_>>(),
        [
            OperationKind::DismScanHealth,
            OperationKind::DismRestoreHealth,
            OperationKind::SfcVerify,
            OperationKind::SfcRepair
        ]
    );
    assert_eq!(plan.steps[1].depends_on, [0]);
    assert_eq!(plan.steps[3].depends_on, [1, 2]);
    assert!(e.approve(plan.id, &"b".repeat(64), 60).is_err());
    assert!(e.approve(plan.id, &plan.digest, 901).is_err());
    let mut policy = e.state.policy.clone();
    policy.idle_seconds = 301;
    e.set_policy(policy).unwrap();
    assert!(e.approve(plan.id, &plan.digest, 60).is_err());
    assert!(e.run(plan.id, false, &control(plan.id)).is_err());
}

#[test]
fn expires_at_is_exclusive_and_clock_rollback_rejects() {
    for time in [7199, 7800, 11000] {
        let (mut e, _, backend, plan) = approved(vec![OperationKind::DismCheckHealth]);
        backend.0.borrow_mut().time = time;
        assert!(e.run(plan.id, false, &control(plan.id)).is_err());
        assert!(backend.0.borrow().execute.is_empty());
    }
}

#[test]
fn policy_opt_in_and_exceptions_are_scoped_and_expiring() {
    let (mut e, _, _, _) = approved(vec![OperationKind::DismCheckHealth]);
    let mut policy = e.state.policy.clone();
    policy.opt_in_until = None;
    assert!(e.set_policy(policy.clone()).is_err());
    policy.opt_in_until = Some(7200 + 30 * 86400 + 1);
    assert!(e.set_policy(policy.clone()).is_err());
    policy.opt_in_until = Some(8000);
    policy.exceptions = vec![PolicyException {
        operation: OperationKind::DismCheckHealth,
        scope: ExceptionScope::ActiveUse,
        expires_at: 7200,
    }];
    assert!(e.set_policy(policy.clone()).is_err());
    policy.exceptions[0].expires_at = 7300;
    e.set_policy(policy.clone()).unwrap();
    policy.exceptions.push(policy.exceptions[0].clone());
    assert!(e.set_policy(policy).is_err());
}

#[test]
fn every_hard_precondition_fails_closed_and_is_fresh() {
    for bad in 0..9 {
        let (mut e, _, backend, plan) = approved(vec![OperationKind::DismCheckHealth]);
        {
            let mut w = backend.0.borrow_mut();
            match bad {
                0 => w.facts.elevated = false,
                1 => w.facts.unmanaged = false,
                2 => w.facts.ac = false,
                3 => w.facts.storage_ready = false,
                4 => w.facts.reboot_pending = true,
                5 => w.facts.busy = true,
                6 => w.facts.captured_at = 7169,
                7 => w.facts.captured_at = 7201,
                _ => w.facts.idle_seconds = None,
            }
        }
        assert!(
            e.run(plan.id, false, &control(plan.id)).is_err(),
            "gate {bad}"
        );
        assert!(backend.0.borrow().execute.is_empty());
    }
}

#[test]
fn active_use_exception_does_not_override_hard_gates_or_other_operations() {
    let (mut e, _, backend) = open();
    let mut policy = OwnerPolicy::default();
    policy.exceptions.push(PolicyException {
        operation: OperationKind::DismCheckHealth,
        scope: ExceptionScope::ActiveUse,
        expires_at: 7300,
    });
    e.set_policy(policy).unwrap();
    backend.0.borrow_mut().facts.idle_seconds = Some(0);
    let p = e
        .plan(PlanRequest {
            operations: vec![OperationKind::DismCheckHealth],
            valid_for_seconds: 60,
        })
        .unwrap();
    e.approve(p.id, &p.digest, 60).unwrap();
    e.run(p.id, false, &control(p.id)).unwrap();
    let p = e
        .plan(PlanRequest {
            operations: vec![OperationKind::SfcVerify],
            valid_for_seconds: 60,
        })
        .unwrap();
    e.approve(p.id, &p.digest, 60).unwrap();
    assert!(e.run(p.id, false, &control(p.id)).is_err());
    assert_eq!(backend.0.borrow().execute, [OperationKind::DismCheckHealth]);
}

#[test]
fn malformed_unknown_paths_and_credentials_cannot_be_deserialized_as_operations() {
    for text in [
        r#""cmd.exe""#,
        r#"{"dism_restore_health":{"source":"C:\\payload"}}"#,
        r#""upgrade_all""#,
        r#""windows_quality_update""#,
    ] {
        assert!(serde_json::from_str::<OperationKind>(text).is_err());
    }
    let (e, disk, backend, _) = approved(vec![OperationKind::DismCheckHealth]);
    let bytes = disk.0.borrow().bytes.clone().unwrap();
    drop(e);
    for key in ["command", "path", "credentials", "shell"] {
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value[key] = "payload".into();
        disk.0.borrow_mut().bytes = Some(serde_json::to_vec(&value).unwrap());
        assert!(Engine::open(disk.clone(), backend.clone()).is_err());
    }
}

#[test]
fn corruption_caps_identity_and_changed_compiled_specs_reject_before_backend() {
    let (_, disk, backend, _) = approved(vec![OperationKind::DismCheckHealth]);
    let original = disk.0.borrow().bytes.clone().unwrap();
    let mut cases = vec![
        Vec::new(),
        b"{broken".to_vec(),
        vec![b' '; MAX_STATE_BYTES + 1],
    ];
    for bad in 0..6 {
        let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
        match bad {
            0 => value["machine"] = "b".repeat(64).into(),
            1 => value["schema"] = 2.into(),
            2 => value["plans"][0]["plan"]["steps"][0]["operation"]["timeout_seconds"] = 1.into(),
            3 => value["plans"][0]["steps"][0]["state"] = "running".into(),
            4 => value["plans"][0]["plan"]["digest"] = "0".repeat(64).into(),
            _ => value["plans"][0]["approval"]["expires_at"] = 999999.into(),
        }
        cases.push(serde_json::to_vec(&value).unwrap());
    }
    for bytes in cases {
        disk.0.borrow_mut().bytes = Some(bytes);
        assert!(Engine::open(disk.clone(), backend.clone()).is_err());
    }
    assert!(backend.0.borrow().execute.is_empty());
}

#[test]
fn successful_repair_requires_independent_verification_and_plan_is_single_use() {
    let (mut e, _, backend, plan) = approved(vec![OperationKind::DismRestoreHealth]);
    let result = e.run(plan.id, false, &control(plan.id)).unwrap();
    assert!(result.steps.iter().all(|s| s.state == StepState::Succeeded));
    assert_eq!(
        backend.0.borrow().execute,
        [
            OperationKind::DismScanHealth,
            OperationKind::DismRestoreHealth
        ]
    );
    assert_eq!(
        backend.0.borrow().verify,
        [OperationKind::DismRestoreHealth]
    );
    assert!(e.run(plan.id, false, &control(plan.id)).is_err());
    assert!(e.approve(plan.id, &plan.digest, 60).is_err());
}

#[test]
fn sfc_does_not_claim_repair_integrity_from_exit_zero() {
    let (mut e, disk, backend, plan) = approved(vec![OperationKind::SfcRepair]);
    let result = e.run(plan.id, false, &control(plan.id)).unwrap();
    assert_eq!(
        result.steps[0].evidence,
        Some(Evidence::DiagnosticCompleted)
    );
    assert_eq!(result.steps[1].state, StepState::NeedsReview);
    assert_eq!(
        result.steps[1].evidence,
        Some(Evidence::DiagnosticCompleted)
    );
    assert!(update_idle(disk.0.borrow().bytes.as_ref().unwrap(), &"a".repeat(64)).is_ok());
    assert_eq!(backend.0.borrow().verify, [OperationKind::SfcRepair]);
}

#[test]
fn defender_completion_is_not_a_threat_absence_assertion() {
    let (mut e, _, backend, plan) = approved(vec![OperationKind::DefenderQuickScan]);
    backend.0.borrow_mut().evidence = Evidence::DefenderScanCompleted;
    let result = e.run(plan.id, false, &control(plan.id)).unwrap();
    assert_eq!(
        result.steps[0].evidence,
        Some(Evidence::DefenderScanCompleted)
    );
    assert_eq!(
        backend.0.borrow().verify,
        [OperationKind::DefenderQuickScan]
    );
}

#[test]
fn timeout_and_cancel_do_not_skip_supervision_or_launch_next_step() {
    for reason in [StopReason::Timeout, StopReason::CancelRequested] {
        let (mut e, disk, backend, plan) = approved(vec![
            OperationKind::DismCheckHealth,
            OperationKind::SfcVerify,
        ]);
        backend.0.borrow_mut().stop = Some(reason);
        let result = e.run(plan.id, false, &control(plan.id)).unwrap();
        assert_eq!(result.steps[0].stop_reason, Some(reason));
        assert_eq!(result.steps[1].state, StepState::Cancelled);
        assert_eq!(backend.0.borrow().supervised, 1);
        let states: Vec<State> = disk
            .0
            .borrow()
            .snapshots
            .iter()
            .map(|b| serde_json::from_slice(b).unwrap())
            .collect();
        assert!(states.iter().any(|s| s
            .plans
            .first()
            .is_some_and(|p| p.steps[0].state == StepState::Monitoring)));
    }
}

#[test]
fn cancellation_before_start_never_consumes_or_launches() {
    let (mut e, _, backend, plan) = approved(vec![OperationKind::DismRestoreHealth]);
    let control = control(plan.id);
    control.cancel.store(true, Ordering::SeqCst);
    assert!(e.run(plan.id, false, &control).is_err());
    assert!(!e.record(plan.id).unwrap().consumed);
    assert!(backend.0.borrow().execute.is_empty());
}

#[test]
fn reboot_recovery_requires_new_boot_and_never_replays_mutation() {
    let (mut e, disk, backend, plan) = approved(vec![OperationKind::DefenderQuickScan]);
    backend.0.borrow_mut().exit = 3010;
    let result = e.run(plan.id, false, &control(plan.id)).unwrap();
    assert_eq!(result.steps[0].state, StepState::RebootRequired);
    drop(e);
    let mut e = Engine::open(disk, backend.clone()).unwrap();
    assert!(e.run(plan.id, true, &control(plan.id)).is_err());
    {
        let mut w = backend.0.borrow_mut();
        w.facts.boot_time = 7200;
        w.evidence = Evidence::DefenderScanCompleted;
        w.time = 8000;
        w.facts.captured_at = 8000;
    }
    let result = e.run(plan.id, true, &control(plan.id)).unwrap();
    assert_eq!(result.steps[0].state, StepState::Succeeded);
    assert_eq!(backend.0.borrow().execute.len(), 1);
    assert_eq!(backend.0.borrow().verify.len(), 1);
}

#[test]
fn busy_interrupted_child_defers_verification_and_every_new_plan() {
    let (mut e, disk, backend, plan) = approved(vec![OperationKind::DefenderQuickScan]);
    backend.0.borrow_mut().stop = Some(StopReason::Timeout);
    e.run(plan.id, false, &control(plan.id)).unwrap();
    let new = e
        .plan(PlanRequest {
            operations: vec![OperationKind::DismCheckHealth],
            valid_for_seconds: 60,
        })
        .unwrap();
    e.approve(new.id, &new.digest, 60).unwrap();
    assert!(e.run(new.id, false, &control(new.id)).is_err());
    drop(e);
    backend.0.borrow_mut().facts.busy = true;
    let mut e = Engine::open(disk, backend.clone()).unwrap();
    assert!(e.run(plan.id, true, &control(plan.id)).is_err());
    assert!(backend.0.borrow().verify.is_empty());
}

#[test]
fn fault_at_every_execution_publication_boundary_never_replays_side_effects() {
    // Both allowed outcomes of a failed atomic publication: old durable bytes,
    // or new durable bytes with an error returned after publication. Includes
    // consume, intent, spawn, verify intent/spawn, success, monitor, and reboot.
    for scenario in 0..6 {
        let setup = || {
            let (e, disk, backend, plan) = approved(if scenario == 5 {
                vec![OperationKind::DismCheckHealth, OperationKind::SfcVerify]
            } else {
                vec![OperationKind::DefenderQuickScan]
            });
            {
                let mut w = backend.0.borrow_mut();
                w.evidence = if scenario == 5 {
                    Evidence::ComponentStoreHealthy
                } else {
                    Evidence::DefenderScanCompleted
                };
                match scenario {
                    1 | 5 => w.stop = Some(StopReason::Timeout),
                    2 => w.exit = 3010,
                    3 => w.exit = 5,
                    4 => w.error = true,
                    _ => {}
                }
            }
            disk.0.borrow_mut().writes = 0;
            (e, disk, backend, plan)
        };
        let (mut clean, disk, _, plan) = setup();
        let _ = clean.run(plan.id, false, &control(plan.id));
        let boundaries = disk.0.borrow().writes;
        assert!(boundaries >= 3);
        for after in [false, true] {
            for boundary in 1..=boundaries {
                let (mut e, disk, backend, plan) = setup();
                {
                    let mut d = disk.0.borrow_mut();
                    d.fail = Some(boundary);
                    d.after_publication = after;
                }
                assert!(
                    e.run(plan.id, false, &control(plan.id)).is_err(),
                    "scenario={scenario} boundary={boundary} after={after}"
                );
                let launches = backend.0.borrow().execute.len();
                assert!(backend.0.borrow().supervised >= launches);
                drop(e);
                disk.0.borrow_mut().fail = None;
                {
                    let mut w = backend.0.borrow_mut();
                    w.error = false;
                    w.stop = None;
                    w.facts.boot_time = 2;
                }
                let mut recovered = Engine::open(disk, backend.clone()).unwrap();
                let consumed = recovered.record(plan.id).unwrap().consumed;
                if consumed {
                    assert!(recovered.run(plan.id, false, &control(plan.id)).is_err());
                    let _ = recovered.run(plan.id, true, &control(plan.id));
                } else {
                    assert_eq!(launches, 0, "Mutation without durable consumed token");
                }
                assert_eq!(
                    backend.0.borrow().execute.len(),
                    launches,
                    "Recovery replayed mutation"
                );
            }
        }
    }
}

#[test]
fn policy_plan_and_approval_faults_never_authorize_unpublished_work() {
    for boundary in 1..=3 {
        for after in [false, true] {
            let (mut e, disk, backend) = open();
            {
                let mut d = disk.0.borrow_mut();
                d.fail = Some(boundary);
                d.after_publication = after;
            }
            let result = (|| -> Result<()> {
                e.set_policy(OwnerPolicy::default())?;
                let p = e.plan(PlanRequest {
                    operations: vec![OperationKind::DismCheckHealth],
                    valid_for_seconds: 60,
                })?;
                e.approve(p.id, &p.digest, 60)?;
                Ok(())
            })();
            assert!(result.is_err());
            assert!(
                e.plan(PlanRequest {
                    operations: vec![OperationKind::DismCheckHealth],
                    valid_for_seconds: 60
                })
                .is_err(),
                "Failed store was not poisoned"
            );
            assert!(backend.0.borrow().execute.is_empty());
            disk.0.borrow_mut().fail = None;
            Engine::open(disk, backend).unwrap();
        }
    }
}

#[test]
fn recover_from_every_successful_snapshot_is_verify_only() {
    let (mut e, disk, backend, plan) = approved(vec![OperationKind::DismRestoreHealth]);
    e.run(plan.id, false, &control(plan.id)).unwrap();
    let snapshots = disk.0.borrow().snapshots.clone();
    let original_launches = backend.0.borrow().execute.len();
    for snapshot in snapshots {
        disk.0.borrow_mut().bytes = Some(snapshot);
        let mut e = Engine::open(disk.clone(), backend.clone()).unwrap();
        let _ = e.run(plan.id, true, &control(plan.id));
        assert_eq!(backend.0.borrow().execute.len(), original_launches);
    }
}

#[test]
fn bounded_task_wait_and_drop_signal_do_not_fabricate_completion() {
    let (tx, rx) = mpsc::sync_channel(1);
    let c = control(Uuid::new_v4());
    let flag = c.cancel.clone();
    let task = Task {
        cancel: c.cancel,
        progress: c.progress,
        result: rx,
    };
    assert!(task.wait(Duration::ZERO).unwrap().is_none());
    assert!(!flag.load(Ordering::SeqCst));
    drop(task);
    assert!(flag.load(Ordering::SeqCst));
    drop(tx);
}

#[test]
fn dependencies_never_run_after_failed_diagnostic() {
    let (mut e, _, backend, plan) = approved(vec![OperationKind::DismRestoreHealth]);
    backend.0.borrow_mut().exit = 5;
    let record = e.run(plan.id, false, &control(plan.id)).unwrap();
    assert_eq!(record.steps[0].state, StepState::Failed);
    assert_eq!(record.steps[1].state, StepState::Cancelled);
    assert_eq!(backend.0.borrow().execute, [OperationKind::DismScanHealth]);
    backend.0.borrow_mut().exit = 0;
    let record = e.run(plan.id, true, &control(plan.id)).unwrap();
    assert_eq!(record.steps[0].state, StepState::Succeeded);
    assert_eq!(record.steps[1].state, StepState::Cancelled);
    assert_eq!(backend.0.borrow().execute, [OperationKind::DismScanHealth]);
}

#[test]
fn window_wraps_midnight_and_exception_expires_exclusively() {
    for (time, allowed) in [
        (7200, false),
        (23 * 3600, true),
        (30 * 60, true),
        (3600, false),
    ] {
        let (mut e, _, backend) = open();
        {
            let mut w = backend.0.borrow_mut();
            w.time = time;
            w.facts.captured_at = time;
        }
        let policy = OwnerPolicy {
            window: MaintenanceWindow {
                start_minute_utc: 23 * 60,
                end_minute_utc: 60,
            },
            ..OwnerPolicy::default()
        };
        e.set_policy(policy).unwrap();
        let p = e
            .plan(PlanRequest {
                operations: vec![OperationKind::DismCheckHealth],
                valid_for_seconds: 600,
            })
            .unwrap();
        e.approve(p.id, &p.digest, 600).unwrap();
        assert_eq!(e.run(p.id, false, &control(p.id)).is_ok(), allowed);
    }
    for time in [7299, 7300] {
        let (mut e, _, backend) = open();
        let mut policy = OwnerPolicy::default();
        policy.exceptions.push(PolicyException {
            operation: OperationKind::DismCheckHealth,
            scope: ExceptionScope::ActiveUse,
            expires_at: 7300,
        });
        e.set_policy(policy).unwrap();
        let p = e
            .plan(PlanRequest {
                operations: vec![OperationKind::DismCheckHealth],
                valid_for_seconds: 600,
            })
            .unwrap();
        e.approve(p.id, &p.digest, 600).unwrap();
        {
            let mut w = backend.0.borrow_mut();
            w.time = time;
            w.facts.captured_at = time;
            w.facts.idle_seconds = None;
        }
        assert_eq!(e.run(p.id, false, &control(p.id)).is_ok(), time == 7299);
    }
}

#[test]
fn repair_failed_verification_is_review_not_success_and_can_be_reverified() {
    let (mut e, _, backend, plan) = approved(vec![OperationKind::DismRestoreHealth]);
    backend.0.borrow_mut().evidence = Evidence::ComponentStoreRepairable;
    let record = e.run(plan.id, false, &control(plan.id)).unwrap();
    assert_eq!(record.steps[1].state, StepState::NeedsReview);
    let launches = backend.0.borrow().execute.len();
    backend.0.borrow_mut().evidence = Evidence::ComponentStoreHealthy;
    let record = e.run(plan.id, true, &control(plan.id)).unwrap();
    assert_eq!(record.steps[1].state, StepState::Succeeded);
    assert_eq!(backend.0.borrow().execute.len(), launches);
}

#[test]
fn completed_state_cannot_be_injected_for_incompatible_evidence() {
    let (mut e, disk, backend, plan) = approved(vec![OperationKind::DismCheckHealth]);
    e.run(plan.id, false, &control(plan.id)).unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(disk.0.borrow().bytes.as_ref().unwrap()).unwrap();
    value["plans"][0]["steps"][0]["evidence"] = "defender_scan_completed".into();
    disk.0.borrow_mut().bytes = Some(serde_json::to_vec(&value).unwrap());
    assert!(Engine::open(disk, backend).is_err());
}

#[test]
fn defender_network_cost_is_fresh_and_unknown_is_not_unmetered() {
    for cost in [None, Some(false)] {
        let (mut e, _, backend, plan) = approved(vec![OperationKind::DefenderQuickScan]);
        backend.0.borrow_mut().facts.unmetered = cost;
        assert!(e.run(plan.id, false, &control(plan.id)).is_err());
        assert!(backend.0.borrow().execute.is_empty());

        let (mut e, _, backend, _) = approved(vec![OperationKind::DismCheckHealth]);
        let mut policy = e.state.policy.clone();
        policy.exceptions.push(PolicyException {
            operation: OperationKind::DefenderQuickScan,
            scope: ExceptionScope::MeteredNetwork,
            expires_at: 7300,
        });
        e.set_policy(policy).unwrap();
        let p = e
            .plan(PlanRequest {
                operations: vec![OperationKind::DefenderQuickScan],
                valid_for_seconds: 600,
            })
            .unwrap();
        e.approve(p.id, &p.digest, 600).unwrap();
        {
            let mut w = backend.0.borrow_mut();
            w.facts.unmetered = cost;
            w.evidence = Evidence::DefenderScanCompleted;
        }
        let r = e.run(p.id, false, &control(p.id)).unwrap();
        assert_eq!(r.steps[0].state, StepState::Succeeded);
    }
    assert!(!OperationKind::DismRestoreHealth.spec().network_access);
    assert!(OperationKind::DefenderQuickScan.spec().network_access);
}

#[test]
fn updater_interlock_blocks_uncertain_records_until_independent_verification() {
    let (mut e, disk, backend, plan) = approved(vec![OperationKind::DefenderQuickScan]);
    let machine = "a".repeat(64);
    assert!(update_idle(disk.0.borrow().bytes.as_ref().unwrap(), &machine).is_ok());
    backend.0.borrow_mut().stop = Some(StopReason::Timeout);
    e.run(plan.id, false, &control(plan.id)).unwrap();
    assert!(update_idle(disk.0.borrow().bytes.as_ref().unwrap(), &machine).is_err());
    {
        let mut w = backend.0.borrow_mut();
        w.stop = None;
        w.evidence = Evidence::DefenderScanCompleted;
    }
    e.run(plan.id, true, &control(plan.id)).unwrap();
    assert!(update_idle(disk.0.borrow().bytes.as_ref().unwrap(), &machine).is_ok());
    assert!(update_idle(disk.0.borrow().bytes.as_ref().unwrap(), &"b".repeat(64)).is_err());
    assert!(update_idle(b"{}", &machine).is_err());
    assert_eq!(backend.0.borrow().execute.len(), 1);
}

#[test]
fn failed_acknowledgement_and_verifier_keep_interlock_closed() {
    for failure in 0..3 {
        let (mut e, disk, backend, plan) = approved(vec![OperationKind::DefenderQuickScan]);
        {
            let mut w = backend.0.borrow_mut();
            w.error = failure == 0;
            w.verify_error = failure == 1;
            w.evidence = if failure == 2 {
                Evidence::Inconclusive
            } else {
                Evidence::DefenderScanCompleted
            };
        }
        let record = e.run(plan.id, false, &control(plan.id)).unwrap();
        assert!(record.steps[0].state.uncertain());
        assert!(update_idle(disk.0.borrow().bytes.as_ref().unwrap(), &"a".repeat(64)).is_err());
        let next = e
            .plan(PlanRequest {
                operations: vec![OperationKind::DismCheckHealth],
                valid_for_seconds: 60,
            })
            .unwrap();
        e.approve(next.id, &next.digest, 60).unwrap();
        assert!(e.run(next.id, false, &control(next.id)).is_err());
        drop(e);
        {
            let mut w = backend.0.borrow_mut();
            w.error = false;
            w.verify_error = false;
            w.evidence = Evidence::DefenderScanCompleted;
            w.time = 8000;
            w.facts.captured_at = 8000;
        }
        let mut e = Engine::open(disk.clone(), backend.clone()).unwrap();
        assert_eq!(
            e.run(plan.id, true, &control(plan.id)).unwrap().steps[0].state,
            StepState::Succeeded
        );
        assert_eq!(
            backend.0.borrow().execute,
            [OperationKind::DefenderQuickScan]
        );
        assert!(update_idle(disk.0.borrow().bytes.as_ref().unwrap(), &"a".repeat(64)).is_ok());
    }
}

#[test]
fn legacy_inconclusive_review_cannot_bypass_interlock() {
    for evidence in [None, Some(Evidence::Inconclusive)] {
        let (mut e, disk, backend, plan) = approved(vec![OperationKind::DefenderQuickScan]);
        backend.0.borrow_mut().error = true;
        e.run(plan.id, false, &control(plan.id)).unwrap();
        // Earlier schema-1 implementations converted launch/verification errors
        // into NeedsReview, indistinguishable from successful reviewable work.
        let mut state: State =
            serde_json::from_slice(disk.0.borrow().bytes.as_ref().unwrap()).unwrap();
        state.plans[0].steps[0].state = StepState::NeedsReview;
        state.plans[0].steps[0].evidence = evidence;
        disk.0.borrow_mut().bytes = Some(serde_json::to_vec(&state).unwrap());
        assert!(update_idle(disk.0.borrow().bytes.as_ref().unwrap(), &"a".repeat(64)).is_err());
        drop(e);
        let mut e = Engine::open(disk.clone(), backend.clone()).unwrap();
        backend.0.borrow_mut().error = false;
        backend.0.borrow_mut().evidence = Evidence::DefenderScanCompleted;
        e.run(plan.id, true, &control(plan.id)).unwrap();
        assert!(update_idle(disk.0.borrow().bytes.as_ref().unwrap(), &"a".repeat(64)).is_ok());
        assert_eq!(backend.0.borrow().execute.len(), 1);
    }
}

#[test]
fn corrupted_terminal_states_cannot_hide_uncertain_execution() {
    let (mut e, disk, backend, plan) = approved(vec![OperationKind::DefenderQuickScan]);
    backend.0.borrow_mut().error = true;
    e.run(plan.id, false, &control(plan.id)).unwrap();
    let original = disk.0.borrow().bytes.clone().unwrap();
    for cancelled in [false, true] {
        let mut state: State = serde_json::from_slice(&original).unwrap();
        let step = &mut state.plans[0].steps[0];
        if cancelled {
            step.state = StepState::Cancelled;
        } else {
            step.state = StepState::NeedsReview;
            step.evidence = Some(Evidence::DiagnosticCompleted);
        }
        let bytes = serde_json::to_vec(&state).unwrap();
        assert!(update_idle(&bytes, &"a".repeat(64)).is_err());
        disk.0.borrow_mut().bytes = Some(bytes);
        assert!(Engine::open(disk.clone(), backend.clone()).is_err());
    }
}

#[test]
fn slow_pinning_cannot_extend_readiness_for_execution_or_verification() {
    for verification in [false, true] {
        let (mut e, disk, backend, plan) = approved(vec![OperationKind::DefenderQuickScan]);
        if verification {
            backend.0.borrow_mut().verify_delay = 31;
        } else {
            backend.0.borrow_mut().launch_delay = 31;
        }
        let record = e.run(plan.id, false, &control(plan.id)).unwrap();
        assert!(record.steps[0].state.uncertain());
        assert!(backend.0.borrow().verify.is_empty());
        assert_eq!(backend.0.borrow().execute.len(), usize::from(verification));
        assert!(update_idle(disk.0.borrow().bytes.as_ref().unwrap(), &"a".repeat(64)).is_err());
    }
}

#[test]
fn launch_permit_ends_at_maintenance_window_even_with_fresh_facts() {
    let (mut e, _, backend) = open();
    let mut policy = OwnerPolicy::default();
    policy.window.end_minute_utc = 121; // 02:01 exclusive
    e.set_policy(policy).unwrap();
    let plan = e
        .plan(PlanRequest {
            operations: vec![OperationKind::DismCheckHealth],
            valid_for_seconds: 600,
        })
        .unwrap();
    e.approve(plan.id, &plan.digest, 600).unwrap();
    {
        let mut w = backend.0.borrow_mut();
        w.time = 7259;
        w.facts.captured_at = 7259;
        w.launch_delay = 1;
    }
    let result = e.run(plan.id, false, &control(plan.id)).unwrap();
    assert_eq!(result.steps[0].state, StepState::Intent);
    assert!(backend.0.borrow().execute.is_empty());
}

#[cfg(not(windows))]
#[test]
fn non_windows_production_entrypoints_never_create_fake_state() {
    assert!(policy().is_err());
    assert!(list().is_err());
    assert!(start(Uuid::new_v4()).is_err());
    assert!(resume(Uuid::new_v4()).is_err());
}
