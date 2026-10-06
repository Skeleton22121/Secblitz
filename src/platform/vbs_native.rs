//! Windows side of `src/vbs.rs`: reads what the decisions need. Everything
//! here only reads: the registry, the list of loaded modules, the first bytes
//! of driver files and one fixed read-only script. No driver is loaded.
use super::run_script_in;
use crate::model::Finding;
use crate::vbs::{self, Facts, Scan, ServiceRow};
use anyhow::{bail, ensure, Result};
use std::{
    ffi::c_void,
    io::Read,
    mem::size_of,
    ptr::{null, null_mut},
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::FILETIME,
    System::{
        ProcessStatus::{EnumDeviceDrivers, GetDeviceDriverFileNameW},
        Registry::{
            RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryInfoKeyW, RegQueryValueExW, HKEY,
            HKEY_LOCAL_MACHINE, KEY_READ, REG_DWORD, REG_EXPAND_SZ, REG_SZ,
        },
    },
};

const MEMORY_INTEGRITY_KEY: &str =
    r"SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity";
const STACK_KEY: &str = r"SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\KernelShadowStacks";
const SERVICES_KEY: &str = r"SYSTEM\CurrentControlSet\Services";

const NOT_FOUND: u32 = 2;
const PATH_NOT_FOUND: u32 = 3;
const NO_MORE_ITEMS: u32 = 259;
/// Seconds between 1601 (Windows file times) and 1970.
const EPOCH_GAP: u64 = 11_644_473_600;
/// Enough to hold the headers of any driver image.
const HEADER_BYTES: u64 = 64 * 1024;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by RegOpenKeyExW and is closed once.
        unsafe { RegCloseKey(self.0) };
    }
}

fn open(parent: HKEY, path: &str) -> Result<Option<Key>> {
    let path = wide(path);
    let mut handle: HKEY = null_mut();
    // SAFETY: `path` is NUL terminated and `handle` is a valid out pointer.
    let status = unsafe { RegOpenKeyExW(parent, path.as_ptr(), 0, KEY_READ, &mut handle) };
    match status {
        0 => Ok(Some(Key(handle))),
        NOT_FOUND | PATH_NOT_FOUND => Ok(None),
        other => bail!("cannot open the key ({other})"),
    }
}

fn dword(key: &Key, name: &str) -> Option<u32> {
    let name = wide(name);
    let mut kind = 0u32;
    let mut data = [0u8; 4];
    let mut size = 4u32;
    // SAFETY: the buffer is 4 bytes and `size` says so.
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            null(),
            &mut kind,
            data.as_mut_ptr(),
            &mut size,
        )
    };
    (status == 0 && kind == REG_DWORD && size == 4).then(|| u32::from_le_bytes(data))
}

fn text(key: &Key, name: &str) -> Option<String> {
    let name = wide(name);
    let mut kind = 0u32;
    let mut buf = [0u16; 1040];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: `size` is the byte length of `buf`.
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            null(),
            &mut kind,
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != 0 || !(kind == REG_SZ || kind == REG_EXPAND_SZ) {
        return None;
    }
    let len = (size as usize / 2).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]).trim_end_matches('\0').to_owned())
}

/// A registry value of one of the two scenarios, or None when absent.
fn scenario_value(path: &str, name: &str) -> Option<u32> {
    let key = open(HKEY_LOCAL_MACHINE, path).ok()??;
    dword(&key, name)
}

