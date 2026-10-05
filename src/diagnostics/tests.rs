//! Synthetic parser/policy fixtures only. These tests do not claim native Windows
//! evidence and never invoke probes, change Windows settings or use a VM.
use super::*;
use serde_json::{json, Value};

fn k(value: impl Into<Value>) -> Value {
    json!({"state":"Known","value":value.into()})
}
fn d(id: ProbeId, value: Value) -> Diagnostic {
    parse::diagnostic(id, &serde_json::to_vec(&value).unwrap())
}
fn assessed(id: ProbeId, value: Value) -> Diagnostic {
    let mut probe = d(id, value);
    probe.assessments = rules::assess(&probe);
    probe.status = rules::aggregate(probe.assessments.iter().map(|a| a.status));
    probe
}
fn assessment<'a>(probe: &'a Diagnostic, id: &str) -> &'a Assessment {
    probe.assessments.iter().find(|a| a.rule.id == id).unwrap()
}

#[test]
fn bool_type_errors_are_unknown_not_coerced() {
    for wrong in [
        json!(0),
        json!(1),
        json!("true"),
        json!("false"),
        Value::Null,
        json!([]),
        json!({}),
    ] {
        let probe = assessed(ProbeId::SecureBoot, json!({"enabled":k(wrong)}));
        assert_eq!(probe.status, Status::Unknown);
        let Some(Evidence::SecureBoot(v)) = probe.evidence else {
            panic!()
        };
        assert_eq!(v.enabled, Reading::Unknown(UnknownReason::InvalidData));
    }
    assert_eq!(
        assessed(ProbeId::SecureBoot, json!({"enabled":k(true)})).status,
        Status::Healthy
    );
    assert_eq!(
        assessed(ProbeId::SecureBoot, json!({"enabled":k(false)})).status,
        Status::Attention
    );
}

#[test]
fn missing_null_and_corrupt_outputs_never_become_healthy() {
    for bytes in [
        b"".as_slice(),
        b"null",
        b"[]",
        b"not JSON",
        b"{",
        b"{\"enabled\": true}",
        b"{\"enabled\":null}",
    ] {
        let mut probe = parse::diagnostic(ProbeId::SecureBoot, bytes);
        if probe.evidence.is_some() {
            probe.assessments = rules::assess(&probe);
            probe.status = rules::aggregate(probe.assessments.iter().map(|a| a.status));
        }
        assert_eq!(probe.status, Status::Unknown);
    }
    for &id in ProbeId::ALL {
        let probe = assessed(id, json!({}));
        assert_eq!(probe.status, Status::Unknown, "{id:?}");
    }
}

#[test]
fn invalid_fact_does_not_erase_sibling_native_fields() {
    let probe = assessed(
        ProbeId::Tpm,
        json!({"present":k(true),"ready":k("true"),"enabled":k(false),"activated":k(true)}),
    );
    assert_eq!(assessment(&probe, "tpm.present").status, Status::Healthy);
    assert_eq!(assessment(&probe, "tpm.ready").status, Status::Unknown);
    assert_eq!(assessment(&probe, "tpm.enabled").status, Status::Attention);
}

#[test]
fn report_retains_evidence_when_other_probes_fail() {
    let report = assemble(
        Profile::Everyday,
        &Context::default(),
        vec![
            d(ProbeId::SecureBoot, json!({"enabled":k(true)})),
            unavailable(ProbeId::Tpm, UnknownReason::Timeout),
        ],
    );
    assert_eq!(report.probes.len(), ProbeId::ALL.len());
    assert_eq!(
        report
            .probes
            .iter()
            .find(|p| p.id == ProbeId::SecureBoot)
            .unwrap()
            .status,
        Status::Healthy
    );
    assert_eq!(
        report
            .probes
            .iter()
            .find(|p| p.id == ProbeId::Tpm)
            .unwrap()
            .failure,
        Some(UnknownReason::Timeout)
    );
    assert_eq!(report.coverage.probes_with_evidence, 1);
    assert_eq!(report.status, Status::Unknown);
}

#[test]
fn bounds_reject_oversized_output_arrays_and_strings() {
    assert_eq!(
        parse::decode(ProbeId::SecureBoot, &vec![b' '; MAX_OUTPUT_BYTES + 1]),
        Err(UnknownReason::OutputLimit)
    );
    assert_eq!(
        parse::decode(
            ProbeId::Vbs,
            &serde_json::to_vec(&json!({"running_services":k(vec![1;MAX_ITEMS+1])})).unwrap()
        ),
        Err(UnknownReason::OutputLimit)
    );
    assert!(parse::decode(
        ProbeId::DefenderHealth,
        &serde_json::to_vec(&json!({"running_mode":k("x".repeat(257))})).unwrap()
    )
    .is_err());
    assert!(parse::decode(
        ProbeId::DefenderHealth,
        &serde_json::to_vec(&json!({"running_mode":k("Normal\nforged") })).unwrap()
    )
    .is_err());
}

#[test]
fn unknown_fields_and_invalid_envelopes_fail_closed() {
    for bytes in [
        br#"{"enabled":{"state":"Known","value":false,"value":true}}"#.as_slice(),
        br#"{"enabled":{"state":"Known","value":false},"enabled":{"state":"Known","value":true}}"#,
    ] {
        assert_eq!(
            parse::decode(ProbeId::SecureBoot, bytes),
            Err(UnknownReason::InvalidData)
        );
    }
    assert!(parse::decode(
        ProbeId::SecureBoot,
        br#"{"enabled":{"state":"Known","value":true},"unexpected":true}"#
    )
    .is_err());
    for raw in [
        json!({"state":"Known","value":true,"extra":true}),
        json!({"state":"Healthy","value":true}),
        json!({"state":"Unknown","value":false}),
        json!({"state":"Known"}),
    ] {
        assert_eq!(
            assessed(ProbeId::SecureBoot, json!({"enabled":raw})).status,
            Status::Unknown
        );
    }
}

