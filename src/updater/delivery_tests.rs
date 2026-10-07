use super::*;
use ed25519_dalek::{Signer, SigningKey};

fn envelope(value: &serde_json::Value, key: &SigningKey) -> Vec<u8> {
    let raw = serde_json::to_vec(value).unwrap();
    serde_json::to_vec(&serde_json::json!({"payload": STANDARD.encode(&raw),
        "signature": STANDARD.encode(key.sign(&raw).to_bytes())}))
    .unwrap()
}
fn fixture() -> (SigningKey, SigningKey, Vec<u8>, serde_json::Value) {
    let root = SigningKey::from_bytes(&[51; 32]);
    let delegate = SigningKey::from_bytes(&[52; 32]);
    let manifest = serde_json::json!({"schema":1,"version":"9.0.0",
        "filename":setup_filename("9.0.0"), "sha256":hex::encode(Sha256::digest(b"test")),
        "size":4,"published_at":1000,"expires_at":2000,"target":arch::TARGET});
    let raw = envelope(&manifest, &delegate);
    let auth = serde_json::json!({"schema":1,"role":"secblitz-delivery","sequence":1,
        "origin":"https://updates.example.org/","target":arch::TARGET,
        "published_at":1000,"expires_at":2000,"version":"9.0.0",
        "manifest_sha256":hex::encode(Sha256::digest(&raw)),
        "manifest_key":hex::encode(delegate.verifying_key().to_bytes()),
        "rollout":{"salt":"ab".repeat(32),"basis_points":5000},"health":"update_health_v1"});
    (root, delegate, raw, auth)
}
fn decode(value: &serde_json::Value, key: &SigningKey) -> Result<delivery::Authorization> {
    delivery::decode(
        &envelope(value, key),
        &key.verifying_key().to_bytes(),
        &origin("https://updates.example.org").unwrap().unwrap(),
    )
}

#[test]
fn root_authorizes_only_one_key_one_exact_candidate_and_one_origin() {
    let (root, delegate, raw, value) = fixture();
    let a = decode(&value, &root).unwrap();
    delivery::candidate(&raw, &a, 1500).unwrap();
    // No delegate may sign its own delegation or a root replacement.
    assert!(delivery::decode(
        &envelope(&value, &delegate),
        &root.verifying_key().to_bytes(),
        &origin("https://updates.example.org").unwrap().unwrap()
    )
    .is_err());
    assert!(delivery::decode(
        &envelope(&value, &root),
        &root.verifying_key().to_bytes(),
        &origin("https://other.example.org").unwrap().unwrap()
    )
    .is_err());
    assert!(verify(&raw, &root.verifying_key().to_bytes(), 1500).is_err());
    assert!(verify(
        &envelope(&value, &root),
        &root.verifying_key().to_bytes(),
        1500
    )
    .is_err());
    let mut padded = raw.clone();
    padded.push(b' ');
    assert!(delivery::candidate(&padded, &a, 1500).is_err());
    let mut wrong_key = a.clone();
    wrong_key.manifest_key = hex::encode(root.verifying_key().to_bytes());
    assert!(delivery::candidate(&raw, &wrong_key, 1500).is_err());
    let mut wrong_version = a.clone();
    wrong_version.version = "9.1.0".into();
    assert!(delivery::candidate(&raw, &wrong_version, 1500).is_err());
}