/// Unix seconds when anything under the key was last written.
fn last_write_unix(path: &str) -> Option<i64> {
    let key = open(HKEY_LOCAL_MACHINE, path).ok()??;
    let mut time = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    // SAFETY: only the last-write out pointer is given; the rest may be null.
    let status = unsafe {
        RegQueryInfoKeyW(
            key.0,
            null_mut(),
            null_mut(),
            null(),
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut time,
        )
    };
    if status != 0 {
        return None;
    }
    let ticks = (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    i64::try_from((ticks / 10_000_000).checked_sub(EPOCH_GAP)?).ok()
}

/// Kernel and file-system driver services as the registry lists them.
fn services() -> Result<Vec<ServiceRow>> {
    let Some(root) = open(HKEY_LOCAL_MACHINE, SERVICES_KEY)? else {
        bail!("the driver list is missing");
    };
    let mut rows = Vec::new();
    for index in 0..16384u32 {
        let mut name = [0u16; 256];
        let mut len = name.len() as u32;
        // SAFETY: `len` is the buffer length in characters.
        let status = unsafe {
            RegEnumKeyExW(
                root.0,
                index,
                name.as_mut_ptr(),
                &mut len,
                null(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        if status == NO_MORE_ITEMS {
            return Ok(rows);
        }
        if status != 0 {
            continue;
        }
        let Ok(Some(service)) = open(root.0, &String::from_utf16_lossy(&name[..len as usize]))
        else {
            continue;
        };
        let (Some(kind), Some(start)) = (dword(&service, "Type"), dword(&service, "Start")) else {
            continue;
        };
        rows.push(ServiceRow {
            name: String::from_utf16_lossy(&name[..len as usize]),
            kind,
            start,
            image: text(&service, "ImagePath"),
        });
    }
    bail!("too many services to list")
}

/// File names of the kernel modules loaded right now.
fn loaded_modules() -> Vec<String> {
    let mut bases = vec![null_mut::<c_void>(); 2048];
    let mut needed = 0u32;
    // SAFETY: the byte size matches the buffer.
    let ok = unsafe {
        EnumDeviceDrivers(
            bases.as_mut_ptr(),
            (bases.len() * size_of::<*mut c_void>()) as u32,
            &mut needed,
        )
    };
    if ok == 0 {
        return Vec::new();
    }
    let count = (needed as usize / size_of::<*mut c_void>()).min(bases.len());
    let mut out = Vec::new();
    for base in &bases[..count] {
        let mut buf = [0u16; 520];
        // SAFETY: the size is the buffer length in characters.
        let n = unsafe { GetDeviceDriverFileNameW(*base, buf.as_mut_ptr(), buf.len() as u32) };
        if n > 0 && (n as usize) < buf.len() {
            out.push(String::from_utf16_lossy(&buf[..n as usize]));
        }
    }
    out
}

fn read_header(path: &str) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    std::fs::File::open(path)?
        .take(HEADER_BYTES)
        .read_to_end(&mut out)?;
    Ok(out)
}

/// Scan every driver that would load or is loaded. A list that cannot be read
/// counts as "could not check", never as "all fine".
pub(super) fn scan_drivers(windows: &str) -> Scan {
    match services() {
        Ok(rows) => {
            let files = vbs::driver_files(&rows, &loaded_modules(), windows);
            vbs::scan_files(&files, &read_header)
        }
        Err(_) => Scan {
            flagged: Vec::new(),
            unreadable: vec!["drivers".into()],
        },
    }
}

/// The read-only facts script. One short PowerShell run, no extra processes.
pub(super) fn facts() -> Result<Facts> {
    let script = format!(
        "$action='vbs_facts'\n$id=''\n$inputJson=$null\n{}\n{}",
        super::super::backend_definitions()?,
        include_str!("vbs.ps1")
    );
    let facts: Facts = run_script_in(script, Duration::from_secs(60), 1)?;
    ensure!(facts.available.len() < 64, "unexpected protection facts");
    Ok(facts)
}

/// True when memory integrity or stack protection is switched on in the
/// registry, which is when checking whether it runs is worth a script run.
fn any_configured() -> bool {
    scenario_value(MEMORY_INTEGRITY_KEY, "Enabled") == Some(1)
        || scenario_value(STACK_KEY, "Enabled") == Some(1)
}

/// What to say after a restart when a setting is on but not running. Never an
/// error: a check that cannot be made simply reports nothing.
pub(super) fn verification_findings() -> Vec<Finding> {
    if !any_configured() {
        return Vec::new();
    }
    match facts() {
        Ok(facts) => vbs::verification(
            &facts,
            last_write_unix(MEMORY_INTEGRITY_KEY),
            last_write_unix(STACK_KEY),
        ),
        Err(_) => Vec::new(),
    }
}
