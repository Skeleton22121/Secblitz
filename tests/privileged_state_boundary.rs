//! Saved service permission snapshots and driver paths are replayed by the
//! privileged engine, so tampered or malformed input must be refused.
use secblitz::permissions::{repair_target, validate_value, TARGET_SENTINEL};
use secblitz::vbs::{
    boot_from_detail, driver_files, is_vbs_check_id, resolve_image, safe_name, split_batches,
    ServiceRow,
};
use serde_json::{json, Value};

const BITS: &str = "permissions.service.bits";
const WINDOWS: &str = r"C:\Windows";

fn sid(authority: u8, subs: &[u32]) -> Vec<u8> {
    let mut b = vec![1, subs.len() as u8, 0, 0, 0, 0, 0, authority];
    for sub in subs {
        b.extend(sub.to_le_bytes());
    }
    b
}

fn acl(aces: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut acl = vec![2, 0, 0, 0, 0, 0, 0, 0];
    acl[4..6].copy_from_slice(&(aces.len() as u16).to_le_bytes());
    for (mask, sid) in aces {
        acl.extend([0, 0]);
        acl.extend(((8 + sid.len()) as u16).to_le_bytes());
        acl.extend(mask.to_le_bytes());
        acl.extend(sid);
    }
    let size = acl.len() as u16;
    acl[2..4].copy_from_slice(&size.to_le_bytes());
    acl
}

fn descriptor(owner: Vec<u8>, acl: Vec<u8>) -> Vec<u8> {
    let group = sid(5, &[32, 544]);
    let mut b = vec![0; 20];
    b[0] = 1;
    b[2..4].copy_from_slice(&0x9004u16.to_le_bytes());
    for (slot, part) in [(4, &owner), (8, &group), (16, &acl)] {
        let offset = b.len() as u32;
        b[slot..slot + 4].copy_from_slice(&offset.to_le_bytes());
        b.extend(part);
    }
    b
}

fn encode(sd: &[u8]) -> Value {
    let hex: String = sd.iter().map(|b| format!("{b:02x}")).collect();
    json!(format!("dacl-v1:{hex}"))
}

fn everyone_full() -> Vec<u8> {
    descriptor(
        sid(5, &[18]),
        acl(&[(0xf01ff, sid(1, &[0])), (0xf01ff, sid(5, &[32, 544]))]),
    )
}

#[test]
fn a_well_formed_snapshot_is_accepted() {
    validate_value(BITS, &encode(&everyone_full())).unwrap();
}

#[test]
fn only_the_two_known_services_are_accepted() {
    let value = encode(&everyone_full());
    validate_value("permissions.service.wuauserv", &value).unwrap();
    for id in [
        "permissions.service.other",
        "",
        "firewall.public.enabled",
        TARGET_SENTINEL,
    ] {
        assert!(validate_value(id, &value).is_err(), "{id}");
        assert!(repair_target(id, &value).is_err(), "{id}");
    }
}

#[test]
fn snapshots_that_are_not_text_are_refused() {
    for value in [
        json!(null),
        json!(7),
        json!(true),
        json!([1, 2]),
        json!({"a": 1}),
    ] {
        assert!(validate_value(BITS, &value).is_err());
    }
}

#[test]
fn snapshots_with_the_wrong_version_prefix_are_refused() {
    let hex: String = everyone_full().iter().map(|b| format!("{b:02x}")).collect();
    for prefix in ["", "dacl-v2:", "DACL-V1:", "dacl-v1"] {
        assert!(
            validate_value(BITS, &json!(format!("{prefix}{hex}"))).is_err(),
            "{prefix}"
        );
    }
}

#[test]
fn snapshots_with_uppercase_odd_or_stray_hex_are_refused() {
    let sd = everyone_full();
    let lower: String = sd.iter().map(|b| format!("{b:02x}")).collect();
    assert!(validate_value(BITS, &json!(format!("dacl-v1:{}", lower.to_uppercase()))).is_err());
    assert!(validate_value(BITS, &json!(format!("dacl-v1:{lower}0"))).is_err());
    assert!(validate_value(BITS, &json!(format!("dacl-v1:{lower}zz"))).is_err());
    assert!(validate_value(BITS, &json!(format!("dacl-v1: {lower}"))).is_err());
}

