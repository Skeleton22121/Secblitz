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
    if spec.source == Source::LegacyServices {
        // Automatic+running, already disabled, disabled but still running.
        return json!({"items": {"RemoteRegistry": 10, "sshd": 4, "WinRM": 12}});
    }
    if spec.source == Source::DefenderExclusions {
        return json!({"items": {
            "path:C:\\Users\\Bob\\Downloads": 1,
            "ext:exe": 1,
            "proc:powershell.exe": 1,
        }});
    }
    if spec.source == Source::StaleAccounts {
        // Two old accounts still on, one already switched off.
        return json!({"items": {
            "S-1-5-21-1111111111-2222222222-3333333333-1001": 1,
            "S-1-5-21-1111111111-2222222222-3333333333-1002": 1,
            "S-1-5-21-1111111111-2222222222-3333333333-1003": 0,
        }});
    }
    if spec.source == Source::ShareGrants {
        return json!({"items": {
            "Photos|S-1-1-0|Change": 1,
            "Work files|S-1-5-32-546|Full": 1,
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

// ---- system area (OS / credentials / update / privacy) -------------------

#[test]
fn legacy_services_stop_and_disable_only_what_is_unsafe_and_undo_restores_each() {
    let id = "services.legacy_remote";
    let before = json!({"items": {"RemoteRegistry": 10, "WinRM": 3, "sshd": 13, "SNMP": 4}});
    let (_dir, state, mut e) = fixture(id, before.clone());
    assert_eq!(e.audit().unwrap().results[0].status, "attention");
    e.apply_selected(&[id.into()], |_, _| {}).unwrap();
    // Manual+stopped and already-disabled services keep their state exactly.
    assert_eq!(
        state.borrow().values[id],
        json!({"items": {"RemoteRegistry": 4, "WinRM": 3, "sshd": 4, "SNMP": 4}})
    );
    assert_eq!(e.audit().unwrap().results[0].status, "compliant");
    e.revert(|_, _| {}).unwrap();
    assert_eq!(state.borrow().values[id], before);
}

#[test]
fn risky_exclusion_removal_is_recorded_and_undo_re_adds_it() {
    let id = "defender.exclusions_risky";
    let before = json!({"items": {"path:C:\\": 1, "ext:dll": 1}});
    let (_dir, state, mut e) = fixture(id, before.clone());
    e.apply_selected(&[id.into()], |_, _| {}).unwrap();
    assert_eq!(
        state.borrow().values[id],
        json!({"items": {"path:C:\\": 0, "ext:dll": 0}})
    );
    // After removal the real backend no longer lists the entries at all: that
    // still counts as the recorded safe state, so undo is not seen as drift.
    state.borrow_mut().values.insert(id.into(), json!({"items": {}}));
    assert_eq!(e.audit().unwrap().results[0].status, "compliant");
    assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "restored");
    assert_eq!(state.borrow().values[id], before);
}

#[test]
fn old_accounts_are_switched_off_never_deleted_and_undo_switches_them_back_on() {
    let id = "accounts.stale_enabled";
    let a = "S-1-5-21-1111111111-2222222222-3333333333-1001";
    let b = "S-1-5-21-1111111111-2222222222-3333333333-1002";
    let before = json!({"items": {a: 1, b: 1}});
    let (_dir, state, mut e) = fixture(id, before.clone());
    assert_eq!(e.audit().unwrap().results[0].status, "attention");
    e.apply_selected(&[id.into()], |_, _| {}).unwrap();
    assert_eq!(state.borrow().values[id], json!({"items": {a: 0, b: 0}}));
    // Switched-off accounts are no longer listed: still the recorded safe state.
    state.borrow_mut().values.insert(id.into(), json!({"items": {}}));
    assert_eq!(e.audit().unwrap().results[0].status, "compliant");
    assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "restored");
    assert_eq!(state.borrow().values[id], before);
}

#[test]
fn an_old_account_switched_on_again_by_hand_is_not_written_by_undo() {
    let id = "accounts.stale_enabled";
    let a = "S-1-5-21-1111111111-2222222222-3333333333-1001";
    let (_dir, state, mut e) = fixture(id, json!({"items": {a: 1}}));
    e.apply_selected(&[id.into()], |_, _| {}).unwrap();
    // The person switched it on again by hand: the recorded original is
    // already in place, so undo has nothing to write.
    state.borrow_mut().values.insert(id.into(), json!({"items": {a: 1}}));
    let writes = state.borrow().writes.len();
    assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "unchanged");
    assert_eq!(state.borrow().writes.len(), writes);
}

#[test]
fn broad_share_entries_are_removed_one_by_one_and_undo_adds_back_exactly_those() {
    let id = "smb.shares_exposed";
    let a = "Photos|S-1-1-0|Change";
    let b = "Work files|S-1-5-32-546|Full";
    let before = json!({"items": {a: 1, b: 1}});
    let (_dir, state, mut e) = fixture(id, before.clone());
    e.apply_selected(&[id.into()], |_, _| {}).unwrap();
    assert_eq!(state.borrow().values[id], json!({"items": {a: 0, b: 0}}));
    // A new broad entry appeared on another share meanwhile: undo restores the
    // recorded entries only and never touches the new one.
    state.borrow_mut().values.insert(
        id.into(),
        json!({"items": {"Games|S-1-1-0|Full": 1}}),
    );
    let undone = e.revert(|_, _| {}).unwrap();
    assert_eq!(undone.results[0].status, "restored");
    assert_eq!(state.borrow().writes.last().unwrap().1, before);
}

#[test]
fn update_pause_undo_restores_every_saved_time() {
    let id = "update.paused";
    let before = json!({"items": {
        "PauseUpdatesExpiryTime": 29_800_000,
        "PauseFeatureUpdatesEndTime": 29_800_000,
        "PauseQualityUpdatesEndTime": 29_800_000,
        "PauseFeatureUpdatesStartTime": 29_700_000,
        "PauseQualityUpdatesStartTime": 0,
    }});
    let (_dir, state, mut e) = fixture(id, before.clone());
    e.apply_selected(&[id.into()], |_, _| {}).unwrap();
    for k in ["PauseUpdatesExpiryTime", "PauseFeatureUpdatesStartTime", "PauseQualityUpdatesStartTime"] {
        assert_eq!(state.borrow().values[id]["items"][k], 0, "{k}");
    }
    e.revert(|_, _| {}).unwrap();
    assert_eq!(state.borrow().values[id], before);
}

#[test]
fn exploit_mitigations_only_move_switched_off_protections() {
    let id = "system.exploit_mitigations";
    // Default (not set) and on are left alone; only the explicit OFF is repaired.
    let before = json!({"items": {"DEP": 2, "SEHOP": 0, "BottomUp": 1, "HighEntropy": 0, "CFG": 2}});
    let (_dir, state, mut e) = fixture(id, before.clone());
    e.apply_selected(&[id.into()], |_, _| {}).unwrap();
    assert_eq!(
        state.borrow().values[id],
        json!({"items": {"DEP": 2, "SEHOP": 1, "BottomUp": 1, "HighEntropy": 1, "CFG": 2}})
    );
    e.revert(|_, _| {}).unwrap();
    assert_eq!(state.borrow().values[id], before);
    // The Windows default alone is never a repair request.
    let stock = json!({"items": {"DEP": 1, "SEHOP": 1, "BottomUp": 2, "HighEntropy": 1, "CFG": 1}});
    let (_dir, state, mut e) = fixture(id, stock);
    assert_eq!(e.audit().unwrap().results[0].status, "compliant");
    assert!(state.borrow().writes.is_empty());
}

#[test]
fn absent_windows_defaults_are_protected_for_the_system_controls() {
    for (id, items) in [
        ("driver.vulnerable_blocklist", json!({"VulnerableDriverBlocklistEnable": null})),
        ("ntlm.extras", json!({"NoLMHash": null, "allownullsessionfallback": null})),
        ("update.store_autoupdate_policy", json!({"AutoDownload": null})),
        ("smartscreen.apps", json!({"SmartScreenEnabled": null, "EnableSmartScreen": null})),
    ] {
        let (_dir, state, mut e) = fixture(id, json!({ "items": items }));
        assert_eq!(e.audit().unwrap().results[0].status, "compliant", "{id}");
        assert!(state.borrow().writes.is_empty(), "{id}");
    }
    // Absent is NOT protected where Windows' default leaves the exposure.
    for (id, items) in [
        ("privacy.recall", json!({"DisableAIDataAnalysis": null})),
        ("privacy.diagnostic_data_level", json!({"AllowTelemetry": null})),
        ("privacy.delivery_optimization", json!({"DODownloadMode": null})),
        ("privacy.clipboard_sync", json!({"AllowCrossDeviceClipboard": null})),
        ("printer.spooler_remote", json!({"RegisterSpoolerRemoteRpcEndPoint": null})),
    ] {
        let (_dir, _state, mut e) = fixture(id, json!({ "items": items }));
        assert_eq!(e.audit().unwrap().results[0].status, "attention", "{id}");
    }
}

#[test]
fn diagnostic_data_is_never_lowered_to_zero_and_zero_is_left_alone() {
    let id = "privacy.diagnostic_data_level";
    for full in [2, 3] {
        let (_dir, state, mut e) = fixture(id, json!({"items": {"AllowTelemetry": full}}));
        e.apply_selected(&[id.into()], |_, _| {}).unwrap();
        assert_eq!(state.borrow().values[id], json!({"items": {"AllowTelemetry": 1}}));
    }
    let (_dir, state, mut e) = fixture(id, json!({"items": {"AllowTelemetry": 0}}));
    assert_eq!(e.audit().unwrap().results[0].status, "compliant");
    assert!(state.borrow().writes.is_empty());
}
