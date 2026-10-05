// Engine-only crash/fault tests; included inside engine::tests to reuse its
// stateful backend and exact preference/ACL fixtures.
mod recovery {
    use super::*;

    const NAME: &str = "00000000000000000001-00000000-0000-4000-8000-000000000001";

    fn fault(point: &'static str, after: usize) {
        IO_FAULT.with(|f| *f.borrow_mut() = Some((point, after)));
    }

    fn header() -> Vec<u8> {
        record_bytes(&Record::Header {
            schema: 1,
            machine: "machine-a".into(),
            transaction: NAME.into(),
            sequence: 1,
        })
        .unwrap()
    }

    fn evidence(dir: &TempDir, stem: &str, bytes: &[u8]) -> PathBuf {
        dir.path().join(format!(
            "{stem}.evidence-{}",
            hex::encode(Sha256::digest(bytes))
        ))
    }

    fn crash_records() -> [Record; 7] {
        [
            Record::Prepare {
                id: DEFENDER.into(),
                before: json!(true),
            },
            Record::Applied {
                id: DEFENDER.into(),
            },
            Record::Sealed,
            Record::Reverting,
            Record::RestorePending {
                id: DEFENDER.into(),
            },
            Record::Restored {
                id: DEFENDER.into(),
            },
            Record::Reverted,
        ]
    }

    // All offsets run through the production parser on every OS. Real disk
    // recovery additionally covers empty/copy/record/EOF cuts and both sides of
    // every record boundary, instead of repeating identical fsync/reopen work
    // for every character of an already-validated committed prefix.
    fn durable_cuts(base: usize, bytes: &[u8]) -> Vec<usize> {
        let mut cuts = std::collections::BTreeSet::from([
            0,
            1,
            base / 2,
            base.saturating_sub(1),
            base,
            base + 1,
            base + (bytes.len() - base) / 2,
            bytes.len().saturating_sub(2),
            bytes.len().saturating_sub(1),
            bytes.len(),
        ]);
        for (at, byte) in bytes.iter().enumerate() {
            if *byte == b'\n' {
                cuts.extend([at, at + 1, at + 2]);
            }
        }
        cuts.into_iter().filter(|cut| *cut <= bytes.len()).collect()
    }

