//! Files only administrators and the filter service may read. The folder lets every user read, so each file has its own permissions.

use anyhow::Result;
use std::path::Path;

/// Administrators and SYSTEM have full access; the filter's account may read and write but not delete. No owner is named: the service is not an administrator.
#[cfg(any(windows, test))]
const PRIVATE_SD: &str = "D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;0x12019f;;;LS)";

/// Rewrites `path` in place (the service may not replace it); a reader catching it mid-write sees an unreadable file, treated as "nothing yet".
#[cfg(windows)]
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use anyhow::ensure;
    use std::io::Write;
    use std::mem::size_of;
    use std::os::windows::io::FromRawHandle;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{GetLastError, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, OPEN_ALWAYS,
    };

    use crate::platform::security::{descriptor, wide};

    let sd = descriptor(PRIVATE_SD)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: 0,
    };
    let name = wide(path)?;
    // SAFETY: `name` is NUL-terminated and `attributes` points to a valid
    // descriptor that outlives the call.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ,
            &attributes,
            OPEN_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };
    ensure!(
        handle != INVALID_HANDLE_VALUE,
        "Cannot write {} ({})",
        path.display(),
        // SAFETY: reads the calling thread's last error.
        unsafe { GetLastError() }
    );
    // SAFETY: a valid handle this function owns; the File closes it.
    let mut file = unsafe { std::fs::File::from_raw_handle(handle) };
    file.write_all(bytes)?;
    file.set_len(bytes.len() as u64)?;
    Ok(())
}

#[cfg(not(windows))]
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.write_all(bytes)?;
    file.set_len(bytes.len() as u64)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_in_place_and_shortens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recent.json");
        write_private(&path, b"a long first version").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"a long first version");
        write_private(&path, b"short").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"short");
    }

    #[test]
    fn private_permissions_do_not_include_users() {
        assert!(PRIVATE_SD.starts_with("D:P("));
        for everyone in [";;;BU)", ";;;WD)", ";;;AU)", ";;;IU)"] {
            assert!(!PRIVATE_SD.contains(everyone), "{everyone}");
        }
        assert!(!PRIVATE_SD.contains("O:"));
    }
}