#[test]
fn a_snapshot_cut_short_at_any_point_is_refused() {
    let sd = everyone_full();
    for cut in 0..sd.len() {
        assert!(
            validate_value(BITS, &encode(&sd[..cut])).is_err(),
            "cut at {cut}"
        );
    }
}

#[test]
fn a_snapshot_with_extra_bytes_on_the_end_is_refused() {
    let mut sd = everyone_full();
    sd.extend([0, 0, 0, 0]);
    assert!(validate_value(BITS, &encode(&sd)).is_err());
}

#[test]
fn a_snapshot_larger_than_the_limit_is_refused() {
    let huge = format!("dacl-v1:{}", "00".repeat(16 * 1024 + 1));
    assert!(validate_value(BITS, &json!(huge)).is_err());
}

#[test]
fn a_snapshot_that_asks_for_audit_rules_is_refused() {
    let mut sd = everyone_full();
    sd[12..16].copy_from_slice(&20u32.to_le_bytes());
    assert!(validate_value(BITS, &encode(&sd)).is_err());
}

#[test]
fn a_snapshot_with_unsupported_header_values_is_refused() {
    for (at, byte) in [(0usize, 2u8), (1, 1)] {
        let mut sd = everyone_full();
        sd[at] = byte;
        assert!(validate_value(BITS, &encode(&sd)).is_err(), "byte {at}");
    }
    let mut sd = everyone_full();
    sd[2..4].copy_from_slice(&0x0001u16.to_le_bytes());
    assert!(validate_value(BITS, &encode(&sd)).is_err());
}

#[test]
fn a_snapshot_whose_owner_points_outside_the_data_is_refused() {
    let mut sd = everyone_full();
    sd[4..8].copy_from_slice(&0xffff_fff0u32.to_le_bytes());
    assert!(validate_value(BITS, &encode(&sd)).is_err());
}

#[test]
fn repair_removes_dangerous_rights_from_everyone_and_keeps_the_rest() {
    let sd = everyone_full();
    let target = repair_target(BITS, &encode(&sd)).unwrap();
    validate_value(BITS, &target).unwrap();
    assert_ne!(target, encode(&sd));
    assert_eq!(repair_target(BITS, &target).unwrap(), target);
}

#[test]
fn repair_leaves_an_already_safe_snapshot_untouched() {
    let safe = descriptor(
        sid(5, &[18]),
        acl(&[
            (0x20002 & !0x000d_0002 | 0x4, sid(1, &[0])),
            (0xf01ff, sid(5, &[32, 544])),
        ]),
    );
    let value = encode(&safe);
    assert_eq!(repair_target(BITS, &value).unwrap(), value);
}

#[test]
fn repair_refuses_a_service_owned_by_someone_untrusted() {
    let sd = descriptor(
        sid(5, &[21, 1, 2, 3, 1001]),
        acl(&[(0xf01ff, sid(1, &[0]))]),
    );
    let value = encode(&sd);
    validate_value(BITS, &value).unwrap();
    assert!(repair_target(BITS, &value).is_err());
}

#[test]
fn repair_refuses_deny_rules_it_cannot_judge() {
    let mut a = acl(&[(0xf01ff, sid(1, &[0]))]);
    a[8] = 1;
    let value = encode(&descriptor(sid(5, &[18]), a));
    assert!(repair_target(BITS, &value).is_err());
}

#[test]
fn repair_refuses_a_missing_permission_list() {
    let mut sd = everyone_full();
    sd[16..20].copy_from_slice(&0u32.to_le_bytes());
    sd.truncate(sd.len() - acl(&[(0xf01ff, sid(1, &[0])), (0xf01ff, sid(5, &[32, 544]))]).len());
    let value = encode(&sd);
    assert!(repair_target(BITS, &value).is_err());
}