#[test]
fn managed_and_policy_states_are_distinct_from_health() {
    let mut value = json!({"domain_joined":k(false),"mdm_registered":k(false),"cloud_join_indicator":k(false),"defender_policy_values":k(false),"update_policy_values":k(false),"policy_manager_values":k(false)});
    assert_eq!(
        rules::management(&[d(ProbeId::Management, value.clone())]),
        ManagementStatus::NoIndicatorsObserved
    );
    value["defender_policy_values"] = k(true);
    assert_eq!(
        rules::management(&[d(ProbeId::Management, value.clone())]),
        ManagementStatus::PolicyPresent
    );
    value["mdm_registered"] = k(true);
    let report = assemble(
        Profile::HigherSecurity,
        &Context::default(),
        vec![
            d(ProbeId::Management, value),
            d(ProbeId::SecureBoot, json!({"enabled":k(false)})),
        ],
    );
    assert_eq!(report.management, ManagementStatus::Managed);
    assert!(report
        .recommendations
        .iter()
        .all(|r| r.guidance.contains("do not override policy")));
    assert!(report
        .recommendations
        .iter()
        .all(|r| r.rule.mapping_version == RULE_MAPPING_VERSION));
}

#[test]
fn failed_management_probe_is_not_unmanaged() {
    assert_eq!(rules::management(&[]), ManagementStatus::Unknown);
    assert_eq!(
        rules::management(&[d(ProbeId::Management, json!({"domain_joined":k(false)}))]),
        ManagementStatus::Unknown
    );
    assert_eq!(
        rules::management(&[d(
            ProbeId::Management,
            json!({"domain_joined":k("false"),"mdm_registered":k(false)})
        )]),
        ManagementStatus::Unknown
    );
}

#[test]
fn empty_offline_update_cache_is_not_up_to_date() {
    let p = assessed(
        ProbeId::UpdateCache,
        json!({"result_code":k(2),"missing":k(json!({"items":[],"truncated":false}))}),
    );
    assert_eq!(
        assessment(&p, "update.cached_quality").status,
        Status::Informational
    );
    // Freshness is judged from install history, never from the offline cache.
    assert!(!p
        .assessments
        .iter()
        .any(|a| a.rule.id == "update.freshness"));
    assert_eq!(p.status, Status::Informational);
}

fn event(op: u32, result: u32, age_days: u64, now: u64, hint: bool) -> UpdateEvent {
    UpdateEvent {
        operation: op,
        result_code: result,
        hresult: 0,
        date_unix_seconds: now - age_days * 86_400,
        quality_title_hint: hint,
    }
}

#[test]
fn update_freshness_is_judged_from_the_latest_successful_quality_install() {
    let now = 1_800_000_000u64;
    let fresh = [event(1, 2, 10, now, true), event(1, 2, 90, now, true)];
    assert_eq!(
        rules::update_freshness(Some(&fresh), Some(now)).0,
        Status::Healthy
    );
    let edge = [event(1, 2, 35, now, true)];
    assert_eq!(
        rules::update_freshness(Some(&edge), Some(now)).0,
        Status::Healthy
    );
    let stale = [event(1, 2, 36, now, true)];
    assert_eq!(
        rules::update_freshness(Some(&stale), Some(now)).0,
        Status::Attention
    );
    // Failed installs, uninstalls and non-quality titles never count as fresh.
    let not_fresh = [
        event(1, 4, 1, now, true),
        event(2, 2, 1, now, true),
        event(1, 2, 1, now, false),
    ];
    let (status, detail) = rules::update_freshness(Some(&not_fresh), Some(now));
    assert_eq!(status, Status::Attention);
    assert!(detail.contains("No recent security update found"));
    assert_eq!(
        rules::update_freshness(Some(&[]), Some(now)).0,
        Status::Attention
    );
    // Unreadable history or clock stays honestly unknown.
    assert_eq!(rules::update_freshness(None, Some(now)).0, Status::Unknown);
    assert_eq!(
        rules::update_freshness(Some(&fresh), None).0,
        Status::Unknown
    );
    // A clock-skewed future entry is not trusted as "fresh".
    let future = [UpdateEvent {
        date_unix_seconds: now + 30 * 86_400,
        ..event(1, 2, 0, now, true)
    }];
    assert_eq!(
        rules::update_freshness(Some(&future), Some(now)).0,
        Status::Attention
    );
}

#[test]
fn history_probe_reports_freshness_and_attention_for_no_updates() {
    let p = assessed(
        ProbeId::UpdateHistory,
        json!({"entries":k(json!({"items":[],"truncated":false}))}),
    );
    assert_eq!(assessment(&p, "update.freshness").status, Status::Attention);
    assert_eq!(p.status, Status::Attention);
}

#[test]
fn backup_coverage_reports_found_stale_and_missing_backups() {
    let now = 1_800_000_000u64;
    let ev = |age: u64| BackupEvent {
        date_unix_seconds: now - age * 86_400,
    };
    assert_eq!(
        rules::backup_coverage(Some(&[ev(3)]), Some(0), Some(now)).0,
        Status::Healthy
    );
    assert_eq!(
        rules::backup_coverage(Some(&[ev(80)]), Some(2), Some(now)).0,
        Status::Attention
    );
    let (status, detail) = rules::backup_coverage(Some(&[]), Some(4), Some(now));
    assert_eq!(status, Status::Attention);
    assert!(detail.starts_with("No backup found"));
    // Shadow copies alone are same-drive restore points, not a backup.
    assert_ne!(
        rules::backup_coverage(Some(&[]), Some(4), Some(now)).0,
        Status::Healthy
    );
    assert_eq!(
        rules::backup_coverage(None, Some(4), Some(now)).0,
        Status::Unknown
    );
}

