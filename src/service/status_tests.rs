use super::*;

#[test]
fn status_acl_is_exact() {
    let sd = descriptor(STATUS_DIRECTORY_SD).unwrap();
    inspect_status_descriptor(sd.0).unwrap();
    for bad in [
        STATUS_DIRECTORY_SD.replace("0x1200a9;;;BU", "0x12019f;;;BU"),
        STATUS_DIRECTORY_SD.replace("0x1301bf;;;LS", "FA;;;LS"),
        STATUS_DIRECTORY_SD.replace("O:BAG:BAD:P", "O:BAG:BAD:"),
        format!("{STATUS_DIRECTORY_SD}(A;OICI;FA;;;WD)"),
        STATUS_DIRECTORY_SD.replace("(A;OICI;0x1200a9;;;BU)", ""),
    ] {
        let sd = descriptor(&bad).unwrap();
        assert!(inspect_status_descriptor(sd.0).is_err(), "{bad}");
    }
}

#[test]
fn status_files_can_be_replaced_while_the_folder_is_pinned() {
    let dir = std::env::temp_dir().join(format!("secblitz-pin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pin = pin_status_directory(&dir).unwrap();
    let summary = crate::status::summarize(&[], true, 1);
    crate::status::write_to(&dir, &summary).unwrap();
    crate::status::write_to(&dir, &summary).unwrap();
    crate::status::write_changed_to(&dir, &["a.b".to_string()]).unwrap();
    assert!(std::fs::rename(&dir, dir.with_extension("moved")).is_err());
    drop(pin);
    std::fs::remove_dir_all(&dir).unwrap();
}
