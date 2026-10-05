// Included into engine::tests. Exercises every extended hardening control
// through the real engine: target, eligibility, journaling, undo, drift.
use crate::hardening::{self, Rule, Source, Spec};

/// A state with exactly the first key unsafe and every other key safe.
fn hardening_unsafe_state(spec: &Spec) -> Value {
    if spec.source == Source::FirewallExposure {
        return json!({"items": {"FPS-A": 15, "FPS-B": 12, "FPS-C": 3, "NETDIS-D": 7}});
    }
    if spec.source == Source::WifiProfiles {
        return json!({"items": {"Cafe Guest": 1, "John's Home": 0}});
    }
    if spec.source == Source::NetbiosAdapters {
        return json!({"items": {
            "{11111111-1111-1111-1111-111111111111}": 0,
            "{22222222-2222-2222-2222-222222222222}": 1,
            "{33333333-3333-3333-3333-333333333333}": 2,
        }});
    }
    let mut items = serde_json::Map::new();
    for (i, k) in spec.keys.iter().enumerate() {
        let Rule::Set {
            safe, absent_safe, ..
        } = k.rule
        else {
            unreachable!()
        };
        let value = if i == 0 {
            (0..=k.max)
                .find(|n| (k.allowed.is_empty() || k.allowed.contains(n)) && !safe.contains(n))
                .map(Value::from)
                .unwrap_or_else(|| {
                    assert!(!absent_safe);
                    Value::Null
                })
        } else {
            Value::from(safe[0])
        };
        items.insert(k.name.into(), value);
    }
    json!({ "items": items })
}

fn hardening_safe_state(spec: &Spec) -> Value {
    let unsafe_state = hardening_unsafe_state(spec);
    let mut safe = spec.derive_target(&unsafe_state).unwrap();
    // Explicit absence of a safe-by-default value must also be accepted.
    if let Some(items) = safe["items"].as_object_mut() {
        for k in spec.keys {
            if let Rule::Set {
                absent_safe: true,
                fix: None,
                ..
            } = k.rule
            {
                items.insert(k.name.into(), Value::Null);
            }
        }
    }
    safe
}

#[test]
fn every_hardening_control_audits_applies_and_undoes_exactly() {
    for spec in hardening::all() {
        let id = spec.id;
        let before = hardening_unsafe_state(spec);
        let (dir, state, mut e) = fixture(id, before.clone());
        assert_eq!(e.audit().unwrap().results[0].status, "attention", "{id}");
        assert_eq!(e.audit().unwrap().results[0].detail, "Eligible", "{id}");

        let report = e.apply_selected(&[id.into()], |_, _| {}).unwrap();
        assert_eq!(report.results[0].status, "applied", "{id}");
        let target = spec.derive_target(&before).unwrap();
        assert_eq!(state.borrow().values[id], target, "{id}");
        assert_eq!(
            state.borrow().writes,
            vec![(id.to_string(), target)],
            "{id}"
        );
        // The journal holds the exact original slice.
        let tx = e.load().unwrap().pop().unwrap();
        assert_eq!(tx.entries[0].before, before, "{id}");
        assert!(!tx.entries[0].before.to_string().contains("effective"));
        drop(tx);
        assert_eq!(e.audit().unwrap().results[0].status, "compliant", "{id}");
        // Applying again is a no-op that keeps the original before-image.
        let again = e.apply_selected(&[id.into()], |_, _| {}).unwrap();
        assert_eq!(again.results[0].status, "unchanged", "{id}");
        assert_eq!(state.borrow().writes.len(), 1);

        drop(e);
        let mut e = reopen(&dir, &state, &[id]);
        let undone = e.revert(|_, _| {}).unwrap();
        assert_eq!(undone.results[0].status, "restored", "{id}");
        assert_eq!(state.borrow().values[id], before, "{id}");
        assert_eq!(state.borrow().writes.len(), 2);
        assert!(e.history().unwrap()[0].ends_with("reverted"));
    }
}

#[test]
fn hardening_safe_and_default_states_are_protected_and_never_written() {
    for spec in hardening::all() {
        let id = spec.id;
        let safe = hardening_safe_state(spec);
        let (_dir, state, mut e) = fixture(id, safe);
        assert_eq!(e.audit().unwrap().results[0].status, "compliant", "{id}");
        let report = e.apply_selected(&[id.into()], |_, _| {}).unwrap();
        assert!(
            matches!(report.results[0].status.as_str(), "unchanged" | "skipped"),
            "{id}: {}",
            report.results[0].status
        );
        assert!(state.borrow().writes.is_empty(), "{id}");
        assert!(e.history().unwrap().is_empty(), "{id}");
    }
}

#[test]
fn managed_hardening_controls_are_left_alone_but_safe_ones_stay_protected() {
    for spec in hardening::all() {
        let id = spec.id;
        let (_dir, state, mut e) = fixture(id, hardening_unsafe_state(spec));
        state.borrow_mut().blocked = true;
        assert_eq!(e.audit().unwrap().results[0].status, "skipped", "{id}");
        let report = e.apply_selected(&[id.into()], |_, _| {}).unwrap();
        assert_eq!(report.results[0].status, "skipped", "{id}");
        assert_eq!(report.results[0].detail, "Managed device");
        assert!(state.borrow().writes.is_empty() && e.history().unwrap().is_empty());

        let (_dir, state, mut e) = fixture(id, hardening_safe_state(spec));
        state.borrow_mut().blocked = true;
        assert_eq!(e.audit().unwrap().results[0].status, "compliant", "{id}");
        assert!(state.borrow().writes.is_empty());
    }
}