#[test]
fn failed_or_partial_search_does_not_clear_missing_updates() {
    for code in [0, 1, 3, 4, 5, 999] {
        let p = assessed(
            ProbeId::UpdateCache,
            json!({"result_code":k(code),"missing":k(json!({"items":[],"truncated":false}))}),
        );
        assert_eq!(
            assessment(&p, "update.cached_quality").status,
            Status::Unknown
        );
    }
    let p = assessed(
        ProbeId::UpdateCache,
        json!({"result_code":k(2),"missing":k(json!({"items":[{"quality_classification":true,"kb":["1234567"]}],"truncated":false}))}),
    );
    assert_eq!(
        assessment(&p, "update.cached_quality").status,
        Status::Attention
    );
}

#[test]
fn history_failure_is_evidence_not_a_claim_of_unresolved_failure() {
    let p = assessed(
        ProbeId::UpdateHistory,
        json!({"entries":k(json!({"items":[{"operation":1,"result_code":4,"hresult":-2145124329i32,"date_unix_seconds":1700000000u64,"quality_title_hint":true}],"truncated":true}))}),
    );
    assert_eq!(
        assessment(&p, "update.failed_install").status,
        Status::Attention
    );
    assert_eq!(
        assessment(&p, "update.history_coverage").status,
        Status::Unknown
    );
    assert!(assessment(&p, "update.failed_install")
        .detail
        .contains("unresolved failure is not inferred"));
}

#[test]
fn defender_unknown_mode_and_signature_sentinels_are_unknown() {
    let p = assessed(
        ProbeId::DefenderHealth,
        json!({"running_mode":k("Unexpected"),"signatures_age_days":k(u32::MAX),"signatures_out_of_date":k(false)}),
    );
    assert_eq!(assessment(&p, "defender.mode").status, Status::Unknown);
    assert_eq!(
        assessment(&p, "defender.signatures").status,
        Status::Unknown
    );
    let passive = assessed(
        ProbeId::DefenderHealth,
        json!({"running_mode":k("Passive Mode")}),
    );
    assert_eq!(
        assessment(&passive, "defender.mode").status,
        Status::Informational
    );
}

#[test]
fn asr_and_cfa_configured_block_is_not_runtime_enforcement() {
    let p = assessed(
        ProbeId::DefenderPolicy,
        json!({"asr":k(json!({"items":[{"id":"d4f940ab-401b-4efc-aadc-ad5f3c50688a","mode":1}],"truncated":false})),"cfa_mode":k(1)}),
    );
    assert_eq!(
        assessment(&p, "asr.configured").status,
        Status::Informational
    );
    assert_eq!(
        assessment(&p, "cfa.configured").status,
        Status::Informational
    );
    assert_eq!(assessment(&p, "asr.effective").status, Status::Unknown);
    let bad = assessed(ProbeId::DefenderPolicy, json!({"cfa_mode":k(99)}));
    assert_eq!(assessment(&bad, "cfa.configured").status, Status::Unknown);
}

#[test]
fn bitlocker_suspended_protection_is_attention() {
    let p = assessed(
        ProbeId::BitLocker,
        json!({"volumes":k(json!({"items":[{"protection_status":k(0),"volume_status":k(1),"encryption_percentage":k(100)}],"truncated":false}))}),
    );
    assert_eq!(
        assessment(&p, "bitlocker.protection").status,
        Status::Attention
    );
    assert_eq!(assessment(&p, "bitlocker.recovery").status, Status::Unknown);
}

#[test]
fn vbs_configured_is_not_running() {
    let p = assessed(
        ProbeId::Vbs,
        json!({"status":k(1),"configured_services":k(vec![2]),"running_services":k(Vec::<u32>::new())}),
    );
    assert_eq!(assessment(&p, "vbs.running").status, Status::Attention);
    assert_eq!(
        assessment(&p, "vbs.memory_integrity").status,
        Status::Attention
    );
}

#[test]
fn winre_parser_is_localization_conservative_and_discards_paths() {
    let v=parse::winre(b"Windows Recovery Environment\r\n    Windows RE status: Enabled\r\nWindows RE location: private-path\r\n");
    assert_eq!(v.enabled, Reading::Known(true));
    assert_eq!(
        parse::winre(b"Windows RE status: Disabled").enabled,
        Reading::Known(false)
    );
    for text in [
        "Windows RE status: Enabled\nWindows RE status: Disabled",
        "Windows RE status: Unknown",
        "Estado de Windows RE: Enabled",
        "private-path",
    ] {
        assert!(matches!(
            parse::winre(text.as_bytes()).enabled,
            Reading::Unknown(_)
        ));
    }
    assert!(!serde_json::to_string(&v).unwrap().contains("private-path"));
}

#[test]
fn winre_accepts_redirected_utf16_but_not_corrupt_or_ambiguous_output() {
    let text = "Windows Recovery Environment\r\n  Windows RE status: Enabled\r\n";
    let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    for bytes in [
        utf16.clone(),
        [vec![0xff, 0xfe], utf16.clone()].concat(),
        [vec![0xef, 0xbb, 0xbf], text.as_bytes().to_vec()].concat(),
    ] {
        assert_eq!(parse::winre(&bytes).enabled, Reading::Known(true));
    }
    for bytes in [
        vec![0xff, 0xfe, 65],
        [utf16, vec![0, 0xd8]].concat(),
        b"Windows RE status: Enabled\n\xff".to_vec(),
    ] {
        assert_eq!(
            parse::winre(&bytes).enabled,
            Reading::Unknown(UnknownReason::InvalidData)
        );
    }
}

