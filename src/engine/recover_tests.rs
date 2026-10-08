// Damaged-history classification and starting fresh.
mod damaged_history {
    use super::*;
    use crate::engine::recover::{
        inspect, start_fresh, DamageKind, JournalDamaged, NotDamaged, DAMAGED,
    };

    const OTHER: &str = "00000000000000000002-00000000-0000-4000-8000-000000000002";

    fn snapshot(dir: &TempDir) -> Vec<(String, Vec<u8>)> {
        let mut all: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                let bytes = fs::read(e.path()).unwrap_or_default();
                (e.file_name().into_string().unwrap(), bytes)
            })
            .collect();
        all.sort();
        all
    }

    fn history() -> (TempDir, Rc<RefCell<FakeState>>) {
        let (dir, state, mut e) = fixture(DEFENDER, json!(false));
        prepare(&mut e, 1, DEFENDER, json!(true));
        drop(e);
        (dir, state)
    }

    fn expect(dir: &TempDir, state: &Rc<RefCell<FakeState>>, kind: DamageKind, files: usize) {
        let before = snapshot(dir);
        let report = inspect(dir.path(), backend(state, &[DEFENDER], "machine-a")).unwrap();
        assert_eq!(report.kind, Some(kind), "{:?}", report.causes);
        assert_eq!(report.files, files, "{:?}", report.causes);
        assert!(!report.causes.is_empty());
        let error = Engine::open(dir.path().into(), backend(state, &[DEFENDER], "machine-a"))
            .err()
            .expect("damaged history must not open");
        assert_eq!(
            error.downcast_ref::<JournalDamaged>(),
            Some(&JournalDamaged { kind, files })
        );
        assert_eq!(snapshot(dir), before);
    }

    #[test]
    fn healthy_history_is_not_damaged() {
        let (dir, state) = history();
        let report = inspect(dir.path(), backend(&state, &[DEFENDER], "machine-a")).unwrap();
        assert_eq!(report.kind, None);
        assert!(matches!(
            start_fresh(dir.path(), backend(&state, &[DEFENDER], "machine-a"))
                .unwrap_err()
                .downcast_ref::<NotDamaged>(),
            Some(NotDamaged)
        ));
        assert!(!dir.path().join(DAMAGED).exists());
        reopen(&dir, &state, &[DEFENDER]);
    }

    #[test]
    fn missing_folder_is_not_damaged() {
        let dir = tempfile::tempdir().unwrap();
        let state = Rc::new(RefCell::new(FakeState::default()));
        let gone = dir.path().join("none");
        let report = inspect(&gone, backend(&state, &[DEFENDER], "machine-a")).unwrap();
        assert_eq!(report.kind, None);
        assert!(!gone.exists());
    }

    #[test]
    fn an_unreadable_file_beside_good_history_is_partial() {
        let (dir, state) = history();
        fs::write(dir.path().join(format!("{OTHER}.jsonl")), b"garbage\n").unwrap();
        expect(&dir, &state, DamageKind::Partial, 1);
    }

    #[test]
    fn a_file_without_a_header_is_partial() {
        let (dir, state) = history();
        fs::write(dir.path().join(format!("{OTHER}.jsonl")), b"").unwrap();
        expect(&dir, &state, DamageKind::Partial, 1);
    }

    #[test]
    fn a_bad_file_name_is_partial() {
        let (dir, state) = history();
        fs::write(dir.path().join("1-not-a-uuid.jsonl"), b"x\n").unwrap();
        expect(&dir, &state, DamageKind::Partial, 1);
    }

    #[test]
    fn history_from_another_pc_is_named_as_such() {
        let (dir, state) = history();
        let other = Rc::new(RefCell::new(FakeState::default()));
        let before = snapshot(&dir);
        let backend = backend(&other, &[DEFENDER], "machine-b");
        let report = inspect(dir.path(), backend).unwrap();
        assert_eq!(report.kind, Some(DamageKind::OtherPc));
        assert_eq!(report.files, 1);
        let error = Engine::open(
            dir.path().into(),
            self::backend(&state, &[DEFENDER], "machine-b"),
        )
        .err()
        .unwrap();
        assert_eq!(
            error.downcast_ref::<JournalDamaged>().map(|d| d.kind),
            Some(DamageKind::OtherPc)
        );
        assert_eq!(snapshot(&dir), before);
    }

    #[test]
    fn two_batches_owning_the_same_setting_are_total() {
        let (dir, state, mut e) = fixture(DEFENDER, json!(false));
        prepare(&mut e, 1, DEFENDER, json!(true));
        prepare(&mut e, 2, DEFENDER, json!(true));
        drop(e);
        expect(&dir, &state, DamageKind::Total, 2);
    }

    #[test]
    fn a_damaged_middle_line_with_nothing_else_readable_is_total() {
        let (dir, state) = history();
        let path = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|x| x == "jsonl"))
            .unwrap();
        let mut bytes = fs::read(&path).unwrap();
        bytes.extend(b"not json\n{\"kind\":\"sealed\"}\n");
        fs::write(&path, bytes).unwrap();
        expect(&dir, &state, DamageKind::Total, 1);
    }

    #[test]
    fn an_unknown_file_is_total() {
        let (dir, state) = history();
        fs::write(dir.path().join("unknown-file"), b"unknown").unwrap();
        expect(&dir, &state, DamageKind::Total, 1);
    }

    #[test]
    fn a_torn_tail_that_cannot_be_proved_is_total() {
        let (dir, state) = history();
        let path = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|x| x == "jsonl"))
            .unwrap();
        let mut bytes = fs::read(&path).unwrap();
        bytes.extend(b"{\"kind\":\"sealed\"}");
        fs::write(&path, bytes).unwrap();
        expect(&dir, &state, DamageKind::Total, 1);
    }

    fn damage(dir: &TempDir) {
        fs::write(dir.path().join(format!("{OTHER}.jsonl")), b"garbage\n").unwrap();
        fs::write(dir.path().join("unknown-file"), b"unknown").unwrap();
    }

    fn sets(dir: &TempDir) -> Vec<PathBuf> {
        let mut sets: Vec<_> = fs::read_dir(dir.path().join(DAMAGED))
            .map(|d| d.map(|e| e.unwrap().path()).collect())
            .unwrap_or_default();
        sets.sort();
        sets
    }

    #[test]
    fn starting_fresh_keeps_every_file_with_matching_hashes_and_reopens() {
        let (dir, state) = history();
        damage(&dir);
        fs::write(dir.path().join("update.lock"), b"").unwrap();
        let before = snapshot(&dir);
        let done = start_fresh(dir.path(), backend(&state, &[DEFENDER], "machine-a")).unwrap();
        assert_eq!(done.moved, 3);
        let mut left: Vec<_> = snapshot(&dir).into_iter().map(|(n, _)| n).collect();
        left.sort();
        assert_eq!(left, [DAMAGED, "engine.lock", "update.lock"]);
        let manifest: Value =
            serde_json::from_slice(&fs::read(done.kept.join("damage.json")).unwrap()).unwrap();
        assert_eq!(manifest["kind"], "total");
        assert_eq!(manifest["engine_version"], env!("CARGO_PKG_VERSION"));
        assert!(!manifest["reason"].as_array().unwrap().is_empty());
        let listed = manifest["files"].as_array().unwrap();
        assert_eq!(listed.len(), 3);
        for (name, bytes) in before
            .iter()
            .filter(|(n, _)| n != "engine.lock" && n != "update.lock")
        {
            assert_eq!(&fs::read(done.kept.join(name)).unwrap(), bytes);
            let entry = listed.iter().find(|f| f["name"] == name.as_str()).unwrap();
            assert_eq!(entry["size"], bytes.len());
            assert_eq!(entry["sha256"], hex::encode(Sha256::digest(bytes)));
        }
        let mut e = reopen(&dir, &state, &[DEFENDER]);
        e.audit().unwrap();
        assert!(e.history().unwrap().is_empty());
    }

    #[test]
    fn a_failed_move_puts_everything_back() {
        for after in 0..3 {
            let (dir, state) = history();
            damage(&dir);
            let before = snapshot(&dir);
            IO_FAULT.with(|f| *f.borrow_mut() = Some(("recover_move", after)));
            let error = start_fresh(dir.path(), backend(&state, &[DEFENDER], "machine-a"))
                .unwrap_err();
            assert!(format!("{error:#}").contains("Injected"), "{error:#}");
            assert_eq!(snapshot(&dir).len(), before.len() + 1);
            for (name, bytes) in &before {
                assert_eq!(&fs::read(dir.path().join(name)).unwrap(), bytes);
            }
            assert!(sets(&dir).is_empty());
            assert!(Engine::open(dir.path().into(), backend(&state, &[DEFENDER], "machine-a")).is_err());
        }
    }

    #[test]
    fn only_the_five_newest_copies_are_kept() {
        let (dir, state) = history();
        let mut first = None;
        for round in 0..7 {
            damage(&dir);
            let done = start_fresh(dir.path(), backend(&state, &[DEFENDER], "machine-a")).unwrap();
            first.get_or_insert(done.kept.clone());
            assert!(sets(&dir).len() <= 5, "round {round}");
            assert!(sets(&dir).contains(&done.kept));
        }
        assert_eq!(sets(&dir).len(), 5);
        assert!(!first.unwrap().exists());
    }

    #[test]
    fn a_held_lock_stops_starting_fresh() {
        let (dir, state) = history();
        damage(&dir);
        let held = fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(dir.path().join("engine.lock"))
            .unwrap();
        fs2::FileExt::try_lock_exclusive(&held).unwrap();
        let error = start_fresh(dir.path(), backend(&state, &[DEFENDER], "machine-a")).unwrap_err();
        assert!(format!("{error:#}").contains("holds the journal lock"));
        assert!(sets(&dir).is_empty());
    }
}
