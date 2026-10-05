use super::core::{Engine, Event, Phase, Storage as _};
use super::*;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Disk {
    bytes: Option<Vec<u8>>,
    writes: usize,
    fail: Option<usize>,
    other_busy: bool,
}
#[derive(Clone, Default)]
struct Store(Arc<Mutex<Disk>>);
impl core::Storage for Store {
    fn load(&mut self) -> Result<Option<Vec<u8>>> {
        Ok(self.0.lock().unwrap().bytes.clone())
    }
    fn save(&mut self, bytes: &[u8]) -> Result<()> {
        let mut d = self.0.lock().unwrap();
        d.writes += 1;
        ensure!(
            d.fail != Some(d.writes),
            "Injected atomic publication failure"
        );
        d.bytes = Some(bytes.to_vec());
        Ok(())
    }
    fn other_operations_idle(&self) -> Result<()> {
        ensure!(!self.0.lock().unwrap().other_busy, "Other operation busy");
        Ok(())
    }
}
struct Os {
    time: u64,
    binding: Binding,
    updates: Vec<Update>,
    downloads: usize,
    installs: usize,
    verifies: usize,
    metadata_changed: bool,
    installed: Vec<UpdateIdentity>,
    reboot: bool,
    expire_after_download: bool,
    timeout: bool,
    alive: bool,
    boot: Uuid,
    lose_ack: bool,
    skip_completion: bool,
    cancel: Option<Arc<AtomicBool>>,
    cancel_after_download: bool,
    discoveries: usize,
    catalog_time: Option<u64>,
    evidence_time: Option<u64>,
}
#[derive(Clone)]
struct Backend(Arc<Mutex<Os>>);
impl Backend {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Os {
            time: 1000,
            binding: Binding {
                machine: "a".repeat(64),
                original_user: "b".repeat(64),
            },
            updates: vec![update()],
            downloads: 0,
            installs: 0,
            verifies: 0,
            metadata_changed: false,
            installed: vec![],
            reboot: false,
            expire_after_download: false,
            timeout: false,
            alive: false,
            boot: Uuid::from_u128(100),
            lose_ack: false,
            skip_completion: false,
            cancel: None,
            cancel_after_download: false,
            discoveries: 0,
            catalog_time: None,
            evidence_time: None,
        })))
    }
}
impl core::Backend for Backend {
    fn set_cancel(&mut self, cancel: Arc<AtomicBool>) {
        self.0.lock().unwrap().cancel = Some(cancel);
    }
    fn binding(&self) -> Result<Binding> {
        Ok(self.0.lock().unwrap().binding.clone())
    }
    fn time(&self) -> Result<u64> {
        Ok(self.0.lock().unwrap().time)
    }
    fn discover(&mut self) -> Result<Catalog> {
        let mut o = self.0.lock().unwrap();
        o.discoveries += 1;
        Ok(Catalog {
            binding: o.binding.clone(),
            searched_at: o.catalog_time.unwrap_or(o.time),
            source: SOURCE.into(),
            updates: o.updates.clone(),
        })
    }
    fn execute(
        &mut self,
        phase: Phase,
        _: &Plan,
        _: &Approval,
        notify: &mut dyn FnMut(Event) -> Result<()>,
    ) -> Result<()> {
        notify(Event::Spawned(ProcessIdentity {
            pid: 42,
            creation_time: 999,
            boot_id: Some(self.0.lock().unwrap().boot),
        }))?;
        let mut o = self.0.lock().unwrap();
        ensure!(!o.metadata_changed, "Metadata changed; no action");
        match phase {
            Phase::Download => {
                o.downloads += 1;
                if o.expire_after_download {
                    o.time += 3600;
                }
                if o.cancel_after_download {
                    o.cancel.as_ref().unwrap().store(true, Ordering::SeqCst);
                }
            }
            Phase::Install => o.installs += 1,
        }
        if o.timeout {
            notify(Event::Timeout)?;
            bail!("Timed out, supervised until exit");
        }
        ensure!(
            !o.lose_ack,
            "Client exited without WUA phase acknowledgement"
        );
        if o.skip_completion {
            return Ok(());
        }
        drop(o);
        notify(Event::PhaseFinished)
    }
    fn verify(&mut self, _: &Plan, process: Option<&ProcessIdentity>) -> Result<Verification> {
        let mut o = self.0.lock().unwrap();
        ensure!(!o.alive, "Recorded worker still alive");
        if let Some(process) = process {
            core::recovery_after_boot(process, o.boot)?;
        }
        o.verifies += 1;
        Ok(Verification {
            installed: o.installed.clone(),
            reboot_pending: o.reboot,
            checked_at: o.evidence_time.unwrap_or(o.time),
        })
    }
}
fn update() -> Update {
    Update {
        identity: UpdateIdentity {
            update_id: Uuid::from_u128(7),
            revision: 4,
        },
        title: "Windows security update".into(),
        description: "Reviewed description".into(),
        kb_articles: vec!["1234567".into()],
        categories: vec![
            Uuid::from_u128(0x0fa1201d_4330_4fa8_8ae9_b877473b6441),
            Uuid::from_u128(0x6964aab4_c5b5_43bd_a17d_ffb4346a8e1d),
        ],
        max_download_bytes: 4096,
        last_changed: "639264960000000000".into(),
        severity: "Critical".into(),
        handler: "trusted WUA handler".into(),
        reboot_behavior: 2,
        eula: "Reviewed license".into(),
        bundled: vec![],
    }
}
fn consent() -> Consent {
    Consent {
        owner_opt_in: true,
        accept_windows_update_source: true,
        accept_reviewed_eulas: true,
        acknowledge_no_automatic_rollback: true,
    }
}
fn prepared() -> (Engine<Store, Backend>, Store, Backend, Plan) {
    let store = Store::default();
    let os = Backend::new();
    let mut e = Engine::open(store.clone(), os.clone()).unwrap();
    let p = e
        .plan(PlanRequest {
            selected: vec![update().identity],
            valid_for_seconds: 86400,
        })
        .unwrap();
    e.approve(p.id, &p.digest, consent(), 900).unwrap();
    (e, store, os, p)
}

