use super::Target;
use anyhow::{ensure, Result};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenElevationType, TokenElevationTypeFull, TOKEN_ELEVATION_TYPE,
    TOKEN_QUERY,
};
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::Shell::ShellExecuteW;

/// True only for the elevated half of a split (UAC) administrator token.
/// A full-token administrator (built-in Administrator, or UAC off) reports
/// `TokenElevationTypeDefault` even though it is elevated, and may open pages.
fn split_token_elevated() -> Result<bool> {
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

/// Control Panel's switch for opening one item by its canonical name.
const CONTROL_SWITCH: &str = "/name";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub(super) fn open(target: Target) -> Result<()> {
    // Check the actual token here, even when a caller bypasses the UI routing.
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
