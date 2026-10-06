// Included inside engine::tests. Putting back chosen settings: ownership rules, crash recovery and what is left untouched.
mod selective {
    use super::*;
    use crate::model::CheckStatus;

    const X: &str = "firewall.public.inbound";
    const Y: &str = "firewall.private.inbound";
    const Z: &str = "firewall.domain.inbound";
    const MI: &str = crate::vbs::MEMORY_INTEGRITY;
    const SP: &str = crate::vbs::STACK_PROTECTION;
    const POINTS: [&str; 6] = [
        "snapshot_create",
        "snapshot_write",
        "snapshot_sync",
        "snapshot_replace",
        "snapshot_directory",
        "snapshot_reopen",
    ];

    type Setup = (TempDir, Rc<RefCell<FakeState>>, Engine);

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn fault(point: &'static str, after: usize) {
        IO_FAULT.with(|f| *f.borrow_mut() = Some((point, after)));
    }

    fn disarm() {
        IO_FAULT.with(|f| *f.borrow_mut() = None);
    }

    fn controls() -> [&'static str; 4] {
        [X, Y, Z, DEFENDER]
    }

    fn four_controls() -> Setup {
        let (dir, state, e) = fixture(X, json!("Allow"));
        drop(e);
        for id in [Y, Z] {
            state.borrow_mut().values.insert(id.into(), json!("Allow"));
        }
        state.borrow_mut().values.insert(DEFENDER.into(), json!(true));
        let e = reopen(&dir, &state, &controls());
        (dir, state, e)
    }

    /// Older batch holds X and Y, the newer one holds Z.
    fn two_batches() -> Setup {
        let (dir, state, mut e) = four_controls();
        e.apply_selected(&ids(&[X, Y]), |_| {}).unwrap();
        e.apply_selected(&ids(&[Z]), |_| {}).unwrap();
        (dir, state, e)
    }

    fn value(state: &Rc<RefCell<FakeState>>, id: &str) -> Value {
        state.borrow().values[id].clone()
    }

    fn set(state: &Rc<RefCell<FakeState>>, id: &str, v: Value) {
        state.borrow_mut().values.insert(id.into(), v);
    }

    fn restores(state: &Rc<RefCell<FakeState>>, id: &str) -> usize {
        state
            .borrow()
            .writes
            .iter()
            .filter(|(w, v)| w == id && *v == json!("Allow"))
            .count()
    }

    type Shape = Vec<(bool, Vec<(String, State)>)>;

    fn shape(e: &Engine) -> Shape {
        e.load()
            .unwrap()
            .iter()
            .map(|t| {
                (
                    t.reverted,
                    t.entries.iter().map(|x| (x.id.clone(), x.state)).collect(),
                )
            })
            .collect()
    }