#[test]
fn support_mapping_does_not_guess_from_versions_or_product_names() {
    let mut app = Application {
        name: "Microsoft Silverlight".into(),
        publisher: "Microsoft Corporation".into(),
        version: "5.1.50918.0".into(),
    };
    assert!(matches!(
        support_assessment(&app),
        SupportAssessment::KnownEndOfSupport { .. }
    ));
    app.publisher = "Unknown publisher".into();
    assert_eq!(support_assessment(&app), SupportAssessment::NotAssessed);
    app.name = "Unknown application".into();
    app.version = "1.0".into();
    assert_eq!(support_assessment(&app), SupportAssessment::NotAssessed);
}

#[test]
fn backups_never_claim_verified_restore_or_data_coverage() {
    let p = assessed(
        ProbeId::Backup,
        json!({"shadow_copy_count":k(4),"success_events":k(json!({"items":[{"date_unix_seconds":1700000000u64}],"truncated":false}))}),
    );
    assert_eq!(
        assessment(&p, "backup.events").status,
        Status::Informational
    );
    // A 2023 success event is stale, so it is a calm "no recent backup" note,
    // never a verified restore or a data-coverage claim.
    assert_eq!(assessment(&p, "backup.coverage").status, Status::Attention);
    assert!(
        assessment(&p, "backup.coverage")
            .detail
            .contains("not tested")
            || assessment(&p, "backup.coverage")
                .detail
                .contains("No recent backup")
    );
    assert!(!p.assessments.iter().any(|a| a.status == Status::Healthy));
}

#[test]
fn truncated_inventory_remains_unknown_with_valid_items() {
    let p = assessed(
        ProbeId::BrowserExtensions,
        json!({"extensions":k(json!({"items":[],"truncated":true})),"profiles_examined":k(1)}),
    );
    assert_eq!(p.status, Status::Unknown);
    let p = assessed(
        ProbeId::Storage,
        json!({"disks":k(json!({"items":[{"health_status":k(0)}],"truncated":true}))}),
    );
    assert_eq!(assessment(&p, "storage.inventory").status, Status::Unknown);
    assert_eq!(
        assessment(&p, "storage.reliability").status,
        Status::Unknown
    );
}

#[test]
fn healthy_subset_does_not_hide_other_unknown_facts() {
    let probe = assessed(
        ProbeId::Storage,
        json!({"disks":k(json!({"items":[{
        "health_status":k(0),"wear_percent":k(10),"read_errors_uncorrected":k(0),"write_errors_uncorrected":k(0)
    }],"truncated":false}))}),
    );
    assert_eq!(assessment(&probe, "storage.health").status, Status::Healthy);
    assert_eq!(
        assessment(&probe, "storage.reliability").status,
        Status::Informational
    );
    assert_eq!(
        assessment(&probe, "diagnostics.evidence_completeness").status,
        Status::Unknown
    );
    assert_eq!(probe.status, Status::Unknown);
}

#[test]
fn impossible_disk_capacity_and_reserved_native_enums_are_unknown() {
    let p = assessed(
        ProbeId::Ntfs,
        json!({"volumes":k(json!({"items":[{"filesystem":k("NTFS"),"dirty":k(false),"capacity_bytes":k(2),"free_bytes":k(3)}],"truncated":false}))}),
    );
    assert_eq!(assessment(&p, "ntfs.free_space").status, Status::Unknown);
    let p = assessed(
        ProbeId::Vbs,
        json!({"status":k(99),"running_services":k(vec![2,99])}),
    );
    assert_eq!(assessment(&p, "vbs.running").status, Status::Unknown);
    assert_eq!(
        assessment(&p, "vbs.memory_integrity").status,
        Status::Unknown
    );
}

#[test]
fn profiles_do_not_change_evidence_or_silently_weaken_protection() {
    let context = Context {
        original_user: OriginalUserScope::Omit,
        compatibility: CompatibilityNeeds {
            printers: true,
            nas: true,
            vpn: true,
            games: true,
            development: true,
        },
    };
    for profile in [
        Profile::Everyday,
        Profile::Gaming,
        Profile::Development,
        Profile::HigherSecurity,
    ] {
        let report = assemble(
            profile,
            &context,
            vec![d(ProbeId::SecureBoot, json!({"enabled":k(false)}))],
        );
        assert_eq!(report.probes[0].status, Status::Attention);
        assert!(report
            .recommendations
            .iter()
            .all(|r| r.compatibility_notes.len() == 5));
        assert!(
            report
                .omissions
                .iter()
                .any(|o| o.scope == Scope::OriginalUser
                    && o.reason.contains("Elevated administrator"))
        );
    }
}

#[test]
fn serialized_report_roundtrips_without_erasing_unknowns() {
    let report = assemble(
        Profile::Everyday,
        &Context::default(),
        vec![d(
            ProbeId::Tpm,
            json!({"present":k(true),"ready":k("not boolean")}),
        )],
    );
    let text = serde_json::to_string(&report).unwrap();
    let restored: Report = serde_json::from_str(&text).unwrap();
    assert_eq!(restored.schema_version, SCHEMA_VERSION);
    assert_eq!(restored.probes[0].status, Status::Unknown);
    assert!(text.contains("InvalidData"));
    assert!(text.contains("observed_at_unix_seconds"));
    assert!(!text.contains("ComputerName"));
}