#[test]
fn delegation_scope_is_strict_bounded_and_weak_keys_are_rejected() {
    let (root, _, _, value) = fixture();
    for (field, bad) in [
        ("schema", serde_json::json!(2)),
        ("role", serde_json::json!("root")),
        ("sequence", serde_json::json!(0)),
        ("sequence", serde_json::json!(-1)),
        ("sequence", serde_json::json!(1.0)),
        ("sequence", serde_json::json!(true)),
        ("expires_at", serde_json::json!(1000)),
        (
            "expires_at",
            serde_json::json!(1000 + delivery::LIFETIME + 1),
        ),
        ("target", serde_json::json!(arch::OTHER_TARGET)),
        ("manifest_key", serde_json::json!("00".repeat(32))),
        (
            "manifest_key",
            serde_json::json!(format!("01{}", "00".repeat(31))),
        ),
        ("manifest_key", serde_json::json!("A".repeat(64))),
        ("manifest_sha256", serde_json::json!("a".repeat(63))),
        ("health", serde_json::json!("cmd.exe /c whoami")),
        ("health", serde_json::json!("version_only")),
        ("threshold", serde_json::json!(0)),
        ("revoked_keys", serde_json::json!([])),
        ("origin", serde_json::json!("https://updates.example.org")),
        ("version", serde_json::json!("9.0.0+build")),
    ] {
        let mut bad_value = value.clone();
        bad_value[field] = bad;
        assert!(decode(&bad_value, &root).is_err(), "{field}");
    }
    for (field, bad) in [
        ("salt", serde_json::json!("x")),
        ("basis_points", serde_json::json!(10001)),
        ("basis_points", serde_json::json!(-1)),
        ("basis_points", serde_json::json!(1.0)),
        ("extra", serde_json::json!(true)),
    ] {
        let mut v = value.clone();
        v["rollout"][field] = bad;
        assert!(decode(&v, &root).is_err());
    }
    let raw = serde_json::to_string(&value).unwrap();
    for (field, item) in value.as_object().unwrap() {
        let duplicate = format!("{{\"{field}\":{item},{}", raw.trim_start_matches('{'));
        let bytes = serde_json::to_vec(
            &serde_json::json!({"payload":STANDARD.encode(duplicate.as_bytes()),
            "signature":STANDARD.encode(root.sign(duplicate.as_bytes()).to_bytes())}),
        )
        .unwrap();
        assert!(delivery::decode(
            &bytes,
            &root.verifying_key().to_bytes(),
            &origin("https://updates.example.org").unwrap().unwrap()
        )
        .is_err());
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(decode(&missing, &root).is_err());
    }
}

#[test]
fn delegation_expiry_is_hard_and_durable_floor_does_not_expire() {
    let (root, _, raw, mut value) = fixture();
    let a = decode(&value, &root).unwrap();
    delivery::fresh(&a, 400).unwrap();
    assert!(delivery::fresh(&a, 399).is_err());
    delivery::candidate(&raw, &a, 1999).unwrap();
    for time in [2000, 2001, u64::MAX] {
        assert!(delivery::candidate(&raw, &a, time).is_err());
    }
    let restored = decode(&value, &root).unwrap();
    value["sequence"] = 2.into();
    value["published_at"] = 3000.into();
    value["expires_at"] = 4000.into();
    let renewed = decode(&value, &root).unwrap();
    delivery::advance(&renewed, Some(&restored)).unwrap();
    assert!(delivery::advance(&restored, Some(&renewed)).is_err());
    value["published_at"] = (u64::MAX - 1).into();
    value["expires_at"] = u64::MAX.into();
    let edge = decode(&value, &root).unwrap();
    delivery::fresh(&edge, u64::MAX - 1).unwrap();
    assert!(delivery::fresh(&edge, u64::MAX).is_err());
}

#[test]
fn key_replacement_and_policy_withdrawal_never_union_old_authority() {
    let (root, _, raw, mut value) = fixture();
    let previous = decode(&value, &root).unwrap();
    delivery::advance(&previous, Some(&previous)).unwrap();
    value["rollout"]["basis_points"] = 0.into();
    let equivocation = decode(&value, &root).unwrap();
    assert!(delivery::advance(&equivocation, Some(&previous)).is_err());
    value["sequence"] = 2.into();
    value["manifest_key"] =
        hex::encode(SigningKey::from_bytes(&[53; 32]).verifying_key().to_bytes()).into();
    value["expires_at"] = 1600.into(); // Root can shorten expiry on a new sequence.
    let replacement = decode(&value, &root).unwrap();
    delivery::advance(&replacement, Some(&previous)).unwrap();
    assert!(delivery::candidate(&raw, &replacement, 1500).is_err());
    assert!(delivery::advance(&previous, Some(&replacement)).is_err());
    for (field, bad) in [
        ("published_at", serde_json::json!(999)),
        ("version", serde_json::json!("8.0.0")),
    ] {
        let mut rollback = value.clone();
        rollback[field] = bad;
        assert!(delivery::advance(&decode(&rollback, &root).unwrap(), Some(&previous)).is_err());
    }
    value["rollout"]["salt"] = "cd".repeat(32).into();
    assert!(delivery::advance(&decode(&value, &root).unwrap(), Some(&previous)).is_err());
}