#[test]
fn exact_selection_digest_binding_and_all_consents_required() {
    let (mut e, _, os, p) = prepared();
    assert!(e.run(p.id, "wrong").is_err());
    for index in 0..4 {
        let mut c = consent();
        match index {
            0 => c.owner_opt_in = false,
            1 => c.accept_windows_update_source = false,
            2 => c.accept_reviewed_eulas = false,
            _ => c.acknowledge_no_automatic_rollback = false,
        }
        assert!(e.approve(p.id, &p.digest, c, 900).is_err());
    }
    os.0.lock().unwrap().binding.original_user = "c".repeat(64);
    assert!(e.run(p.id, &p.digest).is_err());
    assert!(e.approve(p.id, &p.digest, consent(), 900).is_err());
    assert_eq!(os.0.lock().unwrap().downloads, 0);
}

#[test]
fn stale_revision_duplicates_and_empty_selection_never_expand_to_all() {
    let (mut e, _, _, _) = prepared();
    for selected in [
        vec![],
        vec![update().identity, update().identity],
        vec![UpdateIdentity {
            revision: 5,
            ..update().identity
        }],
    ] {
        assert!(e
            .plan(PlanRequest {
                selected,
                valid_for_seconds: 60
            })
            .is_err());
    }
    assert_eq!(e.records().len(), 1);
}
#[test]
fn command_success_does_not_prove_installation_and_no_automatic_replay() {
    let (mut e, store, os, p) = prepared();
    let r = e.run(p.id, &p.digest).unwrap();
    assert_eq!(r.status, Status::NeedsReview);
    {
        let o = os.0.lock().unwrap();
        assert_eq!((o.downloads, o.installs), (1, 1));
    }
    assert!(e.run(p.id, &p.digest).is_err());
    let mut reopened = Engine::open(store, os.clone()).unwrap();
    assert_eq!(reopened.verify(p.id).unwrap().status, Status::NeedsReview);
    assert_eq!(os.0.lock().unwrap().installs, 1);
}
#[test]
fn independent_exact_revision_and_reboot_required_then_clear() {
    let (mut e, mut store, os, p) = prepared();
    {
        let mut o = os.0.lock().unwrap();
        o.installed = vec![update().identity];
        o.reboot = true;
    }
    assert_eq!(
        e.run(p.id, &p.digest).unwrap().status,
        Status::RebootRequired
    );
    assert!(core::idle(&store.load().unwrap().unwrap(), &p.binding.machine).is_err());
    os.0.lock().unwrap().reboot = false;
    assert_eq!(e.verify(p.id).unwrap().status, Status::Succeeded);
    core::idle(&store.load().unwrap().unwrap(), &p.binding.machine).unwrap();
    assert_eq!(os.0.lock().unwrap().installs, 1);
}