#[test]
fn scripts_preserve_read_only_and_privacy_boundaries() {
    let script = format!(
        "{}\n{}\n{}",
        include_str!("common.ps1"),
        include_str!("probes.ps1"),
        include_str!("browsers.ps1")
    );
    for forbidden in [
        "Set-MpPreference",
        "Add-MpPreference",
        "Start-MpScan",
        "Update-MpSignature",
        "Win32_Product",
        "Invoke-Expression",
        "Invoke-WebRequest",
        "Start-Process",
        "Get-BitLockerVolume | Format-List",
        "Get-Content",
        "UninstallString",
        "RecoveryPassword",
        "ServerAddress=",
        "pathToSignedProductExe",
    ] {
        assert!(!script.contains(forbidden), "{forbidden}");
    }
    assert!(script.contains("$searcher.Online = $false"));
    assert!(script.contains("Get-VpnConnection -AllUserConnection"));
    assert!(script.contains("ReparsePoint"));
    assert!(script.contains("ReadBlock"));
    let launcher = include_str!("windows.rs");
    assert!(launcher.contains("CREATE_SUSPENDED"));
    assert!(launcher.contains("CREATE_UNICODE_ENVIRONMENT"));
    assert!(launcher.contains("limits.basic.active_processes = 1"));
    assert!(!launcher.contains("std::env::var"));
    assert!(launcher.contains("elevation.TokenIsElevated != 0"));
    assert!(launcher.contains("EqualSid"));
}

#[cfg(not(windows))]
#[test]
fn unsupported_platform_never_fabricates_native_evidence() {
    let report = collect(Profile::Everyday, &Context::default());
    assert_eq!(report.status, Status::Unsupported);
    assert!(report
        .probes
        .iter()
        .all(|p| p.evidence.is_none() && p.status == Status::Unsupported));
    assert_eq!(report.coverage.probes_with_evidence, 0);
}

// ---------------------------------------------------------------------------
// 2026-10 detect-only checks: synthetic fixtures, no native evidence.
// ---------------------------------------------------------------------------

fn status_of(probe: &Diagnostic, id: &str) -> Status {
    assessment(probe, id).status
}

#[test]
fn defender_protection_threats_scans_and_exclusions() {
    let base = |threats: u32, quick: u64, risky: u32, mode: &str| {
        json!({
            "running_mode":k(mode),"tamper_protected":k(true),"tamper_feature_value":k(5),
            "active_threats":k(threats),"recent_detections":k(0),
            "quick_scan_age_days":k(quick),"full_scan_age_days":k(30),
            "exclusion_count":k(risky),"risky_exclusion_count":k(risky)
        })
    };
    let ok = assessed(ProbeId::DefenderProtection, base(0, 1, 0, "Normal"));
    assert_eq!(ok.status, Status::Healthy);
    let threats = assessed(ProbeId::DefenderProtection, base(2, 1, 0, "Normal"));
    assert_eq!(status_of(&threats, "defender.threats"), Status::Attention);
    for stale in [8u64, u32::MAX as u64] {
        let p = assessed(ProbeId::DefenderProtection, base(0, stale, 0, "Normal"));
        assert_eq!(status_of(&p, "defender.scan_age"), Status::Attention);
    }
    let risky = assessed(ProbeId::DefenderProtection, base(0, 1, 3, "Normal"));
    assert_eq!(
        status_of(&risky, "defender.exclusions_risky"),
        Status::Attention
    );
    // Passive mode means another antivirus is in charge: no false scan alarm.
    let passive = assessed(
        ProbeId::DefenderProtection,
        base(0, u32::MAX as u64, 3, "Passive Mode"),
    );
    assert_eq!(
        status_of(&passive, "defender.scan_age"),
        Status::Informational
    );
    assert_eq!(
        status_of(&passive, "defender.exclusions_risky"),
        Status::Informational
    );
}

#[test]
fn tamper_protection_prefers_the_status_property_and_falls_back_to_the_feature_value() {
    let make = |prop: Value, feature: Value| {
        assessed(
            ProbeId::DefenderProtection,
            json!({"running_mode":k("Normal"),"tamper_protected":prop,"tamper_feature_value":feature}),
        )
    };
    let id = "defender.tamper_protection";
    assert_eq!(status_of(&make(k(true), k(4)), id), Status::Healthy);
    assert_eq!(status_of(&make(k(false), k(5)), id), Status::Attention);
    let unavailable = json!({"state":"Unknown","value":"Unavailable"});
    assert_eq!(
        status_of(&make(unavailable.clone(), k(5)), id),
        Status::Healthy
    );
    assert_eq!(
        status_of(&make(unavailable.clone(), k(4)), id),
        Status::Attention
    );
    assert_eq!(status_of(&make(unavailable, k(0)), id), Status::Unknown);
}

#[test]
fn smartscreen_policy_and_smart_app_control_are_distinguished() {
    let fixture = |local: bool, policy: bool, edge: bool, sac: &str| {
        assessed(
            ProbeId::SmartScreen,
            json!({
                "apps_off_local":k(local),"apps_off_policy":k(policy),
                "edge_off_policy":k(edge),"chrome_off_policy":k(false),"smart_app_control":k(sac)
            }),
        )
    };
    let ok = fixture(false, false, false, "Absent");
    assert_eq!(status_of(&ok, "smartscreen.apps"), Status::Healthy);
    assert_eq!(
        status_of(&ok, "smartscreen.browser_policy"),
        Status::Healthy
    );
    assert_eq!(
        status_of(&ok, "smart_app_control.state"),
        Status::Informational
    );
    let apps = |p: &Diagnostic| status_of(p, "smartscreen.apps");
    assert_eq!(apps(&fixture(true, false, false, "On")), Status::Attention);
    assert_eq!(apps(&fixture(false, true, false, "On")), Status::Attention);
    assert_eq!(
        status_of(
            &fixture(false, false, true, "Off"),
            "smartscreen.browser_policy"
        ),
        Status::Attention
    );
    // Smart App Control being off is information, never an alarm.
    assert_eq!(
        status_of(
            &fixture(false, false, false, "Off"),
            "smart_app_control.state"
        ),
        Status::Informational
    );
    assert_eq!(
        status_of(
            &fixture(false, false, false, "bogus"),
            "smart_app_control.state"
        ),
        Status::Unknown
    );
}