    #[test]
    fn byte_fault_injection_preserves_every_prefix_and_uses_bulk_writes() {
        #[derive(Default)]
        struct Writes {
            bytes: Vec<u8>,
            calls: usize,
        }
        impl Write for Writes {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.calls += 1;
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let bytes = header();
        for cut in 0..bytes.len() {
            let mut writer = Writes::default();
            fault("snapshot_byte", cut);
            // Exercise counts spanning calls, including exactly one full call.
            let split = bytes.len() / 2;
            let result = write_snapshot_with_fault(&mut writer, &bytes[..split])
                .and_then(|_| write_snapshot_with_fault(&mut writer, &bytes[split..]));
            assert!(result.is_err(), "cut {cut}");
            assert_eq!(writer.bytes, bytes[..cut]);
            assert!(writer.calls <= 2);
            assert!(IO_FAULT.with(|f| f.borrow().is_none()));
        }
        for armed in [None, Some(("snapshot_sync", 0))] {
            IO_FAULT.with(|f| *f.borrow_mut() = armed);
            let mut writer = Writes::default();
            write_snapshot_with_fault(&mut writer, &bytes).unwrap();
            assert_eq!(writer.bytes, bytes);
            assert_eq!(
                writer.calls, 1,
                "unarmed/other-boundary writes must remain bulk I/O"
            );
            assert_eq!(IO_FAULT.with(|f| *f.borrow()), armed);
        }
        IO_FAULT.with(|f| *f.borrow_mut() = None);
    }

    #[test]
    fn every_cow_snapshot_byte_prefix_preserves_committed_state() {
        let (_dir, state, e) = fixture(DEFENDER, json!(true));
        let mut committed = header();
        let mut prefixes = 0;
        for (phase, record) in crash_records().iter().enumerate() {
            let mut snapshot = committed.clone();
            snapshot.extend(record_bytes(record).unwrap());
            let tx = e.decode(NAME, None, committed.clone()).unwrap();
            for cut in 0..=snapshot.len() {
                let mut written = Vec::new();
                if cut < snapshot.len() {
                    fault("snapshot_byte", cut);
                    assert!(write_snapshot_with_fault(&mut written, &snapshot).is_err());
                } else {
                    write_snapshot_with_fault(&mut written, &snapshot).unwrap();
                }
                assert_eq!(written, snapshot[..cut]);
                e.validate_staged(NAME, &written, std::slice::from_ref(&tx))
                    .unwrap();
                // Staging never contributes an original or a result. Decode
                // the published view independently at every crash offset.
                let visible = e.decode(NAME, None, committed.clone()).unwrap();
                assert_eq!(visible.bytes, committed);
                assert_eq!(visible.entries.len(), usize::from(phase > 0));
                if phase > 0 {
                    assert_eq!(visible.entries[0].before, json!(true));
                    assert_eq!(
                        visible.entries[0].state,
                        match phase {
                            1 => State::Pending,
                            2..=4 => State::Applied,
                            5 => State::Restoring,
                            6 => State::Restored,
                            _ => unreachable!(),
                        }
                    );
                }
                prefixes += 1;
            }
            committed = snapshot;
        }
        assert!(state.borrow().events.is_empty());
        eprintln!("exhaustive_snapshot_prefixes={prefixes}");
    }

    #[test]
    fn cow_snapshot_durable_cuts_reopen_and_undo_uses_only_committed_originals() {
        let mut committed = header();
        let mut cuts = 0;
        for (phase, record) in crash_records().iter().enumerate() {
            let mut snapshot = committed.clone();
            snapshot.extend(record_bytes(record).unwrap());
            for cut in durable_cuts(committed.len(), &snapshot) {
                let current = if (1..=4).contains(&phase) {
                    json!(false)
                } else {
                    json!(true)
                };
                let (dir, state, mut e) = fixture(DEFENDER, current);
                let path = dir.path().join(format!("{NAME}.jsonl"));
                fs::write(&path, &committed).unwrap();
                let mut tx = e.load().unwrap().pop().unwrap();
                if cut == snapshot.len() {
                    fault("snapshot_sync", 0);
                } else {
                    fault("snapshot_byte", cut);
                }
                let record: Record =
                    serde_json::from_slice(&record_bytes(record).unwrap()).unwrap();
                assert!(
                    e.append(&mut tx, record).is_err(),
                    "phase {phase}, cut {cut}"
                );
                assert!(e.storage_failed);
                assert!(e.revert(|_, _| {}).is_err());
                assert_eq!(fs::read(&path).unwrap(), committed);
                assert!(state.borrow().writes.is_empty());
                drop((tx, e));
                let mut e = reopen(&dir, &state, &[DEFENDER]);
                assert_eq!(
                    fs::read(evidence(&dir, NAME, &snapshot[..cut])).unwrap(),
                    snapshot[..cut]
                );
                assert!(!dir.path().join(format!("{NAME}.jsonl.next")).exists());
                assert_eq!(fs::read(&path).unwrap(), committed);
                e.revert(|_, _| {}).unwrap();
                assert_eq!(state.borrow().values[DEFENDER], json!(true));
                assert_eq!(
                    state.borrow().writes.len(),
                    usize::from((1..=4).contains(&phase)),
                    "phase {phase}, cut {cut}"
                );
                drop(e);
                assert!(reopen(&dir, &state, &[DEFENDER]).load().unwrap()[0].reverted);
                cuts += 1;
            }
            committed = snapshot;
        }
        eprintln!("durable_snapshot_cuts={cuts}");
    }

    #[test]
    fn every_header_creation_prefix_is_unpublished_and_retained() {
        let (_dir, state, e) = fixture(DEFENDER, json!(true));
        let bytes = header();
        for cut in 0..=bytes.len() {
            e.validate_staged(NAME, &bytes[..cut], &[]).unwrap();
        }
        assert!(state.borrow().events.is_empty());
        let cuts = durable_cuts(0, &bytes);
        eprintln!(
            "exhaustive_header_prefixes={} durable_header_cuts={}",
            bytes.len() + 1,
            cuts.len()
        );
        for cut in cuts {
            let (dir, state, mut e) = fixture(DEFENDER, json!(true));
            if cut == header().len() {
                fault("snapshot_sync", 0);
            } else {
                fault("snapshot_byte", cut);
            }
            assert!(e.create(1).is_err());
            drop(e);
            let stage = fs::read_dir(dir.path())
                .unwrap()
                .map(|x| x.unwrap().path())
                .find(|p| p.to_string_lossy().ends_with(".jsonl.next"))
                .unwrap();
            let bytes = fs::read(&stage).unwrap();
            let stem = stage
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .strip_suffix(".jsonl.next")
                .unwrap()
                .to_owned();
            let mut e = reopen(&dir, &state, &[DEFENDER]);
            assert!(e.history().unwrap().is_empty());
            assert_eq!(fs::read(evidence(&dir, &stem, &bytes)).unwrap(), bytes);
            assert!(state.borrow().writes.is_empty());
            e.apply(|_, _| {}).unwrap();
            e.revert(|_, _| {}).unwrap();
            assert_eq!(state.borrow().values[DEFENDER], json!(true));
        }
    }

    #[test]
    fn every_prepare_record_prefix_for_typed_originals_is_recoverable_in_staging() {
        for (id, before) in [
            (FIREWALL, json!("NotConfigured")),
            ("uac.consent", json!({"present":true,"value":0})),
            (
                "lsa.limit_blank_password_use",
                json!({"present":false,"value":null}),
            ),
            ("permissions.service.bits", acl_snapshot(0x0002_0012, 1)),
        ] {
            let record = record_bytes(&Record::Prepare {
                id: id.into(),
                before: before.clone(),
            })
            .unwrap();
            let (_dir, state, e) = fixture(id, before.clone());
            let committed = e.decode(NAME, None, header()).unwrap();
            for cut in 0..=record.len() {
                let mut snapshot = header();
                snapshot.extend(&record[..cut]);
                e.validate_staged(NAME, &snapshot, std::slice::from_ref(&committed))
                    .unwrap();
                assert!(e.decode(NAME, None, header()).unwrap().entries.is_empty());
            }
            assert!(state.borrow().events.is_empty());
            let cuts = durable_cuts(0, &record);
            eprintln!(
                "{id}: exhaustive_prepare_prefixes={} durable_prepare_cuts={}",
                record.len() + 1,
                cuts.len()
            );
            for cut in cuts {
                let (dir, state, mut e) = fixture(id, before.clone());
                fs::write(dir.path().join(format!("{NAME}.jsonl")), header()).unwrap();
                let mut tx = e.load().unwrap().pop().unwrap();
                let mut snapshot = header();
                snapshot.extend(&record[..cut]);
                if cut == record.len() {
                    fault("snapshot_sync", 0);
                } else {
                    fault("snapshot_byte", snapshot.len());
                }
                assert!(e
                    .append(
                        &mut tx,
                        Record::Prepare {
                            id: id.into(),
                            before: before.clone()
                        }
                    )
                    .is_err());
                drop((tx, e));
                let mut e = reopen(&dir, &state, &[id]);
                assert!(e.load().unwrap()[0].entries.is_empty());
                e.revert(|_, _| {}).unwrap();
                assert!(state.borrow().writes.is_empty());
                assert_eq!(state.borrow().values[id], before);
                assert_eq!(fs::read(evidence(&dir, NAME, &snapshot)).unwrap(), snapshot);
            }
        }
    }

    #[test]
    fn actual_apply_and_restore_stop_at_each_storage_commit_boundary() {
        for point in [
            "snapshot_create",
            "snapshot_write",
            "snapshot_sync",
            "snapshot_replace",
            "snapshot_directory",
            "snapshot_reopen",
        ] {
            // Header, Prepare, Applied and Seal; Reverting, RestorePending,
            // Restored and Reverted. Injection counts actual append calls.
            for restore in [false, true] {
                for ordinal in 0..4 {
                    let (dir, state, mut e) = fixture(DEFENDER, json!(true));
                    if restore {
                        e.apply(|_, _| {}).unwrap();
                    }
                    fault(point, ordinal);
                    let result = if restore {
                        e.revert(|_, _| {})
                    } else {
                        e.apply(|_, _| {})
                    };
                    assert!(result.is_err(), "{point} {restore} {ordinal}");
                    let expected_writes = usize::from(restore) + usize::from(ordinal >= 2);
                    assert_eq!(
                        state.borrow().writes.len(),
                        expected_writes,
                        "{point} {restore} {ordinal}"
                    );
                    assert!(e.history().is_err());
                    drop(e);
                    let mut e = reopen(&dir, &state, &[DEFENDER]);
                    e.revert(|_, _| {}).unwrap();
                    assert_eq!(state.borrow().values[DEFENDER], json!(true));
                    // A failed result publication cannot repeat a restore that
                    // already happened; a failed intent cannot authorize one.
                    assert_eq!(
                        state.borrow().writes.len(),
                        if restore || ordinal >= 2 { 2 } else { 0 }
                    );
                    drop(e);
                    reopen(&dir, &state, &[DEFENDER]).history().unwrap();
                }
            }
        }
    }

    #[test]
    fn legacy_tail_recovery_is_cow_and_evidence_survives_recovery_crashes() {
        for point in [
            "evidence_write",
            "evidence_sync",
            "evidence_directory",
            "snapshot_create",
            "snapshot_write",
            "snapshot_sync",
            "snapshot_replace",
            "snapshot_directory",
            "snapshot_reopen",
        ] {
            let (dir, state, mut e) = fixture(DEFENDER, json!(false));
            let tx = prepare(&mut e, 1, DEFENDER, json!(true));
            let name = tx.name.clone();
            let path = dir.path().join(format!("{name}.jsonl"));
            drop(tx);
            let mut damaged = fs::read(&path).unwrap();
            damaged.extend(b"{\"kind\":\"applied\",\"id\":\"defender.real");
            fs::write(&path, &damaged).unwrap();
            fault(point, 0);
            assert!(e.revert(|_, _| {}).is_err(), "{point}");
            assert!(state.borrow().writes.is_empty());
            if point.starts_with("evidence_") {
                assert_eq!(fs::read(&path).unwrap(), damaged);
            } else {
                assert_eq!(fs::read(evidence(&dir, &name, &damaged)).unwrap(), damaged);
            }
            drop(e);
            let mut e = reopen(&dir, &state, &[DEFENDER]);
            e.revert(|_, _| {}).unwrap();
            assert_eq!(state.borrow().values[DEFENDER], json!(true));
            assert_eq!(state.borrow().writes.len(), 1);
            assert_eq!(fs::read(evidence(&dir, &name, &damaged)).unwrap(), damaged);
        }
    }

    #[test]
    fn evidence_copy_failure_never_retires_stage_and_partial_evidence_can_resume() {
        for point in [
            "evidence_write",
            "evidence_sync",
            "evidence_directory",
            "retire_stage",
        ] {
            let (dir, state, e) = fixture(DEFENDER, json!(true));
            let path = dir.path().join(format!("{NAME}.jsonl.next"));
            let bytes = &header()[..40];
            fs::write(&path, bytes).unwrap();
            let archived = evidence(&dir, NAME, bytes);
            fs::write(&archived, &bytes[..10]).unwrap();
            drop(e);
            fault(point, 0);
            assert!(
                Engine::open(dir.path().into(), backend(&state, &[DEFENDER], "machine-a")).is_err()
            );
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert!(state.borrow().events.is_empty());
            let mut e = reopen(&dir, &state, &[DEFENDER]);
            assert_eq!(fs::read(archived).unwrap(), bytes);
            assert!(!path.exists());
            assert!(e.history().unwrap().is_empty());
        }
    }

    #[test]
    fn malformed_completed_interior_wrong_machine_and_invalid_transitions_never_recover() {
        for staged in [false, true] {
            for tail in [
                b"{\"kind\":\"prepare\",\"id\":\"unknown\",\"before\":t".as_slice(),
                b"{\"kind\":\"prepare\",\"id\":\"defender.realtime\",\"before\":\"bad",
                b"{\"kind\":\"applied\",\"id\":\"defender.real", // no Prepare
                b"{\"kind\":\"reverted\"}",
                b"{\"kind\":\"sealed\",\"extra\":1}",
                b"{\"kind\":\"prepare\"}",
                b"{\"kind\":\"preX",
                b"garbage\n{\"kind\":",
                b"{\"kind\":\"sealed\"}\n{\"kind\":",
            ] {
                // A complete Sealed followed by an incomplete legal record is
                // a valid legacy append, but two records in one stage are not.
                if !staged && tail == b"{\"kind\":\"sealed\"}\n{\"kind\":" {
                    continue;
                }
                let (dir, state, mut e) = fixture(DEFENDER, json!(true));
                let wal = dir.path().join(format!("{NAME}.jsonl"));
                fs::write(&wal, header()).unwrap();
                let path = if staged {
                    dir.path().join(format!("{NAME}.jsonl.next"))
                } else {
                    wal
                };
                let mut bytes = header();
                bytes.extend(tail);
                fs::write(&path, &bytes).unwrap();
                assert!(
                    e.revert(|_, _| {}).is_err(),
                    "{staged}: {:?}",
                    String::from_utf8_lossy(tail)
                );
                assert!(state.borrow().events.is_empty());
                assert_eq!(fs::read(&path).unwrap(), bytes);
                assert!(!evidence(&dir, NAME, &bytes).exists());
            }
        }
        for staged in [false, true] {
            let (dir, state, e) = fixture(DEFENDER, json!(true));
            let mut bytes = header();
            bytes.extend(b"{\"kind\":\"pre");
            let path = dir.path().join(format!("{NAME}.jsonl"));
            fs::write(&path, &bytes).unwrap();
            if staged {
                fs::write(dir.path().join(format!("{NAME}.jsonl.next")), header()).unwrap();
            }
            drop(e);
            assert!(
                Engine::open(dir.path().into(), backend(&state, &[DEFENDER], "machine-b")).is_err()
            );
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert!(!evidence(&dir, NAME, &bytes).exists());
        }
    }

    #[test]
    fn directory_preflight_rejects_corruption_before_preserving_any_other_tail() {
        let (dir, state, mut e) = fixture(DEFENDER, json!(false));
        let tx = prepare(&mut e, 1, DEFENDER, json!(true));
        let name = tx.name.clone();
        let path = dir.path().join(format!("{name}.jsonl"));
        drop(tx);
        let mut bytes = fs::read(&path).unwrap();
        bytes.extend(b"{\"kind\":\"appl");
        fs::write(&path, &bytes).unwrap();
        fs::write(dir.path().join("unknown-file"), b"unknown").unwrap();
        assert!(e.revert(|_, _| {}).is_err());
        assert!(!evidence(&dir, &name, &bytes).exists());
        assert_eq!(fs::read(path).unwrap(), bytes);
        assert!(state.borrow().events.is_empty());
    }

    #[test]
    fn operations_namespace_coexists_and_rejects_wrong_types() {
        for name in ["operations", "Patching", "App", crate::platform::WEB_PROTECTION] {
            let (dir, _state, mut e) = fixture(DEFENDER, json!(true));
            let namespace = dir.path().join(name);
            fs::create_dir(&namespace).unwrap();
            fs::write(namespace.join("module-owned-data"), b"opaque").unwrap();
            // Other namespaces are opaque to diagnostic WAL loading, even when
            // their module-owned records are corrupt or require verification.
            e.audit().unwrap();
            assert!(e.history().unwrap().is_empty());
            assert_eq!(
                fs::read(namespace.join("module-owned-data")).unwrap(),
                b"opaque"
            );
            drop(e);
            let (dir, state, mut e) = fixture(DEFENDER, json!(true));
            fs::write(dir.path().join(name), b"not a directory").unwrap();
            assert_reserved_history_rejected(&mut e, &state);
        }
    }

    #[test]
    fn lost_updater_supervisor_releases_lock_but_intent_still_blocks_apply_and_revert() {
        use crate::updater::interlock::{check, Activity};
        for phase in ["started", "exited"] {
            for active_wal in [false, true] {
                let (dir, state, mut e) = fixture(DEFENDER, json!(true));
                if active_wal {
                    e.apply(|_, _| {}).unwrap();
                }
                let wal = fs::read_dir(dir.path())
                    .unwrap()
                    .map(|p| p.unwrap().path())
                    .find(|p| p.extension().is_some_and(|x| x == "jsonl"));
                let original_wal = wal.as_ref().map(|p| fs::read(p).unwrap());
                let updates = dir.path().join("Updates");
                fs::create_dir(&updates).unwrap();
                let attempt = updates.join("install-attempt.json");
                // Real typed updater intent, persisted while its supervisor owns
                // the lock. Losing that owner never turns Exited into healthy.
                let bytes = serde_json::to_vec(&json!({"schema":1,"version":"9.0.0",
                    "before":{"schema":1,"version":"8.0.0","task":"ready","monitor":"running"},
                    "health":"delivery_v1","phase":phase}))
                .unwrap();
                let supervisor = e.lock().unwrap();
                let mut intent = File::create(&attempt).unwrap();
                intent.write_all(&bytes).unwrap();
                intent.sync_all().unwrap();
                drop((intent, supervisor, e));
                let mut e = reopen(&dir, &state, &[DEFENDER]);
                let lock_path = dir.path().join(LOCK_NAME);
                let read_attempt = attempt.clone();
                e.mutation_check = Some(Box::new(move |held| {
                    let contender = open_file(&lock_path, false)?;
                    assert!(
                        fs2::FileExt::try_lock_exclusive(&contender).is_err(),
                        "inspection must run under engine.lock"
                    );
                    check(Activity::Hardening, held, |subsystem, _| {
                        if subsystem == Activity::Updater {
                            crate::updater::install_idle(&fs::read(&read_attempt)?)?;
                        }
                        Ok(())
                    })
                }));
                for data in [&bytes[..], b"{", b"{\"schema\":99}"] {
                    fs::write(&attempt, data).unwrap();
                    let events = state.borrow().events.len();
                    let writes = state.borrow().writes.len();
                    assert!(e.apply(|_, _| panic!("blocked before callbacks")).is_err());
                    assert!(e
                        .apply_selected(&[DEFENDER.into()], |_, _| panic!("blocked"))
                        .is_err());
                    assert!(e.revert(|_, _| panic!("blocked")).is_err());
                    assert_eq!(state.borrow().events.len(), events);
                    assert_eq!(state.borrow().writes.len(), writes);
                    assert_eq!(wal.as_ref().map(|p| fs::read(p).unwrap()), original_wal);
                    assert_eq!(fs::read(&attempt).unwrap(), data);
                    // Gates must not poison the engine or prevent diagnosis.
                    e.audit().unwrap();
                    e.history().unwrap();
                }
                fs::write(&attempt, b"null").unwrap(); // independently resolved by updater
                if !active_wal {
                    e.apply(|_, _| {}).unwrap();
                }
                e.revert(|_, _| {}).unwrap();
                assert_eq!(state.borrow().values[DEFENDER], json!(true));
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn native_mutation_interlock_rejects_foreign_lock_without_running_a_backend() {
        let held = tempfile::tempfile().unwrap();
        fs2::FileExt::try_lock_exclusive(&held).unwrap();
        assert!(native_mutation_interlocks(&held).is_err());
    }

    #[test]
    fn snapshot_and_evidence_hardlinks_are_rejected() {
        for suffix in [
            ".jsonl.next".to_owned(),
            format!(".evidence-{}", "0".repeat(64)),
        ] {
            let (dir, state, mut e) = fixture(DEFENDER, json!(true));
            let outside = tempfile::tempdir().unwrap();
            let path = outside.path().join("source");
            fs::write(&path, b"data").unwrap();
            fs::hard_link(&path, dir.path().join(format!("{NAME}{suffix}"))).unwrap();
            assert_reserved_history_rejected(&mut e, &state);
        }
    }

    #[test]
    fn recovery_uses_shared_lock_and_refuses_inconsistent_evidence() {
        let (dir, state, mut e) = fixture(DEFENDER, json!(false));
        let tx = prepare(&mut e, 1, DEFENDER, json!(true));
        let name = tx.name.clone();
        let path = dir.path().join(format!("{name}.jsonl"));
        drop(tx);
        let mut bytes = fs::read(&path).unwrap();
        bytes.extend(b"{\"kind\":\"appl");
        fs::write(&path, &bytes).unwrap();
        let held = e.lock().unwrap();
        assert!(e.revert(|_, _| {}).is_err());
        assert!(
            Engine::open(dir.path().into(), backend(&state, &[DEFENDER], "machine-a")).is_err()
        );
        let archived = evidence(&dir, &name, &bytes);
        assert!(!archived.exists());
        drop(held);
        fs::write(&archived, b"inconsistent evidence").unwrap();
        assert!(e.revert(|_, _| {}).is_err());
        assert_eq!(fs::read(archived).unwrap(), b"inconsistent evidence");
        assert_eq!(fs::read(path).unwrap(), bytes);
        assert!(state.borrow().events.is_empty());
    }

    #[test]
    fn cow_detects_same_length_content_changes_and_corrupted_copied_prefixes() {
        let (dir, state, mut e) = fixture(DEFENDER, json!(false));
        let mut tx = prepare(&mut e, 1, DEFENDER, json!(true));
        let path = dir.path().join(format!("{}.jsonl", tx.name));
        let original = fs::read(&path).unwrap();
        let mut changed = original.clone();
        changed[20] ^= 1;
        fs::write(&path, &changed).unwrap();
        assert!(e
            .append(
                &mut tx,
                Record::Applied {
                    id: DEFENDER.into()
                }
            )
            .is_err());
        assert_eq!(fs::read(&path).unwrap(), changed);
        let stage = dir.path().join(format!("{}.jsonl.next", tx.name));
        drop((tx, e));
        fs::write(&path, &original).unwrap();
        fs::write(&stage, &changed[..40]).unwrap();
        assert!(
            Engine::open(dir.path().into(), backend(&state, &[DEFENDER], "machine-a")).is_err()
        );
        assert_eq!(fs::read(stage).unwrap(), changed[..40]);
        assert!(state.borrow().writes.is_empty());
    }

    #[test]
    fn completed_legacy_markers_with_extra_fields_are_not_recovered() {
        for marker in ["sealed", "reverting", "reverted"] {
            let (dir, state, mut e) = fixture(DEFENDER, json!(true));
            let mut bytes = header();
            bytes.extend(format!("{{\"kind\":\"{marker}\",\"extra\":true}}\n").as_bytes());
            bytes.extend(b"{\"kind\":");
            let path = dir.path().join(format!("{NAME}.jsonl"));
            fs::write(&path, &bytes).unwrap();
            assert!(e.revert(|_, _| {}).is_err());
            assert_eq!(fs::read(path).unwrap(), bytes);
            assert!(!evidence(&dir, NAME, &bytes).exists());
            assert!(state.borrow().events.is_empty());
        }
    }

    #[test]
    fn older_active_transaction_can_recover_after_newer_batch_was_reverted() {
        let (dir, state, e) = fixture(DEFENDER, json!(true));
        drop(e);
        state
            .borrow_mut()
            .values
            .insert(FIREWALL.into(), json!("Allow"));
        let mut e = reopen(&dir, &state, &[DEFENDER, FIREWALL]);
        let older = e
            .apply_selected(&[DEFENDER.into()], |_, _| {})
            .unwrap()
            .transaction
            .unwrap();
        e.apply_selected(&[FIREWALL.into()], |_, _| {}).unwrap();
        e.revert(|_, _| {}).unwrap();
        fault("snapshot_sync", 0);
        assert!(e.revert(|_, _| {}).is_err());
        drop(e);
        let mut e = reopen(&dir, &state, &[DEFENDER, FIREWALL]);
        assert_eq!(e.revert(|_, _| {}).unwrap().transaction, Some(older));
        assert_eq!(state.borrow().values[DEFENDER], json!(true));
        assert_eq!(state.borrow().values[FIREWALL], json!("Allow"));
        assert_eq!(state.borrow().writes.len(), 4);
    }

    #[test]
    fn overlapping_complete_or_partial_unpublished_prepares_fail_before_recovery() {
        for staged in [false, true] {
            for complete in [false, true] {
                let (dir, state, mut e) = fixture(DEFENDER, json!(true));
                e.apply(|_, _| {}).unwrap();
                let newer = e.create(2).unwrap();
                let name = newer.name.clone();
                let wal = dir.path().join(format!("{name}.jsonl"));
                let mut bytes = fs::read(&wal).unwrap();
                drop(newer);
                let record = record_bytes(&Record::Prepare {
                    id: DEFENDER.into(),
                    before: json!(true),
                })
                .unwrap();
                bytes.extend(
                    &record[..if complete {
                        record.len()
                    } else {
                        record.len() - 4
                    }],
                );
                let path = if staged {
                    dir.path().join(format!("{name}.jsonl.next"))
                } else {
                    wal
                };
                fs::write(&path, &bytes).unwrap();
                let events = state.borrow().events.len();
                assert!(e.revert(|_, _| {}).is_err());
                assert_eq!(fs::read(path).unwrap(), bytes);
                assert!(!evidence(&dir, &name, &bytes).exists());
                assert_eq!(state.borrow().events.len(), events);
            }
        }
    }

    #[test]
    fn recovered_sealed_legacy_tail_blocks_new_batches_until_explicit_undo() {
        let (dir, state, e) = fixture(DEFENDER, json!(true));
        drop(e);
        state
            .borrow_mut()
            .values
            .insert(FIREWALL.into(), json!("Allow"));
        let mut e = reopen(&dir, &state, &[DEFENDER, FIREWALL]);
        let name = e
            .apply_selected(&[DEFENDER.into()], |_, _| {})
            .unwrap()
            .transaction
            .unwrap();
        let path = dir.path().join(format!("{name}.jsonl"));
        let mut bytes = fs::read(&path).unwrap();
        bytes.extend(b"{\"kind\":\"reverting\"");
        fs::write(&path, &bytes).unwrap();
        drop(e);
        let mut e = reopen(&dir, &state, &[DEFENDER, FIREWALL]);
        let observations = state.borrow().observe_count;
        assert_eq!(
            e.apply_selected(&[FIREWALL.into()], |_, _| {})
                .unwrap()
                .results[0]
                .status,
            "pending"
        );
        assert_eq!(state.borrow().observe_count, observations);
        assert_eq!(state.borrow().writes.len(), 1);
        assert_eq!(e.history().unwrap(), [format!("{name} pending")]);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        e.revert(|_, _| {}).unwrap();
        e.apply_selected(&[FIREWALL.into()], |_, _| {}).unwrap();
        e.revert(|_, _| {}).unwrap();
        assert_eq!(state.borrow().values[DEFENDER], json!(true));
        assert_eq!(state.borrow().values[FIREWALL], json!("Allow"));
        assert_eq!(fs::read(evidence(&dir, &name, &bytes)).unwrap(), bytes);
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_evidence_and_operations_symlinks_are_rejected() {
        for name in [
            format!("{NAME}.jsonl.next"),
            format!("{NAME}.evidence-{}", "0".repeat(64)),
            "operations".into(),
            "Patching".into(),
        ] {
            let (dir, state, mut e) = fixture(DEFENDER, json!(true));
            let outside = tempfile::tempdir().unwrap();
            let source = outside.path().join("source");
            if matches!(name.as_str(), "operations" | "Patching") {
                fs::create_dir(&source).unwrap();
            } else {
                fs::write(&source, b"opaque").unwrap();
            }
            std::os::unix::fs::symlink(source, dir.path().join(name)).unwrap();
            assert_reserved_history_rejected(&mut e, &state);
        }
    }
}
