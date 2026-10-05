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
    assert_eq!(assessment(&p, "update.freshness").status, Status::Unknown);
    assert_eq!(p.status, Status::Unknown);
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
    assert_eq!(assessment(&p, "backup.coverage").status, Status::Unknown);
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