#[test]
fn update_policy_blockers_pauses_and_overdue_restarts() {
    let fixture = |_auto: bool, paused: bool, pending: bool, uptime: u32| {
        assessed(
            ProbeId::UpdatePolicy,
            json!({
                "paused":k(paused),"drivers_excluded":k(false),"reboot_pending":k(pending),"uptime_days":k(uptime)
            }),
        )
    };
    let ok = fixture(false, false, false, 30);
    assert_eq!(ok.status, Status::Healthy);
    // Switched-off automatic updates are an engine control now: shown once, there.
    assert!(ok
        .assessments
        .iter()
        .all(|a| a.rule.id != "update.auto_policy_disabled"));
    assert_eq!(
        status_of(&fixture(false, true, false, 1), "update.paused"),
        Status::Attention
    );
    assert_eq!(
        status_of(&fixture(false, false, true, 6), "update.reboot_overdue"),
        Status::Informational
    );
    assert_eq!(
        status_of(&fixture(false, false, true, 7), "update.reboot_overdue"),
        Status::Attention
    );
    // A long uptime alone (Fast Startup) is not a pending restart.
    assert_eq!(
        status_of(&fixture(false, false, false, 90), "update.reboot_overdue"),
        Status::Healthy
    );
    // The owned update.freshness/backup.coverage findings are untouched by this probe.
    assert!(ok
        .assessments
        .iter()
        .all(|a| a.rule.id != "update.freshness"));
}

#[test]
fn hosts_file_reports_counts_and_flags_sensitive_redirects_only() {
    let fixture = |size: u64, redirects: u32, sensitive: u32, blocks: u32| {
        assessed(
            ProbeId::HostsFile,
            json!({
                "size_bytes":k(size),"redirect_count":k(redirects),
                "sensitive_redirect_count":k(sensitive),"sensitive_block_count":k(blocks)
            }),
        )
    };
    let hosts = |p: &Diagnostic| status_of(p, "net.hosts_file");
    assert_eq!(hosts(&fixture(800, 0, 0, 0)), Status::Healthy);
    assert_eq!(hosts(&fixture(800, 4, 0, 0)), Status::Informational);
    assert_eq!(hosts(&fixture(800, 4, 1, 0)), Status::Attention);
    assert_eq!(hosts(&fixture(800, 0, 0, 2)), Status::Attention);
    assert_eq!(hosts(&fixture(2_000_000, 0, 0, 0)), Status::Attention);
    let big = assessed(ProbeId::HostsFile, json!({"size_bytes":k(5_000_000u64)}));
    assert_eq!(hosts(&big), Status::Attention);
    let partial = assessed(ProbeId::HostsFile, json!({"size_bytes":k(800)}));
    assert_eq!(hosts(&partial), Status::Unknown);
}

#[test]
fn legacy_feature_persistence_accounts_sharing_and_firewall_rules() {
    let p = assessed(
        ProbeId::LegacyFeatures,
        json!({"powershell_v2_enabled":k(true)}),
    );
    assert_eq!(status_of(&p, "ps.v2_engine"), Status::Attention);
    let p = assessed(
        ProbeId::LegacyFeatures,
        json!({"powershell_v2_enabled":k(false)}),
    );
    assert_eq!(status_of(&p, "ps.v2_engine"), Status::Healthy);

    let p = assessed(
        ProbeId::Persistence,
        json!({"wmi_consumers":k(1),"unquoted_service_paths":k(3),"unquoted_service_paths_writable":k(0)}),
    );
    assert_eq!(
        status_of(&p, "persistence.wmi_subscriptions"),
        Status::Attention
    );
    assert_eq!(
        status_of(&p, "services.unquoted_paths"),
        Status::Informational
    );
    let p = assessed(
        ProbeId::Persistence,
        json!({"wmi_consumers":k(0),"unquoted_service_paths":k(3),"unquoted_service_paths_writable":k(1)}),
    );
    assert_eq!(status_of(&p, "services.unquoted_paths"), Status::Attention);
    assert_eq!(
        status_of(&p, "persistence.wmi_subscriptions"),
        Status::Healthy
    );

    let p = assessed(
        ProbeId::AccountHygiene,
        json!({"stale_enabled_accounts":k(2)}),
    );
    assert_eq!(status_of(&p, "accounts.stale_enabled"), Status::Attention);
    assert!(p
        .assessments
        .iter()
        .all(|a| a.rule.id != "accounts.builtin_administrator"));
    let p = assessed(
        ProbeId::AccountHygiene,
        json!({"stale_enabled_accounts":k(0)}),
    );
    assert_eq!(p.status, Status::Healthy);

    let p = assessed(
        ProbeId::Sharing,
        json!({"share_count":k(2),"broad_access_shares":k(1),"encrypt_data":k(false)}),
    );
    assert_eq!(status_of(&p, "smb.shares_exposed"), Status::Attention);
    assert_eq!(
        status_of(&p, "smb.server_encryption"),
        Status::Informational
    );
    let p = assessed(
        ProbeId::Sharing,
        json!({"share_count":k(0),"broad_access_shares":k(0),"encrypt_data":k(false)}),
    );
    assert_eq!(p.status, Status::Healthy);

    let rules = |risky: u32| {
        assessed(
            ProbeId::FirewallRules,
            json!({"risky_inbound_allow_rules":k(risky),"user_folder_inbound_allow_rules":k(4)}),
        )
    };
    let id = "firewall.user_dir_inbound_allow";
    assert_eq!(status_of(&rules(1), id), Status::Attention);
    assert_eq!(status_of(&rules(0), id), Status::Informational);
}