#[test]
fn wrong_or_duplicate_independent_installed_evidence_is_never_success() {
    for evidence in [
        vec![UpdateIdentity {
            revision: 5,
            ..update().identity
        }],
        vec![UpdateIdentity {
            update_id: Uuid::from_u128(9),
            ..update().identity
        }],
        vec![update().identity, update().identity],
    ] {
        let (mut e, _, os, p) = prepared();
        os.0.lock().unwrap().installed = evidence;
        let r = e.run(p.id, &p.digest).unwrap();
        assert_eq!(r.status, Status::NeedsReview);
        assert!(r.uncertain && r.verification.is_none());
        assert!(e.discover().is_err());
    }
}
#[test]
fn bundle_identity_must_also_verify_and_metadata_is_digest_bound() {
    let store = Store::default();
    let os = Backend::new();
    let mut child = update();
    child.identity.update_id = Uuid::from_u128(8);
    os.0.lock().unwrap().updates[0].bundled.push(child);
    let mut e = Engine::open(store, os.clone()).unwrap();
    let p = e
        .plan(PlanRequest {
            selected: vec![update().identity],
            valid_for_seconds: 60,
        })
        .unwrap();
    let mut changed = p.clone();
    changed.updates[0].bundled[0].eula.push('!');
    assert_ne!(changed.computed_digest().unwrap(), p.digest);
    e.approve(p.id, &p.digest, consent(), 60).unwrap();
    os.0.lock().unwrap().installed = vec![update().identity];
    assert_eq!(e.run(p.id, &p.digest).unwrap().status, Status::NeedsReview);
}
#[test]
fn metadata_change_timeout_or_expiry_stops_before_next_phase() {
    for scenario in 0..3 {
        let (mut e, store, os, p) = prepared();
        {
            let mut o = os.0.lock().unwrap();
            o.metadata_changed = scenario == 0;
            o.timeout = scenario == 1;
            o.expire_after_download = scenario == 2;
        }
        let r = e.run(p.id, &p.digest).unwrap();
        assert_eq!(r.status, Status::NeedsReview);
        assert!(r.uncertain);
        assert_eq!(os.0.lock().unwrap().installs, 0);
        let mut reopened = Engine::open(store, os.clone()).unwrap();
        reopened.verify(p.id).unwrap();
        assert_eq!(os.0.lock().unwrap().installs, 0);
        assert!(reopened.run(p.id, &p.digest).is_err());
    }
}
#[test]
fn expiry_and_clock_rollback_before_consume_cannot_execute() {
    for time in [999, 1900] {
        let (mut e, _, os, p) = prepared();
        os.0.lock().unwrap().time = time;
        assert!(e.run(p.id, &p.digest).is_err());
        assert_eq!(e.record(p.id).unwrap().status, Status::Planned);
        assert_eq!(os.0.lock().unwrap().downloads, 0);
    }
}
#[test]
fn every_publication_fault_never_replays_and_spawn_fault_prevents_submission() {
    // Success path also durably acknowledges each fully supervised WUA phase.
    // Every publication (including loss of the phase acknowledgement) is injectable.
    for boundary in 1..=10 {
        let (mut e, store, os, p) = prepared();
        os.0.lock().unwrap().installed = vec![update().identity];
        {
            let mut d = store.0.lock().unwrap();
            d.fail = Some(d.writes + boundary);
        }
        assert!(
            e.run(p.id, &p.digest).is_err(),
            "fault {boundary} not observed"
        );
        let before = os.0.lock().unwrap().installs;
        store.0.lock().unwrap().fail = None;
        let mut reopened = Engine::open(store.clone(), os.clone()).unwrap();
        if boundary == 1 {
            assert_eq!(os.0.lock().unwrap().downloads, 0);
            assert_eq!(reopened.record(p.id).unwrap().status, Status::Planned);
        } else {
            assert!(reopened.run(p.id, &p.digest).is_err());
            reopened.verify(p.id).unwrap();
            assert_eq!(os.0.lock().unwrap().installs, before);
        }
        if boundary <= 3 {
            assert_eq!(os.0.lock().unwrap().downloads, 0);
        }
    }
}

