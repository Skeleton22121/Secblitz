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
        inspect_descriptor(sd.0, directory, true, false, true).unwrap();
        assert!(inspect_descriptor(sd.0, directory, true, false, false).is_err());
        for rights in ["FA", "0x12019f", "WD", "WO", "DC", "SD", "GW", "GA"] {
            let bad =
                descriptor(&sddl.replace("0x1200a9;;;BU", &format!("{rights};;;BU"))).unwrap();
            assert!(
                inspect_descriptor(bad.0, directory, true, false, true).is_err(),
                "{rights}"
            );
        }
        for bad in [sddl.replace("O:BA", "O:BU"), sddl.replace("D:P", "D:")] {
            let bad = descriptor(&bad).unwrap();
            assert!(inspect_descriptor(bad.0, directory, true, false, true).is_err());
        }
    }
}

#[test]
fn monitor_acl_remains_private_and_nonreplaceable() {
    for (sddl, directory, report) in [(DIRECTORY_SD, true, false), (REPORT_SD, false, true)] {
        let sd = descriptor(sddl).unwrap();
        inspect_descriptor(sd.0, directory, true, report, false).unwrap();
        for extra in ["(A;;FR;;;BU)", "(A;;FA;;;LS)", "(A;;FA;;;WD)"] {
            let bad = descriptor(&format!("{sddl}{extra}")).unwrap();
            assert!(inspect_descriptor(bad.0, directory, true, report, false).is_err());
        }
    }
}