#[test]
fn os_support_and_secure_boot_certificate_probes_parse_end_to_end() {
    let p = assessed(
        ProbeId::OsSupport,
        json!({"display_version":k("24H2"),"build":k(26100),"edition_id":k("Core")}),
    );
    assert!(matches!(
        p.status,
        Status::Attention | Status::Healthy | Status::Informational
    ));
    assert_eq!(
        assessment(&p, "os.feature_release_support").rule.id,
        "os.feature_release_support"
    );
    let p = assessed(
        ProbeId::SecureBootCerts,
        json!({
            "update_completed_event":k(true),"update_staged_event":k(false),"update_error_event":k(false),
            "servicing_status":k("Updated"),"ca2023_in_db":k(true),"secure_boot_enabled":k(true)
        }),
    );
    assert_eq!(p.status, Status::Healthy);
    // A bad servicing type is unknown, not a guess.
    let p = assessed(ProbeId::SecureBootCerts, json!({"servicing_status":k(7)}));
    assert_eq!(p.status, Status::Unknown);
}

#[test]
fn new_probes_have_compiled_branches_and_read_only_privacy_boundaries() {
    let script = format!(
        "{}\n{}",
        include_str!("common.ps1"),
        include_str!("probes.ps1")
    );
    // Fixed Windows tools and the WLAN API run natively, not through PowerShell.
    let native = [ProbeId::WindowsHello, ProbeId::WifiSecurity];
    for &id in &ProbeId::ALL[23..] {
        assert_eq!(
            script.contains(&format!("'{id:?}' {{")),
            !native.contains(&id),
            "{id:?} compiled branch"
        );
    }
    // Hosts entries, exclusion lists, share names and rule programs never reach output.
    for forbidden in [
        "Get-Content",
        "Set-ItemProperty",
        "Remove-Item",
        "Set-NetFirewallRule",
        "Disable-WindowsOptionalFeature",
        "Set-SmbShare",
        "Stop-Service",
        "Set-Service",
        "Set-Content",
        "Out-File",
        "New-Item",
        "New-ItemProperty",
        "Set-LocalUser",
        "Disable-LocalUser",
        "Add-LocalGroupMember",
        "Remove-LocalGroupMember",
        "Set-DnsClient",
        "Add-DnsClientDohServerAddress",
        "Remove-DnsClientDohServerAddress",
        "Disable-ScheduledTask",
        "Unregister-ScheduledTask",
        "Register-ScheduledTask",
        "Start-ScheduledTask",
    ] {
        assert!(!script.contains(forbidden), "{forbidden}");
    }
    assert!(script.contains("Exclusion paths, extensions and process names are never emitted"));
    assert!(script.contains("message text can carry firmware"));
}

#[test]
fn every_new_probe_has_a_launcher_module_entry_and_unique_source() {
    let launcher = include_str!("windows.rs");
    for id in [
        "DefenderProtection",
        "SecureBootCerts",
        "UpdatePolicy",
        "Persistence",
        "LegacyFeatures",
        "AccountHygiene",
        "Sharing",
        "FirewallRules",
        "AccountSetup",
        "WindowsHello",
        "DnsEncryption",
        "WifiSecurity",
        "Autostart",
    ] {
        assert!(launcher.contains(&format!("ProbeId::{id}")), "{id}");
    }
    // The pinned module list matches what each branch loads.
    let script = include_str!("probes.ps1");
    for (probe, modules) in [
        (
            "Autostart",
            vec![
                "CimCmdlets",
                "ScheduledTasks",
                "Microsoft.PowerShell.Security",
            ],
        ),
        (
            "AccountSetup",
            vec!["Microsoft.PowerShell.LocalAccounts", "CimCmdlets"],
        ),
        ("DnsEncryption", vec!["DnsClient"]),
    ] {
        let branch = script.split(&format!("'{probe}' {{")).nth(1).unwrap();
        let branch = &branch[..branch.find("\n        '").unwrap_or(branch.len())];
        for module in modules {
            assert!(
                branch.contains(&format!("Load '{module}'")),
                "{probe} {module}"
            );
            assert!(launcher.contains(&format!("\"{module}\"")), "{module}");
        }
    }
    let mut sources: Vec<_> = ProbeId::ALL.iter().map(|id| id.source()).collect();
    sources.sort_unstable();
    sources.dedup();
    assert_eq!(sources.len(), ProbeId::ALL.len());
}

// ---------------------------------------------------------------------------
// Sign-in, encryption, network and start-up checks.
// ---------------------------------------------------------------------------

#[test]
fn account_setup_daily_admin_and_find_my_device() {
    let fixture = |admin: Value, find: Value| {
        assessed(
            ProbeId::AccountSetup,
            json!({"current_user_is_admin":admin,"find_my_device":find}),
        )
    };
    let unknown = json!({"state":"Unknown","value":"Unavailable"});
    let p = fixture(k(true), k("Off"));
    assert_eq!(status_of(&p, "accounts.daily_admin"), Status::Attention);
    assert_eq!(status_of(&p, "accounts.find_my_device"), Status::Attention);
    let p = fixture(k(false), k("On"));
    assert_eq!(p.status, Status::Healthy);
    // A desktop, a local-only account or an unreported setting is never an alarm.
    for neutral in ["NotApplicable", "Unreported"] {
        let p = fixture(k(false), k(neutral));
        assert_eq!(
            status_of(&p, "accounts.find_my_device"),
            Status::Informational
        );
    }
    // Nested-group doubt and a bogus value stay unknown, never "standard account".
    let p = fixture(unknown, k("maybe"));
    assert_eq!(status_of(&p, "accounts.daily_admin"), Status::Unknown);
    assert_eq!(status_of(&p, "accounts.find_my_device"), Status::Unknown);
    assert_eq!(p.status, Status::Unknown);
}

