use anyhow::{ensure, Result};
use std::ptr::{null, null_mut};
use windows_sys::Win32::UI::Shell::ShellExecuteW;

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
