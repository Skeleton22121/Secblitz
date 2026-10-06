//! Owned Windows security descriptors and SIDs shared by the service installers.

use anyhow::{ensure, Result};
use std::{
    ffi::{c_void, OsStr},
    os::windows::ffi::OsStrExt,
    ptr::null_mut,
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, ConvertStringSidToSidW,
    },
};

/// A `LocalAlloc` block (security descriptor or SID) freed on drop.
pub struct Local(pub *mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        // SAFETY: the pointer came from a Convert* call that allocates with LocalAlloc.
        unsafe {
            LocalFree(self.0);
        }
    }
}

pub fn error() -> anyhow::Error {
    std::io::Error::last_os_error().into()
}

pub fn wide(s: impl AsRef<OsStr>) -> Result<Vec<u16>> {
    let mut v: Vec<u16> = s.as_ref().encode_wide().collect();
    ensure!(!v.contains(&0), "Embedded NUL");
    v.push(0);
    Ok(v)
}

pub fn descriptor(s: &str) -> Result<Local> {
    let s = wide(s)?;
    let mut p = null_mut();
    // SAFETY: `s` is NUL-terminated and `p` is a valid out pointer.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(s.as_ptr(), 1, &mut p, null_mut())
    };
    ensure!(ok != 0, "Security descriptor: {}", error());
    Ok(Local(p))
}

pub fn sid(s: &str) -> Result<Local> {
    let s = wide(s)?;
    let mut p = null_mut();
    // SAFETY: `s` is NUL-terminated and `p` is a valid out pointer.
    let ok = unsafe { ConvertStringSidToSidW(s.as_ptr(), &mut p) };
    ensure!(ok != 0, "SID: {}", error());
    Ok(Local(p))
}