#[test]
fn dsregcmd_output_yields_only_the_ngc_flag() {
    let make = |text: &str| parse::dsreg(text.as_bytes()).pin_set;
    assert_eq!(
        make("| User State |\n\n                    NgcSet : YES\n  WamDefaultSet : NO"),
        Reading::Known(true)
    );
    assert_eq!(make("   NgcSet : NO\r\n"), Reading::Known(false));
    // Missing, duplicated or unexpected values are unknown, never "no PIN".
    for bad in [
        "",
        "no such line",
        "NgcSet : MAYBE",
        "NgcSet : YES\nNgcSet : NO",
        "NgcSet",
    ] {
        assert!(make(bad).known().is_none(), "{bad:?}");
    }
    let hello = assessed(ProbeId::WindowsHello, json!({"pin_set":k(false)}));
    assert_eq!(
        status_of(&hello, "accounts.hello_configured"),
        Status::Attention
    );
    let hello = assessed(ProbeId::WindowsHello, json!({"pin_set":k(true)}));
    assert_eq!(hello.status, Status::Healthy);
}

#[test]
fn kernel_stack_protection_is_a_tip_and_skipped_when_not_reported() {
    let fixture = |stacks: &str, running: Vec<u32>| {
        assessed(
            ProbeId::Vbs,
            json!({"status":k(2),"configured_services":k(running.clone()),"running_services":k(running),"kernel_shadow_stacks":k(stacks)}),
        )
    };
    let id = "vbs.kernel_stack_protection";
    assert_eq!(status_of(&fixture("On", vec![2]), id), Status::Healthy);
    assert_eq!(status_of(&fixture("Off", vec![2]), id), Status::Attention);
    assert_eq!(
        status_of(&fixture("Off", vec![1]), id),
        Status::Informational
    );
    // Not reported by this Windows build: no assessment at all, no change to the probe.
    let absent = fixture("Absent", vec![2]);
    assert!(absent.assessments.iter().all(|a| a.rule.id != id));
    assert_eq!(status_of(&fixture("bogus", vec![2]), id), Status::Unknown);
}

#[test]
fn dns_encryption_is_informational_unless_a_capable_provider_is_unencrypted() {
    let fixture = |total: u32, encrypted: u32, upgradeable: u32| {
        assessed(
            ProbeId::DnsEncryption,
            json!({"dns_servers":k(total),"encrypted_dns_servers":k(encrypted),"upgradeable_dns_servers":k(upgradeable)}),
        )
    };
    let id = "net.dns_encryption";
    assert_eq!(status_of(&fixture(2, 2, 0), id), Status::Healthy);
    assert_eq!(status_of(&fixture(2, 0, 2), id), Status::Attention);
    // Router-provided DNS: nothing to nag about, and DNS is never changed.
    assert_eq!(status_of(&fixture(1, 0, 0), id), Status::Informational);
    assert_eq!(status_of(&fixture(2, 1, 1), id), Status::Informational);
    assert_eq!(status_of(&fixture(0, 0, 0), id), Status::Unknown);
}

#[test]
fn wifi_security_classes_and_assessments() {
    use parse::wifi_class;
    assert_eq!(wifi_class(false, 1, 0), "Open");
    assert_eq!(wifi_class(true, 1, 1), "Wep");
    assert_eq!(wifi_class(true, 2, 1), "Wep");
    assert_eq!(wifi_class(true, 4, 2), "Old"); // WPA-PSK
    assert_eq!(wifi_class(true, 7, 2), "Old"); // WPA2 with TKIP
    assert_eq!(wifi_class(true, 7, 4), "Strong");
    assert_eq!(wifi_class(true, 9, 4), "Strong");
    assert_eq!(wifi_class(true, 7, 0x100), "Other");
    assert_eq!(wifi_class(true, 99, 4), "Other");
    for (class, status) in [
        ("Strong", Status::Healthy),
        ("None", Status::Informational),
        ("Open", Status::Attention),
        ("Wep", Status::Attention),
        ("Old", Status::Attention),
        ("Other", Status::Unknown),
        ("bogus", Status::Unknown),
    ] {
        let p = assessed(ProbeId::WifiSecurity, json!({"current_network":k(class)}));
        assert_eq!(status_of(&p, "net.wifi_security"), status, "{class}");
    }
}

#[test]
fn autostart_counts_flag_risky_entries_and_need_complete_evidence() {
    let fixture = |checked: Value, risky: u32, command: u32| {
        assessed(
            ProbeId::Autostart,
            json!({"entries_checked":checked,"risky_unsigned":k(risky),"suspicious_command":k(command)}),
        )
    };
    let id = "persistence.run_and_tasks";
    assert_eq!(status_of(&fixture(k(12), 0, 0), id), Status::Healthy);
    assert_eq!(status_of(&fixture(k(12), 1, 0), id), Status::Attention);
    assert_eq!(status_of(&fixture(k(12), 0, 2), id), Status::Attention);
    // A source that could not be read cannot make the result "clean".
    let unreadable = json!({"state":"Unknown","value":"Unavailable"});
    assert_eq!(
        status_of(&fixture(unreadable.clone(), 0, 0), id),
        Status::Unknown
    );
    assert_eq!(status_of(&fixture(unreadable, 1, 0), id), Status::Attention);
}

#[test]
fn engine_owned_checks_are_not_duplicated_here() {
    let script = include_str!("probes.ps1");
    for removed in [
        "auto_updates_blocked",
        "update_access_blocked",
        "update_service_disabled",
        "builtin_admin_enabled",
    ] {
        assert!(!script.contains(removed), "{removed}");
    }
    let checks = include_str!("checks.rs");
    let code = checks.split("#[cfg(test)]").next().unwrap();
    for duplicate in [
        "a(\"update.auto_policy_disabled\"",
        "boolean(\"accounts.builtin_administrator\"",
    ] {
        assert!(!code.contains(duplicate), "{duplicate}");
    }
}
