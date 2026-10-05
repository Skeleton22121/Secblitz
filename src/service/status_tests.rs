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