#[test]
fn dead_client_is_not_service_completion_and_pid_reuse_does_not_clear_intent() {
    let (mut e, mut store, os, p) = prepared();
    os.0.lock().unwrap().lose_ack = true;
    let r = e.run(p.id, &p.digest).unwrap();
    assert_eq!(r.status, Status::NeedsReview);
    assert!(r.process.is_some());
    drop(e);
    let mut e = Engine::open(store.clone(), os.clone()).unwrap();
    os.0.lock().unwrap().installed = vec![update().identity];
    // Even independently visible installed bits do not prove abandoned service
    // work has ended in this boot. No PID being alive cannot override this.
    assert_eq!(e.verify(p.id).unwrap().status, Status::NeedsReview);
    assert_eq!(os.0.lock().unwrap().verifies, 0);
    assert!(core::idle(&store.load().unwrap().unwrap(), &p.binding.machine).is_err());
    os.0.lock().unwrap().boot = Uuid::from_u128(101);
    let resolved = e.verify(p.id).unwrap();
    assert_eq!(resolved.status, Status::Succeeded);
    assert!(resolved.uncertain && resolved.process.is_none());
    assert_eq!(os.0.lock().unwrap().installs, 0); // verification never replayed installation
    core::idle(&store.load().unwrap().unwrap(), &p.binding.machine).unwrap();
    for boot_id in [None, Some(Uuid::nil()), Some(Uuid::from_u128(101))] {
        assert!(core::recovery_after_boot(
            &ProcessIdentity {
                pid: 42,
                creation_time: 999,
                boot_id
            },
            Uuid::from_u128(101)
        )
        .is_err());
    }
}

#[test]
fn cancellation_never_submits_a_later_phase_or_reuses_an_unpublished_approval() {
    let (mut e, _, os, p) = prepared();
    os.0.lock().unwrap().cancel_after_download = true;
    let r = e.run(p.id, &p.digest).unwrap();
    assert_eq!(r.status, Status::NeedsReview);
    assert_eq!(
        (os.0.lock().unwrap().downloads, r.process.is_some()),
        (1, false)
    );
    assert_eq!(os.0.lock().unwrap().installs, 0);
    assert!(e.verify(p.id).is_err()); // fresh request required after cancellation
    let (mut e, store, os, p) = prepared();
    let writes = store.0.lock().unwrap().writes;
    store.0.lock().unwrap().fail = Some(writes + 1);
    assert!(e.approve(p.id, &p.digest, consent(), 900).is_err());
    let searches = os.0.lock().unwrap().discoveries;
    assert!(e.discover().is_err());
    assert!(e.run(p.id, &p.digest).is_err());
    assert_eq!(os.0.lock().unwrap().discoveries, searches);
    assert_eq!(os.0.lock().unwrap().downloads, 0);
}

#[test]
fn task_poll_timeout_does_not_cancel_but_drop_and_explicit_cancel_do() {
    let (_send, result) = mpsc::channel::<Result<()>>();
    let cancel = Arc::new(AtomicBool::new(false));
    let task = Task {
        result,
        cancel: cancel.clone(),
    };
    assert!(task.wait(Duration::ZERO).unwrap().is_none());
    assert!(!cancel.load(Ordering::SeqCst));
    task.request_cancel();
    assert!(cancel.load(Ordering::SeqCst));
    cancel.store(false, Ordering::SeqCst);
    drop(task);
    assert!(cancel.load(Ordering::SeqCst));
}

#[test]
fn backend_success_without_phase_receipt_never_authorizes_installation() {
    let (mut e, _, os, p) = prepared();
    os.0.lock().unwrap().skip_completion = true;
    assert_eq!(e.run(p.id, &p.digest).unwrap().status, Status::NeedsReview);
    assert_eq!(os.0.lock().unwrap().installs, 0);
    assert!(e.record(p.id).unwrap().process.is_some());
}

#[test]
fn stale_catalog_and_installed_evidence_are_not_current_observations() {
    let (mut e, _, os, p) = prepared();
    os.0.lock().unwrap().catalog_time = Some(999);
    assert!(e.discover().is_err());
    {
        let mut o = os.0.lock().unwrap();
        o.time = 1100;
        o.evidence_time = Some(1000); // after plan creation, before this verification
        o.installed = vec![update().identity];
    }
    assert_eq!(e.run(p.id, &p.digest).unwrap().status, Status::NeedsReview);
    os.0.lock().unwrap().evidence_time = None;
    assert_eq!(e.verify(p.id).unwrap().status, Status::Succeeded);
}

