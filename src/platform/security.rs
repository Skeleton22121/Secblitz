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

/// Longest path, in UTF-16 units, read from a string Windows hands back.
pub const MAX_WIDE_UNITS: usize = 32768;

/// The text of a NUL-terminated UTF-16 string, or `None` when the pointer is null or no
/// terminator appears within [`MAX_WIDE_UNITS`].
///
/// # Safety
/// A non-null `raw` must point to readable memory that stays valid and unchanged for `'a`,
/// up to its terminator or [`MAX_WIDE_UNITS`] units.
pub unsafe fn wide_str<'a>(raw: *const u16) -> Option<&'a [u16]> {
    if raw.is_null() {
        return None;
    }
    let mut n = 0;
    while n < MAX_WIDE_UNITS && *raw.add(n) != 0 {
        n += 1;
    }
    (n < MAX_WIDE_UNITS).then(|| std::slice::from_raw_parts(raw, n))
}

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
