//! The Name Resolution Policy entry for `.` that sends every name to the filter,
//! written to the DNS client policy key under a fixed name of our own. Only that
//! entry is ever written or deleted; network adapters are never touched.

use anyhow::{ensure, Result};
use std::net::IpAddr;
use std::ptr::{null, null_mut};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteKeyExW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, KEY_WOW64_64KEY, REG_DWORD, REG_MULTI_SZ,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};

use super::control::{parse_servers_value, servers_value};

const POLICY: &str = r"SYSTEM\CurrentControlSet\Services\Dnscache\Parameters\DnsPolicyConfig";
/// Our entry's fixed name. Nothing else in the policy key is ever opened for writing.
const RULE: &str = "{0EE85A24-B573-4712-97FF-CC4BC51D8757}";
const DISPLAY_NAME: &str = "Secblitz web protection";
const COMMENT: &str = "Managed by Secblitz";
const NOT_FOUND: u32 = 2;
const MAX_VALUE: usize = 4096;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn rule_path() -> Vec<u16> {
    wide(&format!(r"{POLICY}\{RULE}"))
}

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

impl Key {
    fn open_read() -> Result<Option<Key>> {
        let mut key: HKEY = null_mut();
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                rule_path().as_ptr(),
                0,
                KEY_READ | KEY_WOW64_64KEY,
                &mut key,
            )
        };
        if status == NOT_FOUND {
            return Ok(None);
        }
        ensure!(
            status == 0,
            "Cannot read the web protection rule ({status})"
        );
        Ok(Some(Key(key)))
    }

    fn create() -> Result<Key> {
        let mut key: HKEY = null_mut();
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_LOCAL_MACHINE,
                rule_path().as_ptr(),
                0,
                null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE | KEY_WOW64_64KEY,
                null(),
                &mut key,
                null_mut(),
            )
        };
        ensure!(
            status == 0,
            "Cannot save the web protection rule ({status})"
        );
        Ok(Key(key))
    }

    fn string(&self, name: &str) -> Option<String> {
        let mut kind = 0u32;
        let mut buffer = vec![0u16; MAX_VALUE];
        let mut bytes = (buffer.len() * 2) as u32;
        let status = unsafe {
            RegQueryValueExW(
                self.0,
                wide(name).as_ptr(),
                null(),
                &mut kind,
                buffer.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if status != 0 || (kind != REG_SZ && kind != REG_MULTI_SZ) {
            return None;
        }
        buffer.truncate(bytes as usize / 2);
        let parts: Vec<String> = buffer
            .split(|c| *c == 0)
            .filter(|p| !p.is_empty())
            .map(String::from_utf16_lossy)
            .collect();
        Some(parts.join(";"))
    }

    fn set_raw(&self, name: &str, kind: u32, data: &[u8]) -> Result<()> {
        let status = unsafe {
            RegSetValueExW(
                self.0,
                wide(name).as_ptr(),
                0,
                kind,
                data.as_ptr(),
                data.len() as u32,
            )
        };
        ensure!(
            status == 0,
            "Cannot save the web protection rule ({status})"
        );
        Ok(())
    }

    fn set_string(&self, name: &str, value: &str, kind: u32) -> Result<()> {
        let mut units: Vec<u16> = value.encode_utf16().collect();
        units.push(0);
        if kind == REG_MULTI_SZ {
            units.push(0);
        }
        let bytes: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
        self.set_raw(name, kind, &bytes)
    }

    fn set_dword(&self, name: &str, value: u32) -> Result<()> {
        self.set_raw(name, REG_DWORD, &value.to_le_bytes())
    }
}

/// also empties on its own within a minute (blocked answers live 60 s).
fn flush_cache() {
    unsafe {
        let module = LoadLibraryExW(
            wide("dnsapi.dll").as_ptr(),
            null_mut(),
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        );
        if module.is_null() {
            return;
        }
        if let Some(flush) = GetProcAddress(module, c"DnsFlushResolverCache".as_ptr().cast()) {
            let flush: unsafe extern "system" fn() -> i32 = std::mem::transmute(flush);
            flush();
        }
    }
}

/// The servers of our rule, or `None` when there is no rule of ours. A rule
/// that is not exactly ours (another namespace, odd servers) reads as an
/// empty list, so the next change rewrites it.
pub fn current_rule() -> Result<Option<Vec<IpAddr>>> {
    let Some(key) = Key::open_read()? else {
        return Ok(None);
    };
    let namespace = key.string("Name").unwrap_or_default();
    let servers = key.string("GenericDNSServers").unwrap_or_default();
    if namespace != "." {
        return Ok(Some(Vec::new()));
    }
    Ok(Some(parse_servers_value(&servers)))
}

pub fn set_rule(servers: &[IpAddr]) -> Result<()> {
    ensure!(
        !servers.is_empty() && servers.len() <= 12,
        "Wrong number of servers"
    );
    let key = Key::create()?;
    // The namespace goes last, so the entry only applies once it is complete.
    key.set_dword("Version", 2)?;
    key.set_dword("ConfigOptions", 8)?;
    key.set_string("GenericDNSServers", &servers_value(servers), REG_SZ)?;
    key.set_string("IPSECCARestriction", "", REG_SZ)?;
    key.set_string("DisplayName", DISPLAY_NAME, REG_SZ)?;
    key.set_string("Comment", COMMENT, REG_SZ)?;
    key.set_string("Name", ".", REG_MULTI_SZ)?;
    drop(key);
    ensure!(
        current_rule()?.as_deref() == Some(servers),
        "The web protection rule was not saved"
    );
    flush_cache();
    Ok(())
}

pub fn remove_rule() -> Result<()> {
    let status =
        unsafe { RegDeleteKeyExW(HKEY_LOCAL_MACHINE, rule_path().as_ptr(), KEY_WOW64_64KEY, 0) };
    ensure!(
        status == 0 || status == NOT_FOUND,
        "Cannot remove the web protection rule ({status})"
    );
    ensure!(
        current_rule()?.is_none(),
        "The web protection rule is still there"
    );
    flush_cache();
    Ok(())
}
