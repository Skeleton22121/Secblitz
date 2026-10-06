use super::Target;
use anyhow::{ensure, Result};
use std::mem::zeroed;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_NOT_ALL_ASSIGNED, HANDLE, LUID,
};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, GetTokenInformation, IsWellKnownSid, LookupPrivilegeValueW,
    TokenElevationType, TokenElevationTypeFull, TokenUser, WinLocalServiceSid,
    WinLocalSystemSid, WinNetworkServiceSid, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_ELEVATION_TYPE, TOKEN_PRIVILEGES, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::System::Shutdown::InitiateShutdownW;
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::Shell::ShellExecuteW;

/// Planned "Security fix" restart through the shutdown API. Open programs may still stop it, so unsaved work is never lost.
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

/// Elevated half of a split token only: a full-token administrator reports TokenElevationTypeDefault but may open pages.
pub fn split_token_elevated() -> Result<bool> {
    unsafe {
        let mut token: HANDLE = null_mut();
        ensure!(
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) != 0,
            "Windows could not open settings"
        );
        let mut kind: TOKEN_ELEVATION_TYPE = 0;
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevationType,
            (&mut kind as *mut TOKEN_ELEVATION_TYPE).cast(),
            std::mem::size_of::<TOKEN_ELEVATION_TYPE>() as u32,
            &mut len,
        );
        CloseHandle(token);
        ensure!(ok != 0, "Windows could not open settings");
        Ok(kind == TokenElevationTypeFull)
    }
}

/// LocalSystem, LocalService and NetworkService also report TokenElevationTypeDefault; pages are for signed-in people only.
fn service_account() -> Result<bool> {
    unsafe {
        let mut token: HANDLE = null_mut();
        ensure!(
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) != 0,
            "Windows could not open settings"
        );
        let mut needed = 0u32;
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut needed);
        // 8-byte aligned buffer for the TOKEN_USER the call fills in.
        let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        let ok = GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), needed, &mut needed);
        let result = if ok != 0 {
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let sid = user.User.Sid;
            Ok([WinLocalSystemSid, WinLocalServiceSid, WinNetworkServiceSid]
                .into_iter()
                .any(|kind| IsWellKnownSid(sid, kind) != 0))
        } else {
            Err(anyhow::anyhow!("Windows could not open settings"))
        };
        CloseHandle(token);
        result
    }
}

const CONTROL_SWITCH: &str = "/name";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub(super) fn open(target: Target) -> Result<()> {
    // Check the actual token here, even when a caller bypasses the UI routing.
    ensure!(
        !service_account()?,
        "Open Settings from the non-elevated interactive application"
    );
    let split = split_token_elevated()?;
    match target {
        Target::Uri(uri) => {
            super::validate_settings_request(uri, split)?;
            shell_open(&wide(uri), None)
        }
        Target::Control(name) => {
            super::validate_control_request(name, split)?;
            // control.exe by absolute path under the system directory: never
            // found through PATH or the current directory. It is a windowed
            // program, so no console appears.
            let mut buf = [0u16; 260];
            let n = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
            ensure!(n > 0 && n < buf.len(), "Windows could not open settings");
            let dir = String::from_utf16(&buf[..n])?;
            let exe = format!("{dir}\\control.exe");
            shell_open(&wide(&exe), Some(&wide(&format!("{} {}", CONTROL_SWITCH, name))))
        }
    }
}

fn shell_open(file: &[u16], params: Option<&[u16]>) -> Result<()> {
    let verb = wide("open");
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.map_or(null(), |p| p.as_ptr()),
            null(),
            1,
        )
    } as isize;
    ensure!(
        result > 32,
        "Windows could not open settings (ShellExecute code {result})"
    );
    Ok(())
}
