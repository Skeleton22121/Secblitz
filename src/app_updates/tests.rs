use super::*;
use anyhow::Result;

// Historical data is compiled only into tests, never the production catalog.
const HISTORICAL: &str = include_str!("fixtures/obsolete-vscode.json");
const NOW: u64 = 1_791_000_000;

fn obsolete() -> Release {
    serde_json::from_str(HISTORICAL).unwrap()
}

fn fresh_metadata() -> Release {
    // Synthetic, unauthenticated metadata: it can satisfy a structural date
    // check, but must never be selectable or authorize any operation.
    let mut r = obsolete();
    r.version = "1.140.0".into();
    r.review = Some(CatalogReview {
        reviewed_at: NOW - 100,
        expires_at: NOW + 100,
        stable_observed_at: NOW - 10,
        stable_version_observed: r.version.clone(),
    });
    r
}

fn disabled<T>(result: Result<T>) {
    let error = match result {
        Ok(_) => panic!("Production operation unexpectedly enabled"),
        Err(error) => error,
    };
    assert!(error.downcast_ref::<Unsupported>().is_some(), "{error:#}");
    assert_eq!(error.to_string(), UNSUPPORTED_REASON);
}

fn full_consent() -> Consent {
    Consent {
        upgrade_selected_package: true,
        accept_package_license: true,
        accept_microsoft_source: true,
        acknowledge_no_automatic_rollback: true,
        disruptions: DisruptiveConsent {
            allow_tunnel_process_stop: true,
            allow_tunnel_service_reconfiguration: true,
            allow_context_menu_process_stop: true,
            allow_shell_integration_replacement: true,
            allow_previous_version_cleanup: true,
        },
    }
}

#[test]
fn production_capabilities_are_disabled_on_every_platform() {
    let c = capabilities();
    assert!(!c.supported);
    assert!(!c.native_backend);
    assert!(c.reviewed_packages.is_empty());
    let json = serde_json::to_value(c).unwrap();
    assert_eq!(json["supported"], false);
    assert_eq!(json["reviewed_packages"], serde_json::json!([]));
    assert!(!json.to_string().contains(&obsolete().installer_sha256));
}

#[test]
fn historical_target_cannot_be_planned_approved_or_started() {
    let old = obsolete();
    disabled(plan(&old.package_id, &old.version, 3600));
    let id = Uuid::from_u128(1);
    disabled(approve(id, &old.installer_sha256, full_consent()));
    disabled(start(id, &old.installer_sha256));
    // No record-loading fallback that could resurrect an old approved plan.
    disabled(get(id));
    disabled(list());
    disabled(verify(id));
}

#[test]
fn apparently_fresh_current_metadata_still_cannot_enable_production() {
    let r = fresh_metadata();
    r.validate_freshness_at(NOW).unwrap();
    disabled(plan(&r.package_id, &r.version, 600));
    disabled(approve(
        Uuid::nil(),
        "recomputed-plan-digest",
        full_consent(),
    ));
    disabled(start(Uuid::nil(), "recomputed-plan-digest"));
    disabled(discover());
}

#[test]
fn every_public_operation_is_a_synchronous_typed_rejection() {
    disabled(discover());
    disabled(plan("--all", "latest", u64::MAX));
    disabled(plan("https://attacker/installer.exe", "--force", 0));
    disabled(approve(Uuid::nil(), "", Consent::default()));
    disabled(start(Uuid::nil(), ""));
    disabled(verify(Uuid::nil()));
    disabled(get(Uuid::nil()));
    disabled(list());
}

#[test]
fn legacy_catalog_without_dated_evidence_is_never_fresh() {
    let r = obsolete();
    for at in [0, NOW, u64::MAX] {
        assert!(r.validate_freshness_at(at).is_err());
    }
}

#[test]
fn expired_catalog_and_exclusive_expiry_boundary_are_rejected() {
    let r = fresh_metadata();
    for at in [NOW + 100, NOW + 101, u64::MAX] {
        assert!(r.validate_freshness_at(at).is_err());
        disabled(plan(&r.package_id, &r.version, 3600));
    }
    r.validate_freshness_at(NOW + 99).unwrap();
}

#[test]
fn catalog_lifetime_is_capped_at_thirty_days_even_with_a_fresh_stable_check() {
    let mut r = fresh_metadata();
    let review = r.review.as_mut().unwrap();
    review.reviewed_at = NOW;
    review.stable_observed_at = NOW;
    review.expires_at = NOW + MAX_CATALOG_AGE_SECONDS;
    r.validate_freshness_at(NOW).unwrap();
    r.review.as_mut().unwrap().expires_at += 1;
    assert!(r.validate_freshness_at(NOW).is_err());
    for expiry in [0, NOW, u64::MAX] {
        r.review.as_mut().unwrap().expires_at = expiry;
        assert!(r.validate_freshness_at(NOW).is_err());
    }
}