    fn journal_bytes(dir: &TempDir) -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|x| x.unwrap())
            .filter(|x| x.file_name().to_string_lossy().ends_with(".jsonl"))
            .map(|x| {
                (
                    x.file_name().to_string_lossy().into_owned(),
                    fs::read(x.path()).unwrap(),
                )
            })
            .collect();
        files.sort();
        files
    }

    /// What must hold after every operation: the journal loads, each setting has one owner, and nothing is left half published.
    fn invariants(dir: &TempDir, e: &Engine) {
        let txs = e.load().expect("the journal loads");
        let mut owners = HashSet::new();
        for tx in &txs {
            assert!(
                tx.entries.iter().all(|x| x.state != State::Restored) || tx.sealed || tx.reverting,
                "{} has a put back setting before it was complete",
                tx.name
            );
            if !tx.reverted {
                for x in tx.entries.iter().filter(|x| x.state != State::Restored) {
                    assert!(owners.insert(x.id.clone()), "{} has two owners", x.id);
                }
            }
        }
        assert!(
            fs::read_dir(dir.path())
                .unwrap()
                .all(|x| !x.unwrap().file_name().to_string_lossy().ends_with(".next")),
            "an unpublished snapshot was left behind"
        );
    }

    fn status(report: &Report, id: &str) -> CheckStatus {
        report
            .results
            .iter()
            .find(|r| r.id == id)
            .unwrap_or_else(|| panic!("no result for {id}"))
            .status
            .clone()
    }

    fn evidence_path(dir: &TempDir, stem: &str, bytes: &[u8]) -> PathBuf {
        dir.path()
            .join(format!("{stem}.evidence-{}", hex::encode(Sha256::digest(bytes))))
    }

    #[test]
    fn chosen_settings_in_different_batches_are_put_back_and_the_rest_left_alone() {
        for order in [[Y, Z], [Z, Y]] {
            let (dir, state, mut e) = two_batches();
            let report = e.revert_selected(&ids(&order), |_| {}).unwrap();
            invariants(&dir, &e);
            assert_eq!(status(&report, Z), CheckStatus::Restored);
            assert_eq!(status(&report, Y), CheckStatus::Restored);
            let seen: Vec<_> = report.results.iter().map(|r| r.id.as_str()).collect();
            assert_eq!(seen, [Z, Y], "newest first whatever the order chosen");
            assert_eq!(value(&state, Y), json!("Allow"));
            assert_eq!(value(&state, Z), json!("Allow"));
            assert_eq!(value(&state, X), json!("Block"));
            let history = e.history().unwrap();
            assert!(history[0].ends_with(" reverted"), "{history:?}");
            assert!(history[1].ends_with(" applied"), "{history:?}");
            assert_eq!(e.undoable_changes().unwrap(), 1);
            let audit = e.audit().unwrap();
            let undoable: Vec<_> = audit.results.iter().filter(|r| r.undoable).collect();
            assert_eq!(undoable.len(), 1);
            assert_eq!(undoable[0].id, X);
            assert_eq!(audit.undo_next, ids(&[X]));
        }
    }

    #[test]
    fn a_put_back_setting_can_be_fixed_again_and_undone_again() {
        let (dir, state, mut e) = two_batches();
        e.revert_selected(&ids(&[X]), |_| {}).unwrap();
        invariants(&dir, &e);
        let fixed = e.apply_selected(&ids(&[X]), |_| {}).unwrap();
        assert_eq!(status(&fixed, X), CheckStatus::Applied);
        invariants(&dir, &e);
        assert_eq!(value(&state, X), json!("Block"));
        let again = e.revert_selected(&ids(&[X]), |_| {}).unwrap();
        assert_eq!(status(&again, X), CheckStatus::Restored);
        assert_eq!(value(&state, X), json!("Allow"));
        invariants(&dir, &e);
        assert_eq!(restores(&state, X), 2);
        e.revert_all(|_| {}).unwrap();
        invariants(&dir, &e);
        assert!(e.load().unwrap().iter().all(|t| t.reverted));
        for id in [X, Y, Z] {
            assert_eq!(value(&state, id), json!("Allow"));
        }
    }

    #[test]
    fn putting_back_every_setting_of_a_batch_closes_it() {
        let (dir, state, mut e) = two_batches();
        let report = e.revert_selected(&ids(&[X, Y]), |_| {}).unwrap();
        assert!(report.results.iter().all(|r| r.status == CheckStatus::Restored));
        invariants(&dir, &e);
        let history = e.history().unwrap();
        assert!(history.iter().all(|h| !h.ends_with(" applied") || h == &history[0]));
        assert_eq!(e.load().unwrap()[0].reverted, true);
        assert_eq!(e.undoable_changes().unwrap(), 1);
        e.revert_selected(&ids(&[Z]), |_| {}).unwrap();
        assert_eq!(e.undoable_changes().unwrap(), 0);
        assert!(e.load().unwrap().iter().all(|t| t.reverted));
        assert!(e.history().unwrap().iter().all(|h| h.ends_with(" reverted")));
        assert_eq!(restores(&state, X), 1);
        assert!(e.audit().unwrap().undo_next.is_empty());
    }

    #[test]
    fn a_forged_journal_with_two_unreleased_owners_is_still_rejected() {
        for released in [None, Some("pending")] {
            let (dir, _state, mut e) = four_controls();
            for sequence in 1..=2 {
                let mut tx = prepare(&mut e, sequence, X, json!("Allow"));
                e.append(&mut tx, Record::Applied { id: X.into() }).unwrap();
                e.append(&mut tx, Record::Sealed).unwrap();
                if sequence == 1 && released.is_some() {
                    e.append(&mut tx, Record::RestorePending { id: X.into() })
                        .unwrap();
                }
            }
            let err = e.load().err().expect("two owners must not load");
            assert!(
                format!("{err:#}").contains("Duplicate active control owner"),
                "{err:#}"
            );
            drop(e);
            assert!(Engine::open(
                dir.path().into(),
                backend(&Rc::new(RefCell::new(FakeState::default())), &controls(), "machine-a")
            )
            .is_err());
        }
    }

    #[test]
    fn a_restored_owner_beside_a_later_owner_is_accepted() {
        let (_dir, _state, mut e) = four_controls();
        let mut first = prepare(&mut e, 1, X, json!("Allow"));
        e.append(&mut first, Record::Applied { id: X.into() }).unwrap();
        e.append(&mut first, Record::Sealed).unwrap();
        e.append(&mut first, Record::RestorePending { id: X.into() })
            .unwrap();
        e.append(&mut first, Record::Restored { id: X.into() }).unwrap();
        let mut second = prepare(&mut e, 2, X, json!("Allow"));
        e.append(&mut second, Record::Applied { id: X.into() }).unwrap();
        e.append(&mut second, Record::Sealed).unwrap();
        assert_eq!(e.load().unwrap().len(), 2);
    }

    #[test]
    fn a_setting_changed_since_is_left_as_it_is_and_the_journal_is_untouched() {
        let (dir, state, mut e) = two_batches();
        let before = journal_bytes(&dir);
        set(&state, Y, json!("NotConfigured"));
        let writes = state.borrow().writes.len();
        let report = e.revert_selected(&ids(&[X, Y]), |_| {}).unwrap();
        assert_eq!(status(&report, Y), CheckStatus::Conflict);
        assert_eq!(status(&report, X), CheckStatus::Restored);
        assert_eq!(state.borrow().writes.len(), writes + 1);
        assert_eq!(value(&state, Y), json!("NotConfigured"));
        invariants(&dir, &e);
        let only_y = journal_bytes(&dir);
        let again = e.revert_selected(&ids(&[Y]), |_| {}).unwrap();
        assert_eq!(status(&again, Y), CheckStatus::Conflict);
        assert_eq!(journal_bytes(&dir), only_y);
        assert_ne!(only_y, before);
        let shape_now = shape(&e);
        assert_eq!(shape_now[0].1[1], (Y.to_owned(), State::Applied));
        let fixed = e.apply_selected(&ids(&[DEFENDER]), |_| {}).unwrap();
        assert_eq!(status(&fixed, DEFENDER), CheckStatus::Applied);
    }

    #[test]
    fn a_managed_setting_is_skipped_without_a_trace() {
        let (dir, state, mut e) = two_batches();
        let before = journal_bytes(&dir);
        state.borrow_mut().blocked = true;
        let report = e.revert_selected(&ids(&[X]), |_| {}).unwrap();
        assert_eq!(status(&report, X), CheckStatus::Skipped);
        assert_eq!(journal_bytes(&dir), before);
        assert_eq!(restores(&state, X), 0);
    }

    #[test]
    fn a_change_made_just_before_the_write_writes_nothing_and_records_no_intent() {
        let (dir, state, mut e) = two_batches();
        let before = journal_bytes(&dir);
        let at = state.borrow().observe_count + 2;
        state.borrow_mut().drift_at = Some((at, json!("NotConfigured")));
        let report = e.revert_selected(&ids(&[X]), |_| {}).unwrap();
        assert_eq!(status(&report, X), CheckStatus::Conflict);
        assert_eq!(restores(&state, X), 0);
        assert_eq!(value(&state, X), json!("NotConfigured"));
        assert_eq!(journal_bytes(&dir), before);
        invariants(&dir, &e);
    }

    #[test]
    fn a_setting_already_back_is_recorded_as_put_back_without_a_write() {
        let (dir, state, mut e) = two_batches();
        set(&state, X, json!("Allow"));
        let writes = state.borrow().writes.len();
        let report = e.revert_selected(&ids(&[X]), |_| {}).unwrap();
        assert_eq!(status(&report, X), CheckStatus::Unchanged);
        assert_eq!(state.borrow().writes.len(), writes);
        assert_eq!(shape(&e)[0].1[0], (X.to_owned(), State::Restored));
        invariants(&dir, &e);
    }

    #[test]
    fn an_interrupted_undo_finishes_without_a_second_write() {
        let (dir, state, mut e) = two_batches();
        state.borrow_mut().fail_write = true;
        assert!(e.revert_selected(&ids(&[X]), |_| {}).is_err());
        state.borrow_mut().fail_write = false;
        drop(e);
        let mut e = reopen(&dir, &state, &controls());
        invariants(&dir, &e);
        assert_eq!(shape(&e)[0].1[0], (X.to_owned(), State::Restoring));
        assert_eq!(value(&state, X), json!("Allow"));
        let report = e.revert_selected(&ids(&[X]), |_| {}).unwrap();
        assert_eq!(status(&report, X), CheckStatus::Unchanged);
        assert_eq!(restores(&state, X), 1);
        assert_eq!(shape(&e)[0].1[0], (X.to_owned(), State::Restored));
        invariants(&dir, &e);
    }

    #[test]
    fn an_undo_that_stopped_before_writing_writes_once_when_repeated() {
        let (dir, state, mut e) = two_batches();
        state.borrow_mut().fail_before_write = true;
        assert!(e.revert_selected(&ids(&[X]), |_| {}).is_err());
        state.borrow_mut().fail_before_write = false;
        drop(e);
        let mut e = reopen(&dir, &state, &controls());
        assert_eq!(value(&state, X), json!("Block"));
        let report = e.revert_selected(&ids(&[X]), |_| {}).unwrap();
        assert_eq!(status(&report, X), CheckStatus::Restored);
        assert_eq!(restores(&state, X), 1);
        invariants(&dir, &e);
    }

    #[test]
    fn fixing_a_setting_whose_undo_was_interrupted_settles_it_first() {
        let (dir, state, mut e) = two_batches();
        state.borrow_mut().fail_write = true;
        assert!(e.revert_selected(&ids(&[X]), |_| {}).is_err());
        state.borrow_mut().fail_write = false;
        drop(e);
        let mut e = reopen(&dir, &state, &controls());
        let before = state.borrow().writes.len();
        let report = e.apply_selected(&ids(&[X]), |_| {}).unwrap();
        assert_eq!(status(&report, X), CheckStatus::Applied);
        assert_eq!(state.borrow().writes.len(), before + 1);
        assert_eq!(value(&state, X), json!("Block"));
        invariants(&dir, &e);
        let shape_now = shape(&e);
        assert_eq!(shape_now[0].1[0], (X.to_owned(), State::Restored));
        assert_eq!(shape_now.len(), 3);
    }

    #[test]
    fn a_partly_put_back_batch_does_not_stop_other_fixes() {
        let (dir, state, mut e) = two_batches();
        state.borrow_mut().fail_write = true;
        assert!(e.revert_selected(&ids(&[Y]), |_| {}).is_err());
        state.borrow_mut().fail_write = false;
        drop(e);
        let mut e = reopen(&dir, &state, &controls());
        assert!(!e.load().unwrap()[0].incomplete());
        e.can_change(false).unwrap();
        let report = e.apply_selected(&ids(&[DEFENDER]), |_| {}).unwrap();
        assert_eq!(status(&report, DEFENDER), CheckStatus::Applied);
        invariants(&dir, &e);
    }

    #[test]
    fn undo_last_fixes_after_a_partial_undo_only_puts_back_what_remains() {
        let (dir, state, mut e) = four_controls();
        e.apply_selected(&ids(&[X]), |_| {}).unwrap();
        e.apply_selected(&ids(&[Y, Z]), |_| {}).unwrap();
        e.revert_selected(&ids(&[Z]), |_| {}).unwrap();
        assert_eq!(e.audit().unwrap().undo_next, ids(&[Y]));
        let report = e.revert(|_| {}).unwrap();
        assert_eq!(report.results.len(), 1);
        assert_eq!(status(&report, Y), CheckStatus::Restored);
        assert_eq!(restores(&state, Z), 1);
        invariants(&dir, &e);
        assert_eq!(value(&state, X), json!("Block"));
        assert_eq!(e.audit().unwrap().undo_next, ids(&[X]));
        e.revert_selected(&ids(&[X]), |_| {}).unwrap();
        assert!(e.revert(|_| {}).unwrap().results.is_empty());
    }

    #[test]
    fn undo_last_fixes_skips_a_newest_batch_that_is_already_all_back() {
        let (dir, state, mut e) = four_controls();
        e.apply_selected(&ids(&[X, Y]), |_| {}).unwrap();
        e.apply_selected(&ids(&[Z]), |_| {}).unwrap();
        e.revert_selected(&ids(&[Z]), |_| {}).unwrap();
        assert_eq!(e.audit().unwrap().undo_next, ids(&[X, Y]));
        let report = e.revert(|_| {}).unwrap();
        assert_eq!(report.results.len(), 2);
        assert!(report.results.iter().all(|r| r.status == CheckStatus::Restored));
        assert_eq!(restores(&state, Z), 1);
        invariants(&dir, &e);
        assert!(e.load().unwrap().iter().all(|t| t.reverted));
    }

    #[test]
    fn undo_everything_after_a_partial_undo_resumes_and_never_writes_twice() {
        let (dir, state, mut e) = two_batches();
        state.borrow_mut().fail_write = true;
        assert!(e.revert_selected(&ids(&[Y]), |_| {}).is_err());
        state.borrow_mut().fail_write = false;
        drop(e);
        let mut e = reopen(&dir, &state, &controls());
        e.revert_selected(&ids(&[Z]), |_| {}).unwrap();
        let report = e.revert_all(|_| {}).unwrap();
        invariants(&dir, &e);
        assert_eq!(status(&report, Y), CheckStatus::Unchanged);
        assert_eq!(status(&report, X), CheckStatus::Restored);
        assert!(e.load().unwrap().iter().all(|t| t.reverted));
        for id in [X, Y, Z] {
            assert_eq!(restores(&state, id), 1, "{id}");
            assert_eq!(value(&state, id), json!("Allow"));
        }
    }

    #[test]
    fn every_chosen_undo_survives_a_stop_at_every_storage_step() {
        let all = [X, Y, Z];
        for mask in 1..8usize {
            let chosen: Vec<String> = (0..3)
                .filter(|n| mask & (1 << n) != 0)
                .map(|n| all[n].to_owned())
                .collect();
            let (rdir, rstate, mut reference) = two_batches();
            reference.revert_selected(&chosen, |_| {}).unwrap();
            invariants(&rdir, &reference);
            let want_shape = shape(&reference);
            let want_values: Vec<_> = all.iter().map(|id| value(&rstate, id)).collect();
            for point in POINTS {
                for ordinal in 0..24 {
                    let (dir, state, mut e) = two_batches();
                    fault(point, ordinal);
                    let first = e.revert_selected(&chosen, |_| {});
                    disarm();
                    if first.is_ok() {
                        assert_eq!(shape(&e), want_shape, "{point} {ordinal}");
                        break;
                    }
                    drop(e);
                    let mut e = reopen(&dir, &state, &controls());
                    invariants(&dir, &e);
                    e.revert_selected(&chosen, |_| {}).unwrap();
                    invariants(&dir, &e);
                    let ctx = format!("{chosen:?} {point} {ordinal}");
                    assert_eq!(shape(&e), want_shape, "{ctx}");
                    let values: Vec<_> = all.iter().map(|id| value(&state, id)).collect();
                    assert_eq!(values, want_values, "{ctx}");
                    for id in all {
                        let wanted = usize::from(chosen.iter().any(|c| c == id));
                        assert_eq!(restores(&state, id), wanted, "{id} {ctx}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_stop_at_every_storage_step_during_a_re_fix_keeps_one_owner() {
        for point in POINTS {
            for ordinal in 0..24 {
                let (dir, state, mut e) = two_batches();
                e.revert_selected(&ids(&[X]), |_| {}).unwrap();
                fault(point, ordinal);
                let first = e.apply_selected(&ids(&[X]), |_| {});
                disarm();
                if first.is_ok() {
                    invariants(&dir, &e);
                    break;
                }
                drop(e);
                let mut e = reopen(&dir, &state, &controls());
                invariants(&dir, &e);
                // A batch that never got its first record blocks fixing until it is closed.
                if status(&e.apply_selected(&ids(&[X]), |_| {}).unwrap(), X) != CheckStatus::Applied {
                    e.revert(|_| {}).unwrap();
                    e.apply_selected(&ids(&[X]), |_| {}).unwrap();
                }
                invariants(&dir, &e);
                assert_eq!(value(&state, X), json!("Block"), "{point} {ordinal}");
                e.revert_selected(&ids(&[X]), |_| {}).unwrap();
                assert_eq!(value(&state, X), json!("Allow"), "{point} {ordinal}");
            }
        }
    }

    fn older_batch_stage(phase: usize) -> (Setup, String, Vec<u8>, Record) {
        let (dir, state, mut e) = four_controls();
        e.apply_selected(&ids(&[X]), |_| {}).unwrap();
        e.apply_selected(&ids(&[Z]), |_| {}).unwrap();
        let mut older = e.load().unwrap().remove(0);
        if phase >= 1 {
            e.append(&mut older, Record::RestorePending { id: X.into() })
                .unwrap();
        }
        if phase >= 2 {
            e.append(&mut older, Record::Restored { id: X.into() }).unwrap();
        }
        let record = match phase {
            0 => Record::RestorePending { id: X.into() },
            1 => Record::Restored { id: X.into() },
            _ => Record::Reverted,
        };
        let name = older.name.clone();
        let bytes = older.bytes.clone();
        drop(older);
        ((dir, state, e), name, bytes, record)
    }

    #[test]
    fn an_unpublished_snapshot_of_an_older_batch_is_retired_at_every_cut() {
        for phase in 0..3 {
            let ((dir, state, e), name, committed, record) = older_batch_stage(phase);
            drop(e);
            let mut full = committed.clone();
            full.extend(record_bytes(&record).unwrap());
            let stage = dir.path().join(format!("{name}.jsonl.next"));
            for cut in 0..=full.len() {
                fs::write(&stage, &full[..cut]).unwrap();
                let e = reopen(&dir, &state, &controls());
                assert!(!stage.exists(), "phase {phase} cut {cut}");
                assert_eq!(
                    fs::read(evidence_path(&dir, &name, &full[..cut])).unwrap(),
                    full[..cut]
                );
                assert_eq!(
                    fs::read(dir.path().join(format!("{name}.jsonl"))).unwrap(),
                    committed
                );
                invariants(&dir, &e);
            }
        }
    }

    #[test]
    fn an_unpublished_snapshot_that_starts_reverting_an_older_batch_is_refused() {
        let ((dir, state, e), name, committed, _) = older_batch_stage(0);
        drop(e);
        let stage = dir.path().join(format!("{name}.jsonl.next"));
        let record = record_bytes(&Record::Reverting).unwrap();
        // Up to `{"kind":"revert` the bytes are also the start of a legal record.
        for cut in "{\"kind\":\"reverti".len()..=record.len() {
            let mut bytes = committed.clone();
            bytes.extend(&record[..cut]);
            fs::write(&stage, &bytes).unwrap();
            assert!(
                Engine::open(dir.path().into(), backend(&state, &controls(), "machine-a"))
                    .is_err(),
                "cut {cut}"
            );
        }
    }

    #[test]
    fn restore_records_are_only_legal_after_a_batch_is_sealed_or_reverting() {
        let (_dir, _state, e) = four_controls();
        let name = "00000000000000000001-00000000-0000-4000-8000-000000000001";
        let header = record_bytes(&Record::Header {
            schema: 1,
            machine: "machine-a".into(),
            transaction: name.into(),
            sequence: 1,
        })
        .unwrap();
        let mut unsealed = header.clone();
        for record in [
            Record::Prepare {
                id: X.into(),
                before: json!("Allow"),
            },
            Record::Applied { id: X.into() },
        ] {
            unsealed.extend(record_bytes(&record).unwrap());
        }
        let mut sealed = unsealed.clone();
        sealed.extend(record_bytes(&Record::Sealed).unwrap());
        for (base, legal) in [(&header, false), (&unsealed, false), (&sealed, true)] {
            for record in [
                Record::RestorePending { id: X.into() },
                Record::Restored { id: X.into() },
                Record::Reverted,
            ] {
                let mut bytes = base.clone();
                if matches!(record, Record::Restored { .. }) {
                    bytes.extend(
                        record_bytes(&Record::RestorePending { id: X.into() }).unwrap(),
                    );
                }
                bytes.extend(record_bytes(&record).unwrap());
                let loaded = e.decode(name, None, bytes);
                if legal && !matches!(record, Record::Reverted) {
                    loaded.unwrap_or_else(|e| panic!("{record:?}: {e:#}"));
                } else {
                    assert!(loaded.is_err(), "{record:?} after {} bytes", base.len());
                }
            }
        }
    }

    #[test]
    fn a_protection_another_one_needs_waits_for_it() {
        let items = json!({"items": {"Enabled": null, "WasEnabledBy": null}});
        let setup = || {
            let (dir, state, e) = fixture(MI, items.clone());
            drop(e);
            set(&state, SP, items.clone());
            let mut e = reopen(&dir, &state, &[MI, SP]);
            e.apply_selected(&ids(&[MI]), |_| {}).unwrap();
            e.apply_selected(&ids(&[SP]), |_| {}).unwrap();
            (dir, state, e)
        };
        let (dir, state, mut e) = setup();
        let before = journal_bytes(&dir);
        let report = e.revert_selected(&ids(&[MI]), |_| {}).unwrap();
        assert_eq!(status(&report, MI), CheckStatus::Skipped);
        assert_eq!(journal_bytes(&dir), before);
        assert_eq!(state.borrow().writes.len(), 2);

        let (dir, state, mut e) = setup();
        let report = e.revert_selected(&ids(&[MI, SP]), |_| {}).unwrap();
        let order: Vec<_> = report.results.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(order, [SP, MI]);
        assert!(report.results.iter().all(|r| r.status == CheckStatus::Restored));
        let undone: Vec<_> = state.borrow().writes[2..].iter().map(|w| w.0.clone()).collect();
        assert_eq!(undone, [SP, MI]);
        invariants(&dir, &e);

        let (dir, state, mut e) = setup();
        set(&state, SP, json!({"items": {"Enabled": 0, "WasEnabledBy": null}}));
        let report = e.revert_selected(&ids(&[MI, SP]), |_| {}).unwrap();
        assert_eq!(status(&report, SP), CheckStatus::Conflict);
        assert_eq!(status(&report, MI), CheckStatus::Skipped);
        assert_eq!(state.borrow().writes.len(), 2);
        invariants(&dir, &e);
    }

    #[test]
    fn a_bad_selection_is_refused_before_anything_is_locked_or_changed() {
        let (dir, _state, mut e) = two_batches();
        let before = journal_bytes(&dir);
        let held = e.lock().unwrap();
        for (selection, message) in [
            (ids(&[]), "Select at least one control"),
            (ids(&[X, X]), "Duplicate selected control: firewall.public.inbound"),
            (ids(&["no.such.control"]), "Unknown selected control: no.such.control"),
        ] {
            let err = e.revert_selected(&selection, |_| {}).unwrap_err();
            assert!(format!("{err:#}").contains(message), "{err:#}");
        }
        drop(held);
        assert_eq!(journal_bytes(&dir), before);
    }

    #[test]
    fn settings_with_nothing_recorded_or_in_an_unfinished_batch_are_skipped() {
        let (dir, state, mut e) = two_batches();
        let report = e.revert_selected(&ids(&[DEFENDER]), |_| {}).unwrap();
        assert_eq!(status(&report, DEFENDER), CheckStatus::Skipped);
        assert!(report.results[0].detail.contains("Nothing recorded"));
        e.revert_selected(&ids(&[X]), |_| {}).unwrap();
        let again = e.revert_selected(&ids(&[X]), |_| {}).unwrap();
        assert_eq!(status(&again, X), CheckStatus::Skipped);
        state.borrow_mut().fail_write = true;
        assert!(e.apply_selected(&ids(&[DEFENDER]), |_| {}).is_err());
        state.borrow_mut().fail_write = false;
        drop(e);
        let mut e = reopen(&dir, &state, &controls());
        let before = journal_bytes(&dir);
        let report = e.revert_selected(&ids(&[DEFENDER, Y]), |_| {}).unwrap();
        assert_eq!(status(&report, DEFENDER), CheckStatus::Skipped);
        assert!(report.results[0].detail.starts_with("Revert the active transaction"));
        assert_eq!(status(&report, Y), CheckStatus::Restored);
        assert_ne!(journal_bytes(&dir), before);
        invariants(&dir, &e);
    }

    #[test]
    fn checking_never_changes_anything_after_a_partial_undo() {
        let (dir, state, mut e) = two_batches();
        state.borrow_mut().fail_write = true;
        assert!(e.revert_selected(&ids(&[Y]), |_| {}).is_err());
        state.borrow_mut().fail_write = false;
        drop(e);
        let mut e = reopen(&dir, &state, &controls());
        let before = journal_bytes(&dir);
        let writes = state.borrow().writes.len();
        e.audit().unwrap();
        e.history().unwrap();
        e.undoable_changes().unwrap();
        e.can_change(false).unwrap();
        e.can_change(true).unwrap();
        assert_eq!(journal_bytes(&dir), before);
        assert_eq!(state.borrow().writes.len(), writes);
    }

    #[test]
    fn a_whole_batch_undo_still_records_its_intent_before_the_final_read() {
        let (dir, state, mut e) = two_batches();
        let at = state.borrow().observe_count + 2;
        state.borrow_mut().drift_at = Some((at, json!("NotConfigured")));
        let report = e.revert(|_| {}).unwrap();
        assert_eq!(status(&report, Z), CheckStatus::Conflict);
        let txs = e.load().unwrap();
        assert!(txs[1].reverting);
        assert_eq!(txs[1].entries[0].state, State::Restoring);
        assert_eq!(restores(&state, Z), 0);
        drop(txs);
        invariants(&dir, &e);
    }
}

mod released_journals {
    use super::*;
    use crate::model::CheckStatus;

    const X: &str = "firewall.public.inbound";
    const Y: &str = "firewall.private.inbound";
    const Z: &str = "firewall.domain.inbound";

    /// Journals exactly as 0.8.1 wrote them, with the system values at that moment.
    fn open_fixture(name: &str) -> (TempDir, Rc<RefCell<FakeState>>, Engine) {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/journals/0.8.1")
            .join(name);
        let dir = tempfile::tempdir().unwrap();
        for file in fs::read_dir(&source).unwrap() {
            let file = file.unwrap();
            if file.file_name().to_string_lossy().ends_with(".jsonl") {
                fs::copy(file.path(), dir.path().join(file.file_name())).unwrap();
            }
        }
        let values: serde_json::Map<String, Value> =
            serde_json::from_slice(&fs::read(source.join("state.json")).unwrap()).unwrap();
        let state = Rc::new(RefCell::new(FakeState::default()));
        state.borrow_mut().values = values.into_iter().collect();
        let e = reopen(&dir, &state, &[X, Y, Z]);
        (dir, state, e)
    }

    fn files(dir: &TempDir) -> Vec<(String, Vec<u8>)> {
        let mut all: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|x| x.unwrap())
            .filter(|x| x.file_name().to_string_lossy().ends_with(".jsonl"))
            .map(|x| (x.file_name().to_string_lossy().into_owned(), fs::read(x.path()).unwrap()))
            .collect();
        all.sort();
        all
    }

    fn states(e: &Engine) -> Vec<Vec<State>> {
        e.load()
            .unwrap()
            .iter()
            .map(|t| t.entries.iter().map(|x| x.state).collect())
            .collect()
    }

    fn words(e: &mut Engine) -> Vec<String> {
        e.history()
            .unwrap()
            .iter()
            .map(|h| h.rsplit(' ').next().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn released_journals_load_unchanged_and_read_only_work_leaves_them_alone() {
        let cases: [(&str, Vec<&str>, Vec<Vec<State>>, usize); 5] = [
            ("sealed_two_entries", vec!["applied"], vec![vec![State::Applied; 2]], 2),
            (
                "reverting_restoring",
                vec!["reverting"],
                vec![vec![State::Restoring, State::Restored]],
                1,
            ),
            ("reverted", vec!["reverted"], vec![vec![State::Restored; 2]], 0),
            (
                "three_batch_stack",
                vec!["applied"; 3],
                vec![vec![State::Applied]; 3],
                3,
            ),
            (
                "reverted_then_two_sealed",
                vec!["applied", "applied", "reverted"],
                vec![vec![State::Restored], vec![State::Applied], vec![State::Applied]],
                2,
            ),
        ];
        for (name, history, want, undoable) in cases {
            let (dir, state, mut e) = open_fixture(name);
            let bytes = files(&dir);
            let got = states(&e);
            assert_eq!(got, want, "{name}");
            assert_eq!(words(&mut e), history, "{name}");
            assert_eq!(e.undoable_changes().unwrap(), undoable, "{name}");
            e.audit().unwrap();
            e.history().unwrap();
            assert_eq!(files(&dir), bytes, "{name}");
            assert!(state.borrow().writes.is_empty());
        }
    }

    #[test]
    fn a_released_sealed_batch_can_have_one_setting_put_back() {
        let (dir, state, mut e) = open_fixture("sealed_two_entries");
        let report = e
            .revert_selected(&[Y.to_owned()], |_| {})
            .unwrap();
        assert_eq!(report.results[0].status, CheckStatus::Restored);
        assert_eq!(state.borrow().values[Y], json!("Allow"));
        assert_eq!(state.borrow().values[X], json!("Block"));
        let report = e.revert(|_| {}).unwrap();
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].id, X);
        assert!(e.load().unwrap().iter().all(|t| t.reverted));
        drop(dir);
    }

    #[test]
    fn released_journals_still_undo_and_fix() {
        let (_dir, state, mut e) = open_fixture("reverting_restoring");
        let report = e.revert(|_| {}).unwrap();
        assert_eq!(report.results[0].status, CheckStatus::Restored);
        assert_eq!(state.borrow().values[X], json!("Allow"));
        assert!(e.load().unwrap()[0].reverted);

        let (_dir, state, mut e) = open_fixture("three_batch_stack");
        e.revert_all(|_| {}).unwrap();
        assert!(e.load().unwrap().iter().all(|t| t.reverted));
        for id in [X, Y, Z] {
            assert_eq!(state.borrow().values[id], json!("Allow"));
        }

        let (_dir, state, mut e) = open_fixture("reverted_then_two_sealed");
        state.borrow_mut().values.insert(X.into(), json!("Allow"));
        let report = e.apply_selected(&[X.to_owned()], |_| {}).unwrap();
        assert_eq!(report.results[0].status, CheckStatus::Applied);
        assert_eq!(e.load().unwrap().len(), 4);
    }
}
