use super::*;

#[test]
fn directory_pin_prevents_rename() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("ancestor");
    std::fs::create_dir(&path).unwrap();
    let pin = open(
        &path,
        READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY,
        OPEN_EXISTING,
        None,
    )
    .unwrap();
    assert!(std::fs::rename(&path, temp.path().join("replaced")).is_err());
    drop(pin);
    std::fs::rename(&path, temp.path().join("replaced")).unwrap();
}

#[test]
fn shared_app_acl_allows_users_rx_only() {
    for (sddl, directory) in [(APP_DIRECTORY_SD, true), (BINARY_SD, false)] {
        let sd = descriptor(sddl).unwrap();
        inspect_descriptor(sd.0, directory, true, false, true, false).unwrap();
        assert!(inspect_descriptor(sd.0, directory, true, false, false, false).is_err());
        for rights in ["FA", "0x12019f", "WD", "WO", "DC", "SD", "GW", "GA"] {
            let bad =
                descriptor(&sddl.replace("0x1200a9;;;BU", &format!("{rights};;;BU"))).unwrap();
            assert!(
                inspect_descriptor(bad.0, directory, true, false, true, false).is_err(),
                "{rights}"
            );
        }
        for bad in [sddl.replace("O:BA", "O:BU"), sddl.replace("D:P", "D:")] {
            let bad = descriptor(&bad).unwrap();
            assert!(inspect_descriptor(bad.0, directory, true, false, true, false).is_err());
        }
    }
}

#[test]
fn monitor_acl_remains_private_and_nonreplaceable() {
    for (sddl, directory, report) in [(DIRECTORY_SD, true, false), (REPORT_SD, false, true)] {
        let sd = descriptor(sddl).unwrap();
        inspect_descriptor(sd.0, directory, true, report, false, false).unwrap();
        for extra in ["(A;;FR;;;BU)", "(A;;FA;;;LS)", "(A;;FA;;;WD)"] {
            let bad = descriptor(&format!("{sddl}{extra}")).unwrap();
            assert!(inspect_descriptor(bad.0, directory, true, report, false, false).is_err());
        }
    }
}

#[test]
fn fresh_volume_root_acl_is_allowed_only_at_a_volume_root() {
    let sddl = "O:SYG:SYD:(A;OICIIO;SDGXGWGR;;;AU)(A;;0x1301bf;;;AU)(A;OICIIO;GA;;;SY)(A;;FA;;;SY)(A;OICIIO;GA;;;BA)(A;;FA;;;BA)(A;OICIIO;GXGR;;;BU)(A;;0x1200a9;;;BU)";
    let sd = descriptor(sddl).unwrap();
    inspect_descriptor(sd.0, true, false, false, false, true).unwrap();
    assert!(inspect_descriptor(sd.0, true, false, false, false, false).is_err());
    for rights in ["WD", "WO", "0x40", "GW", "GA"] {
        let bad = descriptor(&sddl.replace(
            "(A;;0x1301bf;;;AU)",
            &format!("(A;;0x1301bf;;;AU)(A;;{rights};;;AU)"),
        ))
        .unwrap();
        assert!(
            inspect_descriptor(bad.0, true, false, false, false, true).is_err(),
            "{rights}"
        );
    }
}