#[test]
fn partially_unsafe_hardening_state_only_moves_unsafe_keys() {
    let spec = hardening::spec("printer.point_and_print").unwrap();
    let before = json!({"items": {
        "RestrictDriverInstallationToAdministrators": 1,
        "NoWarningNoElevationOnInstall": 1,
        "UpdatePromptSettings": 1,
    }});
    let (_dir, state, mut e) = fixture(spec.id, before.clone());
    e.apply_selected(&[spec.id.into()], |_, _| {}).unwrap();
    assert_eq!(
        state.borrow().values[spec.id],
        json!({"items": {
            "RestrictDriverInstallationToAdministrators": 1,
            "NoWarningNoElevationOnInstall": null,
            "UpdatePromptSettings": 1,
        }})
    );
    e.revert(|_, _| {}).unwrap();
    assert_eq!(state.borrow().values[spec.id], before);
}

#[test]
fn hardening_undo_never_overwrites_a_setting_changed_after_the_fix() {
    let id = "lsa.run_as_ppl";
    let (_dir, state, mut e) = fixture(id, json!({"items": {"RunAsPPL": null}}));
    e.apply_selected(&[id.into()], |_, _| {}).unwrap();
    assert_eq!(state.borrow().values[id], json!({"items": {"RunAsPPL": 2}}));
    // Someone upgraded it to the firmware-locked mode afterwards.
    state
        .borrow_mut()
        .values
        .insert(id.into(), json!({"items": {"RunAsPPL": 1}}));
    assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "conflict");
    assert_eq!(state.borrow().writes.len(), 1);
    // Back to our value: undo proceeds and restores absence.
    state
        .borrow_mut()
        .values
        .insert(id.into(), json!({"items": {"RunAsPPL": 2}}));
    assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "restored");
    assert_eq!(
        state.borrow().values[id],
        json!({"items": {"RunAsPPL": null}})
    );
}

#[test]
fn dynamic_hardening_undo_ignores_items_that_appeared_or_vanished() {
    let id = "net.public_sharing_exposure";
    let before = json!({"items": {"FPS-A": 15, "FPS-B": 12, "FPS-C": 3}});
    let (_dir, state, mut e) = fixture(id, before.clone());
    e.apply_selected(&[id.into()], |_, _| {}).unwrap();
    assert_eq!(
        state.borrow().values[id],
        json!({"items": {"FPS-A": 11, "FPS-B": 4, "FPS-C": 3}})
    );
    // A new rule appeared and one recorded rule was removed meanwhile.
    state.borrow_mut().values.insert(
        id.into(),
        json!({"items": {"FPS-A": 11, "FPS-C": 3, "NETDIS-NEW": 15}}),
    );
    let undone = e.revert(|_, _| {}).unwrap();
    assert_eq!(undone.results[0].status, "restored");
    // Only the surviving recorded rule was written back; the new rule is untouched.
    assert_eq!(
        state.borrow().writes.last().unwrap().1,
        json!({"items": {"FPS-A": 15, "FPS-C": 3}})
    );
    assert!(e.history().unwrap()[0].ends_with("reverted"));
}

#[test]
fn hardening_journal_images_are_strict() {
    let id = "net.llmnr";
    let (_dir, _state, mut e) = fixture(id, json!({"items": {"EnableMulticast": null}}));
    for ok in [
        json!({"items": {"EnableMulticast": null}}),
        json!({"items": {"EnableMulticast": 1}}),
    ] {
        let tx = prepare(&mut e, 1, id, ok);
        let name = tx.name.clone();
        drop(tx);
        assert!(e.load().unwrap().iter().any(|t| t.name == name));
        let path = e.dir.join(format!("{name}.jsonl"));
        std::fs::remove_file(path).unwrap();
    }
    // Duplicate keys, extra fields, wrong control domain and redundant images fail closed.
    let header = |n: u64, name: &str| {
        String::from_utf8(
            record_bytes(&Record::Header {
                schema: SCHEMA,
                machine: "machine-a".into(),
                transaction: name.into(),
                sequence: n,
            })
            .unwrap(),
        )
        .unwrap()
    };
    for (i, line) in [
        r#"{"kind":"prepare","id":"net.llmnr","before":{"items":{"EnableMulticast":1,"EnableMulticast":0}}}"#,
        r#"{"kind":"prepare","id":"net.llmnr","before":{"items":{"EnableMulticast":1},"extra":1}}"#,
        r#"{"kind":"prepare","id":"net.llmnr","before":{"items":{"Other":1}}}"#,
        r#"{"kind":"prepare","id":"net.llmnr","before":{"items":{"EnableMulticast":2}}}"#,
        r#"{"kind":"prepare","id":"net.llmnr","before":{"items":{"EnableMulticast":0}}}"#,
        r#"{"kind":"prepare","id":"net.llmnr","before":{"present":true,"value":1}}"#,
    ]
    .into_iter()
    .enumerate()
    {
        let dir = tempfile::tempdir().unwrap();
        let state = Rc::new(RefCell::new(FakeState::default()));
        state
            .borrow_mut()
            .values
            .insert(id.into(), json!({"items": {"EnableMulticast": null}}));
        let name = format!("{:020}-00000000-0000-4000-8000-00000000000{i}", 1);
        std::fs::write(
            dir.path().join(format!("{name}.jsonl")),
            format!("{}{line}\n", header(1, &name)),
        )
        .unwrap();
        assert!(
            Engine::open(dir.path().into(), backend(&state, &[id], "machine-a")).is_err(),
            "accepted journal line {line}"
        );
    }
}
