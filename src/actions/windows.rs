use anyhow::{ensure, Result};
use std::mem::zeroed;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_NOT_ALL_ASSIGNED, HANDLE, LUID,
};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Shutdown::InitiateShutdownW;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::Shell::ShellExecuteW;

/// Restart the PC the way Windows Update would: a planned "Operating System:
/// Security fix" restart through the documented shutdown API (no console, no
/// child process). Open programs are asked to close and may stop it, so
/// unsaved work is never thrown away.
pub(super) fn restart_for_updates() -> Result<()> {
    unsafe {
        let mut token: HANDLE = null_mut();
        ensure!(
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
                &mut token
            ) != 0,
            "Windows would not let us restart this PC. Restart it from the Start menu instead."
        );
        let name: Vec<u16> = "SeShutdownPrivilege".encode_utf16().chain(Some(0)).collect();
        let mut luid: LUID = zeroed();
        let enabled = LookupPrivilegeValueW(null(), name.as_ptr(), &mut luid) != 0 && {
            let privileges = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES {
                    Luid: luid,
                    Attributes: SE_PRIVILEGE_ENABLED,
                }],
            };
            AdjustTokenPrivileges(token, 0, &privileges, 0, null_mut(), null_mut()) != 0
                && GetLastError() != ERROR_NOT_ALL_ASSIGNED
        };
        CloseHandle(token);
        ensure!(enabled, "Windows would not let us restart this PC. Restart it from the Start menu instead.");
        let code = InitiateShutdownW(
            null(),
            null(),
            0,
            super::RESTART_FLAGS,
            super::RESTART_REASON,
        );
        ensure!(code == 0, "Windows could not start the restart. Restart it from the Start menu instead.");
    }
    Ok(())
}

pub(super) fn open_settings(uri: &str) -> Result<()> {
    // Check the actual token here, even when a caller bypasses the UI routing.
    super::validate_settings_request(uri, crate::platform::is_elevated()?)?;
    let uri: Vec<u16> = uri.encode_utf16().chain(Some(0)).collect();
    let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    let result =
        unsafe { ShellExecuteW(null_mut(), verb.as_ptr(), uri.as_ptr(), null(), null(), 1) }
            as isize;
    ensure!(
        result > 32,
        "Windows could not open settings (ShellExecute code {result})"
    );
    Ok(())
}