#[test]
fn rebuilding_or_refreshing_publisher_evidence_cannot_extend_expired_review() {
    let mut r = fresh_metadata();
    r.review.as_mut().unwrap().stable_observed_at = NOW + 101;
    assert!(r.validate_freshness_at(NOW + 101).is_err());
}

#[test]
fn fresh_static_hash_does_not_prove_current_stable_or_current_security() {
    let mut r = obsolete();
    r.review = fresh_metadata().review;
    // A fresh timestamp and the historical valid hash cannot make an obsolete
    // target equal the independently observed current stable version.
    assert!(r.validate_freshness_at(NOW).is_err());
    disabled(plan(&r.package_id, &r.version, 600));
    assert!(!capabilities().supported);
}

#[test]
fn clock_rollback_future_evidence_and_stale_publisher_observation_fail_closed() {
    assert!(fresh_metadata().validate_freshness_at(NOW - 101).is_err());
    let mut r = fresh_metadata();
    r.review.as_mut().unwrap().stable_observed_at = NOW + 1;
    assert!(r.validate_freshness_at(NOW).is_err());
    r.review.as_mut().unwrap().stable_observed_at = NOW - 101;
    assert!(r.validate_freshness_at(NOW).is_err());
    let review = r.review.as_mut().unwrap();
    review.reviewed_at = NOW - MAX_STABLE_OBSERVATION_AGE_SECONDS;
    review.stable_observed_at = review.reviewed_at;
    assert!(r.validate_freshness_at(NOW).is_err());
}

#[test]
fn old_approvals_do_not_implicitly_consent_to_any_disruption() {
    let old_approval = serde_json::json!({
        "upgrade_selected_package": true,
        "accept_package_license": true,
        "accept_microsoft_source": true,
        "acknowledge_no_automatic_rollback": true
    });
    let c: Consent = serde_json::from_value(old_approval).unwrap();
    assert_eq!(c.disruptions, DisruptiveConsent::default());
    disabled(approve(Uuid::nil(), "legacy-approved-digest", c));
}

#[test]
fn each_disruptive_acknowledgement_defaults_false_and_cannot_bypass_disable() {
    let value = serde_json::to_value(full_consent()).unwrap();
    for key in [
        "allow_tunnel_process_stop",
        "allow_tunnel_service_reconfiguration",
        "allow_context_menu_process_stop",
        "allow_shell_integration_replacement",
        "allow_previous_version_cleanup",
    ] {
        for missing in [false, true] {
            let mut v = value.clone();
            if missing {
                v["disruptions"].as_object_mut().unwrap().remove(key);
            } else {
                v["disruptions"][key] = false.into();
            }
            let c: Consent = serde_json::from_value(v).unwrap();
            assert_eq!(serde_json::to_value(&c).unwrap()["disruptions"][key], false);
            disabled(approve(Uuid::nil(), "digest", c));
            disabled(start(Uuid::nil(), "digest"));
        }
    }
}

#[test]
fn no_combination_of_base_or_disruptive_consent_enables_an_operation() {
    for mask in 0u16..512 {
        let mut c = full_consent();
        c.upgrade_selected_package = mask & 1 != 0;
        c.accept_package_license = mask & 2 != 0;
        c.accept_microsoft_source = mask & 4 != 0;
        c.acknowledge_no_automatic_rollback = mask & 8 != 0;
        c.disruptions.allow_tunnel_process_stop = mask & 16 != 0;
        c.disruptions.allow_tunnel_service_reconfiguration = mask & 32 != 0;
        c.disruptions.allow_context_menu_process_stop = mask & 64 != 0;
        c.disruptions.allow_shell_integration_replacement = mask & 128 != 0;
        c.disruptions.allow_previous_version_cleanup = mask & 256 != 0;
        disabled(approve(Uuid::nil(), "any-approved-digest", c));
        disabled(start(Uuid::nil(), "any-approved-digest"));
    }
}

#[test]
fn arbitrary_installer_arguments_or_secblitz_termination_are_not_consent_fields() {
    for key in [
        "allow_secblitz_termination",
        "installer_path",
        "arguments",
        "ignore_pins",
    ] {
        let mut v = serde_json::to_value(full_consent()).unwrap();
        v["disruptions"][key] = true.into();
        assert!(serde_json::from_value::<Consent>(v).is_err());
    }
}

#[test]
fn malformed_or_duplicate_review_fields_are_rejected() {
    assert!(serde_json::from_str::<CatalogReview>(r#"{"reviewed_at":1,"reviewed_at":2,"expires_at":3,"stable_observed_at":2,"stable_version_observed":"1.140.0"}"#).is_err());
    let mut value = serde_json::to_value(fresh_metadata()).unwrap();
    value["review"]["force"] = true.into();
    assert!(serde_json::from_value::<Release>(value).is_err());
}
