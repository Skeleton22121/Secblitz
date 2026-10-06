// Included inside engine::tests. Each batch owns a different control: the journal allows one active owner per control.
mod revert_all {
    use super::*;

    const X: &str = "firewall.public.inbound";
    const Y: &str = "firewall.private.inbound";
    const Z: &str = "firewall.domain.inbound";

    fn three_controls() -> (TempDir, Rc<RefCell<FakeState>>, Engine) {
        let (dir, state, e) = fixture(X, json!("Allow"));
        drop(e);
        for id in [Y, Z] {
            state.borrow_mut().values.insert(id.into(), json!("Allow"));
        }
        let e = reopen(&dir, &state, &[X, Y, Z]);
        (dir, state, e)
    }

    fn apply_one(e: &mut Engine, id: &str) -> String {
        let report = e.apply_selected(&[id.into()], |_, _| {}).unwrap();
        report.transaction.expect("a batch was recorded")
    }

    fn set(state: &Rc<RefCell<FakeState>>, id: &str, value: Value) {
        state.borrow_mut().values.insert(id.into(), value);
    }

    #[test]
    fn revert_all_restores_every_batch_newest_first() {
        let (_dir, state, mut e) = three_controls();
        for id in [X, Y, Z] {
            apply_one(&mut e, id);
        }
        let mut seen = Vec::new();
        let report = e
            .revert_all(|id, status| seen.push((id.to_owned(), status.to_owned())))
            .unwrap();
        assert!(report.transaction.is_none());
        for id in [X, Y, Z] {
            assert_eq!(state.borrow().values[id], json!("Allow"));
        }
        assert!(e.load().unwrap().iter().all(|t| t.reverted));
        let restored = |id: &str| (id.to_owned(), "restored".to_owned());
        assert_eq!(seen, vec![restored(Z), restored(Y), restored(X)]);
        assert_eq!(report.results.len(), 3);
        assert_eq!(report.results[0].id, Z);
        assert!(report.results.iter().all(|r| r.status == "restored"));
    }

    #[test]
    fn revert_all_continues_past_conflict() {
        let (_dir, state, mut e) = three_controls();
        apply_one(&mut e, X);
        let middle = apply_one(&mut e, Y);
        let newest = apply_one(&mut e, Z);
        set(&state, Y, json!("NotConfigured"));
        let report = e.revert_all(|_, _| {}).unwrap();
        let statuses: Vec<_> = report
            .results
            .iter()
            .map(|r| (r.id.as_str(), r.status.as_str()))
            .collect();
        assert_eq!(
            statuses,
            vec![(Z, "restored"), (Y, "conflict"), (X, "restored")]
        );
        let txs = e.load().unwrap();
        let reverted = |n: &String| txs.iter().find(|t| &t.name == n).unwrap().reverted;
        assert!(reverted(&newest));
        assert!(!reverted(&middle));
        assert_eq!(txs.iter().filter(|t| !t.reverted).count(), 1);
        let expected = Engine::journal_finding(txs.iter().find(|t| t.name == middle).unwrap());
        assert!(report.findings.iter().any(|f| f.detail == expected.detail));
        assert_eq!(state.borrow().values[X], json!("Allow"));
        assert_eq!(state.borrow().values[Y], json!("NotConfigured"));
        assert_eq!(state.borrow().values[Z], json!("Allow"));
    }

    #[test]
    fn revert_all_never_strands_an_older_batch_behind_a_left_over_one() {
        let (_dir, state, mut e) = three_controls();
        apply_one(&mut e, X);
        apply_one(&mut e, Y);
        apply_one(&mut e, Z);
        // Two newer batches conflict. Only the newest may stay half undone, or
        // the journal would be rejected on the next load.
        set(&state, Y, json!("NotConfigured"));
        set(&state, Z, json!("NotConfigured"));
        let report = e.revert_all(|_, _| {}).unwrap();
        let statuses: Vec<_> = report
            .results
            .iter()
            .map(|r| (r.id.as_str(), r.status.as_str()))
            .collect();
        assert_eq!(
            statuses,
            vec![(Z, "conflict"), (Y, "conflict"), (X, "restored")]
        );
        assert_eq!(report.findings.len(), 2);
        set(&state, Y, json!("Block"));
        set(&state, Z, json!("Block"));
        let report = e.revert_all(|_, _| {}).unwrap();
        assert!(report.results.iter().all(|r| r.status == "restored"));
        assert!(e.load().unwrap().iter().all(|t| t.reverted));
        assert_eq!(e.undoable_changes().unwrap(), 0);
    }

    #[test]
    fn revert_all_reports_skipped() {
        let (_dir, state, mut e) = three_controls();
        apply_one(&mut e, X);
        apply_one(&mut e, Y);
        state.borrow_mut().blocked = true;
        let report = e.revert_all(|_, _| {}).unwrap();
        assert_eq!(report.results.len(), 2);
        assert!(report.results.iter().all(|r| r.status == "skipped"));
        assert!(state.borrow().writes.iter().all(|(_, v)| v == "Block"));
        state.borrow_mut().blocked = false;
        let report = e.revert_all(|_, _| {}).unwrap();
        assert!(report.results.iter().all(|r| r.status == "restored"));
        assert_eq!(state.borrow().values[X], json!("Allow"));
        assert_eq!(state.borrow().values[Y], json!("Allow"));
    }

    #[test]
    fn revert_all_recovers_interrupted_entry() {
        let (dir, state, mut e) = fixture(DEFENDER, json!(true));
        apply_one(&mut e, DEFENDER);
        // Crash after the restore wrote but before Restored reached the journal.
        let mut tx = e.load().unwrap().pop().unwrap();
        e.append(&mut tx, Record::Reverting).unwrap();
        e.append(
            &mut tx,
            Record::RestorePending {
                id: DEFENDER.into(),
            },
        )
        .unwrap();
        set(&state, DEFENDER, json!(true));
        drop(tx);
        drop(e);
        let mut e = reopen(&dir, &state, &[DEFENDER]);
        let report = e.revert_all(|_, _| {}).unwrap();
        assert_eq!(report.results[0].status, "unchanged");
        assert!(e.load().unwrap()[0].reverted);
        assert_eq!(state.borrow().values[DEFENDER], json!(true));
    }

    #[test]
    fn undoable_changes_counts_distinct_ids() {
        let (_dir, state, mut e) = three_controls();
        assert_eq!(e.undoable_changes().unwrap(), 0);
        apply_one(&mut e, X);
        apply_one(&mut e, Y);
        assert_eq!(e.undoable_changes().unwrap(), 2);
        apply_one(&mut e, Z);
        let writes = state.borrow().writes.len();
        assert_eq!(e.undoable_changes().unwrap(), 3);
        assert_eq!(state.borrow().writes.len(), writes);
        e.revert_all(|_, _| {}).unwrap();
        assert_eq!(e.undoable_changes().unwrap(), 0);
    }
}