#[test]
fn cohort_is_stable_nested_and_has_exact_zero_and_full_boundaries() {
    let (root, _, _, value) = fixture();
    let mut a = decode(&value, &root).unwrap();
    // A golden vector locks byte ordering, domain separation and modulo.
    assert_eq!(delivery::cohort(&[7; 16], &a.rollout).unwrap(), 9219);
    for id in 0u32..1000 {
        let mut device = [0u8; 16];
        device[..4].copy_from_slice(&id.to_be_bytes());
        let bucket = delivery::cohort(&device, &a.rollout).unwrap();
        assert!(bucket < 10000);
        for points in [0, bucket, bucket + 1, 10000] {
            a.rollout.basis_points = points;
            assert_eq!(delivery::eligible(&device, &a).unwrap(), bucket < points);
            assert_eq!(delivery::cohort(&device, &a.rollout).unwrap(), bucket);
        }
    }
}

#[test]
fn holdback_observes_floor_and_failed_health_cannot_enable_downgrade() {
    let (root, _, raw, mut value) = fixture();
    value["rollout"]["basis_points"] = 0.into();
    let a = decode(&value, &root).unwrap();
    let m = delivery::candidate(&raw, &a, 1500).unwrap();
    let floor = advance_floor(&m, "8.0.0", None).unwrap();
    assert!(!delivery::eligible(&[7; 16], &a).unwrap());
    assert!(health::validate(b"{}", "9.0.0", None).is_err());
    let old = Manifest {
        version: "8.5.0".into(),
        filename: setup_filename("8.5.0").into(),
        ..m
    };
    assert!(advance_floor(&old, "8.0.0", Some(&floor)).is_err());
    assert!(advance_floor(&old, "9.0.0", None).is_err());
}

#[test]
fn health_requires_exact_report_version_and_preserved_optional_components() {
    let before = UpdateHealth {
        schema: 1,
        version: "8.0.0".into(),
        task: TaskHealth::Ready,
        monitor: MonitorHealth::Running,
    };
    let good = serde_json::json!({"schema":1,"version":"9.0.0","task":"ready","monitor":"running"});
    health::validate(&serde_json::to_vec(&good).unwrap(), "9.0.0", Some(&before)).unwrap();
    for (field, bad) in [
        ("schema", serde_json::json!(2)),
        ("version", serde_json::json!("8.0.0")),
        ("version", serde_json::json!("9.0.0+build")),
        ("task", serde_json::json!("absent")),
        ("task", serde_json::json!("unknown")),
        ("monitor", serde_json::json!("stopped")),
        ("monitor", serde_json::json!("start_pending")),
        ("healthy", serde_json::json!(true)),
    ] {
        let mut v = good.clone();
        v[field] = bad;
        assert!(
            health::validate(&serde_json::to_vec(&v).unwrap(), "9.0.0", Some(&before)).is_err()
        );
    }
    for raw in [
        b"{}".as_slice(),
        b"null",
        b"secblitz 9.0.0",
        b"{}{}",
        b"\xff",
    ] {
        assert!(health::validate(raw, "9.0.0", None).is_err());
    }
    let duplicate = b"{\"schema\":1,\"schema\":1,\"version\":\"9.0.0\",\"task\":\"ready\",\"monitor\":\"running\"}";
    assert!(health::validate(duplicate, "9.0.0", None).is_err());
    assert!(health::validate(&vec![b' '; health::LIMIT + 1], "9.0.0", None).is_err());
    assert_eq!(
        delivery::HealthCommand::UpdateHealthV1.args(),
        ["update", "health", "--json"]
    );
}