#[test]
fn acknowledgement_is_typed_not_last_duplicate_key_wins() {
    script::acknowledged(br#"{"acknowledged":true}"#).unwrap();
    for data in [
        br#"{"acknowledged":false,"acknowledged":true}"#.as_slice(),
        br#"{"acknowledged":1}"#,
        br#"{"acknowledged":true,"extra":1}"#,
        b"null",
    ] {
        assert!(script::acknowledged(data).is_err());
    }
}

#[test]
fn unclassified_or_unsupported_bundle_content_is_rejected_before_plan_publication() {
    for scenario in 0..5 {
        let store = Store::default();
        let os = Backend::new();
        let mut child = update();
        child.identity.update_id = Uuid::from_u128(8);
        match scenario {
            0 => child.categories.clear(),
            1 => child
                .categories
                .push(Uuid::from_u128(0x3689bdc8_b205_4af4_8d4a_a63924c5e9d5)),
            2 => child.title = "Optional Preview".into(),
            3 => child.last_changed = "not ticks".into(),
            _ => child.kb_articles = vec![String::new()],
        }
        os.0.lock().unwrap().updates[0].bundled.push(child);
        let mut e = Engine::open(store.clone(), os).unwrap();
        assert!(e
            .plan(PlanRequest {
                selected: vec![update().identity],
                valid_for_seconds: 60
            })
            .is_err());
        assert_eq!(store.0.lock().unwrap().writes, 0);
    }
}
#[test]
fn alive_worker_and_other_operations_interlocks_fail_closed() {
    let (mut e, store, os, p) = prepared();
    store.0.lock().unwrap().other_busy = true;
    assert!(e.run(p.id, &p.digest).is_err());
    assert_eq!(os.0.lock().unwrap().downloads, 0);
    store.0.lock().unwrap().other_busy = false;
    e.run(p.id, &p.digest).unwrap();
    os.0.lock().unwrap().alive = true;
    assert_eq!(e.verify(p.id).unwrap().status, Status::NeedsReview);
    assert!(e.run(p.id, &p.digest).is_err());
}
#[test]
fn tampered_state_unknown_fields_wrong_machine_and_false_success_rejected() {
    let (_, store, os, p) = prepared();
    let original = store.0.lock().unwrap().bytes.clone().unwrap();
    for scenario in 0..5 {
        let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
        match scenario {
            0 => value["records"][0]["plan"]["updates"][0]["title"] = "tampered".into(),
            1 => value["command"] = "winget upgrade --all".into(),
            2 => value["records"][0]["status"] = "succeeded".into(),
            3 => value["binding_machine"] = "c".repeat(64).into(),
            _ => value["records"][0]["plan"]["updates"][0]["installer_path"] = "evil.exe".into(),
        }
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(core::idle(&bytes, &p.binding.machine).is_err());
        store.0.lock().unwrap().bytes = Some(bytes);
        assert!(Engine::open(store.clone(), os.clone()).is_err());
    }
}
#[test]
fn compiled_script_uses_only_encoded_data_and_fixed_actions() {
    let (_, _, _, mut p) = prepared();
    p.updates[0].title = "'; Start-Process evil; #".into();
    let a = Approval {
        digest: p.digest.clone(),
        approved_at: 1,
        expires_at: 2,
        consent: consent(),
    };
    for action in [script::Action::Download, script::Action::Install] {
        let s = script::build(action, Some(&p), Some(&a)).unwrap();
        assert!(!s.contains(&p.updates[0].title));
        assert!(!s.contains("switch -CaseSensitive ($action)"));
        assert!(s.contains("function Gate(") && s.contains("$installer.Install()"));
    }
    assert!(script::build(script::Action::Install, None, Some(&a)).is_err());
    assert!(script::build(script::Action::Verify, Some(&p), Some(&a)).is_err());
    assert!(!capabilities().selected_app_upgrades);
    #[cfg(not(windows))]
    {
        assert!(!capabilities().windows_quality_updates);
        assert!(discover().is_err());
        assert!(start(p.id, &p.digest).is_err());
    }
}