#[test]
fn driver_image_paths_are_resolved_inside_the_windows_folder_only() {
    assert_eq!(
        resolve_image(r"\SystemRoot\System32\drivers\a.sys", WINDOWS).as_deref(),
        Some(r"C:\Windows\System32\drivers\a.sys")
    );
    assert_eq!(
        resolve_image(r"system32\drivers\a.sys", WINDOWS).as_deref(),
        Some(r"C:\Windows\system32\drivers\a.sys")
    );
    assert_eq!(
        resolve_image(r"\??\D:\x\b.sys", WINDOWS).as_deref(),
        Some(r"D:\x\b.sys")
    );
}

#[test]
fn driver_image_paths_that_escape_or_are_odd_are_refused() {
    for raw in [
        "",
        "   ",
        r"\SystemRoot\..\..\evil.sys",
        r"system32\..\..\evil.sys",
        r"\Device\HarddiskVolume1\a.sys",
        r"\SystemRoot\System32\notadriver.exe",
        "C:\\a\u{0}.sys",
        "\\SystemRoot\\a\n.sys",
        r"\\server\share\a.sys",
    ] {
        assert_eq!(resolve_image(raw, WINDOWS), None, "{raw:?}");
    }
}

#[test]
fn file_names_are_kept_only_when_plain() {
    assert_eq!(
        safe_name("my-driver_1.sys").as_deref(),
        Some("my-driver_1.sys")
    );
    for bad in ["", "a b", "a/b", "a\\b", "a;b", &"x".repeat(65)] {
        assert_eq!(safe_name(bad), None, "{bad:?}");
    }
}

#[test]
fn services_that_are_not_boot_or_system_drivers_are_not_listed() {
    let rows = [
        ServiceRow {
            name: "disabled".into(),
            kind: 1,
            start: 4,
            image: None,
        },
        ServiceRow {
            name: "userservice".into(),
            kind: 16,
            start: 2,
            image: None,
        },
        ServiceRow {
            name: "good".into(),
            kind: 1,
            start: 1,
            image: None,
        },
    ];
    let list = driver_files(&rows, &[], WINDOWS);
    assert_eq!(list.files.len(), 1);
    assert_eq!(list.files[0].path, r"C:\Windows\System32\drivers\good.sys");
    assert!(list.unresolved.is_empty());
}

#[test]
fn a_driver_with_an_unreadable_image_is_reported_not_skipped() {
    let rows = [ServiceRow {
        name: "odd".into(),
        kind: 1,
        start: 0,
        image: Some(r"\SystemRoot\..\x.sys".into()),
    }];
    let list = driver_files(&rows, &[], WINDOWS);
    assert!(list.files.is_empty());
    assert_eq!(list.unresolved, vec!["odd".to_string()]);
}

#[test]
fn a_driver_listed_twice_is_scanned_once() {
    let rows = [ServiceRow {
        name: "a".into(),
        kind: 1,
        start: 0,
        image: None,
    }];
    let loaded = vec![r"\SystemRoot\System32\drivers\A.SYS".to_string()];
    assert_eq!(driver_files(&rows, &loaded, WINDOWS).files.len(), 1);
}

#[test]
fn protection_checks_run_one_at_a_time_and_ordinary_ones_together() {
    let ids: Vec<String> = [
        "a.one",
        "vbs.memory_integrity",
        "b.two",
        "vbs.kernel_stack_protection",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert!(is_vbs_check_id("vbs.memory_integrity"));
    assert!(!is_vbs_check_id("vbs.other"));
    let batches = split_batches(&ids);
    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0], vec!["a.one", "b.two"]);
    assert_eq!(batches[1], vec!["vbs.memory_integrity"]);
    assert_eq!(batches[2], vec!["vbs.kernel_stack_protection"]);
    assert!(split_batches(&[]).is_empty());
}

#[test]
fn the_restart_marker_is_read_only_when_it_is_a_number() {
    assert_eq!(boot_from_detail("boot: 42 more"), Some(42));
    assert_eq!(boot_from_detail("boot: abc"), None);
    assert_eq!(boot_from_detail("nothing here"), None);
    assert_eq!(boot_from_detail("boot: 99999999999999999999999"), None);
}
