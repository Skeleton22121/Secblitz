//! Which apps used the camera, microphone and location, and the switches Windows keeps for them.
//!
//! The window treats the launcher's listing file as untrusted; every switch is rechecked against the launcher's own copy.
#![cfg_attr(not(windows), allow(dead_code))]

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const CONSENT_STORE: &str =
    r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore";
const NON_PACKAGED: &str = "NonPackaged";
const APP_PRIVACY_POLICY: &str = r"SOFTWARE\Policies\Microsoft\Windows\AppPrivacy";
const MAX_APPS: usize = 200;
const MAX_DESKTOP: usize = 400;
const NAMED_DESKTOP: usize = 12;
const MAX_NAME: usize = 60;
const NAMES_SHOWN: usize = 3;
const NAMES_BUDGET: usize = 60;
const MAX_HANDOFF: u64 = 256 * 1024;
const FILETIME_UNIX_OFFSET: u64 = 11_644_473_600;
const YEAR_3000: u64 = 32_503_680_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    Camera,
    Microphone,
    Location,
}

impl Capability {
    pub const ALL: [Capability; 3] = [
        Capability::Camera,
        Capability::Microphone,
        Capability::Location,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Capability::Camera => "webcam",
            Capability::Microphone => "microphone",
            Capability::Location => "location",
        }
    }

    pub fn to_byte(self) -> u8 {
        match self {
            Capability::Camera => 0,
            Capability::Microphone => 1,
            Capability::Location => 2,
        }
    }

    pub fn from_byte(byte: u8) -> Option<Self> {
        Self::ALL.get(usize::from(byte)).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Master,
    DesktopApps,
    App(u8),
}

impl Target {
    pub fn to_byte(self) -> u8 {
        match self {
            Target::Master => 0,
            Target::DesktopApps => 1,
            Target::App(i) => 2 + i,
        }
    }

    pub fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            0 => Target::Master,
            1 => Target::DesktopApps,
            n if usize::from(n - 2) < MAX_APPS => Target::App(n - 2),
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The Store app's family name; the folder path for desktop apps.
    pub key: String,
    pub name: String,
    pub allowed: bool,
    pub in_use: bool,
    /// Unix seconds when it last started or stopped using the capability.
    pub last_used: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    pub capability: Capability,
    pub master: bool,
    pub apps: Vec<Entry>,
    pub desktop_allowed: bool,
    pub desktop: Vec<Entry>,
    #[serde(default)]
    pub controlled: bool,
}

impl Listing {
    pub fn empty(capability: Capability) -> Self {
        Self {
            capability,
            master: true,
            apps: Vec::new(),
            desktop_allowed: true,
            desktop: Vec::new(),
            controlled: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.apps.is_empty() && self.desktop.is_empty()
    }

    /// Anything read from a file is shortened, cleaned and counted again before it is shown.
    pub fn sanitized(mut self) -> Self {
        self.apps.truncate(MAX_APPS);
        self.apps.retain(|e| valid_package_key(&e.key));
        self.desktop.truncate(MAX_APPS);
        for entry in self.apps.iter_mut().chain(self.desktop.iter_mut()) {
            entry.name = clean_name(&entry.name);
            entry.key = clean_name(&entry.key);
            entry.last_used = entry.last_used.filter(|t| *t < YEAR_3000);
        }
        self.apps.retain(|e| !e.name.is_empty());
        self.desktop.retain(|e| !e.name.is_empty());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recency {
    InUse,
    JustNow,
    Minutes(u64),
    Hours(u64),
    Days(u64),
    Never,
}

impl Entry {
    pub fn recency(&self, now: u64) -> Recency {
        recency(self.in_use, self.last_used, now)
    }
}

pub fn recency(in_use: bool, last_used: Option<u64>, now: u64) -> Recency {
    if in_use {
        return Recency::InUse;
    }
    let Some(at) = last_used else {
        return Recency::Never;
    };
    match now.saturating_sub(at) {
        0..=59 => Recency::JustNow,
        s @ 60..=3599 => Recency::Minutes(s / 60),
        s @ 3600..=86_399 => Recency::Hours(s / 3600),
        s => Recency::Days(s / 86_400),
    }
}

/// A Windows FILETIME (100 ns steps since 1601) as seconds since 1970. Zero means "not set".
pub fn filetime_to_unix(filetime: u64) -> Option<u64> {
    if filetime == 0 {
        return None;
    }
    let secs = (filetime / 10_000_000).checked_sub(FILETIME_UNIX_OFFSET)?;
    (secs < YEAR_3000).then_some(secs)
}

pub fn sort_entries(entries: &mut [Entry]) {
    entries.sort_by(|a, b| {
        b.in_use
            .cmp(&a.in_use)
            .then_with(|| b.last_used.cmp(&a.last_used))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

const KNOWN_PACKAGES: [(&str, &str); 32] = [
    ("Microsoft.WindowsCamera", "Windows Camera"),
    ("Microsoft.WindowsSoundRecorder", "Sound Recorder"),
    ("Microsoft.Windows.Photos", "Photos"),
    ("Microsoft.WindowsMaps", "Maps"),
    ("Microsoft.BingWeather", "Weather"),
    ("Microsoft.Windows.Cortana", "Cortana"),
    ("Microsoft.549981C3F5F10", "Cortana"),
    ("Microsoft.SkypeApp", "Skype"),
    ("Microsoft.Teams", "Microsoft Teams"),
    ("MSTeams", "Microsoft Teams"),
    ("Microsoft.MicrosoftEdge", "Microsoft Edge"),
    ("Microsoft.MicrosoftEdge.Stable", "Microsoft Edge"),
    ("Microsoft.YourPhone", "Phone Link"),
    ("Microsoft.GamingApp", "Xbox"),
    ("Microsoft.XboxApp", "Xbox"),
    ("Microsoft.OutlookForWindows", "Outlook"),
    ("microsoft.windowscommunicationsapps", "Mail and Calendar"),
    ("Microsoft.MicrosoftOfficeHub", "Microsoft 365"),
    ("Microsoft.ZuneMusic", "Media Player"),
    ("Microsoft.ZuneVideo", "Movies and TV"),
    ("Microsoft.WindowsAlarms", "Alarms and Clock"),
    ("Microsoft.LockApp", "Lock screen"),
    ("Microsoft.AAD.BrokerPlugin", "Work or school sign-in"),
    ("Microsoft.Windows.CloudExperienceHost", "Windows setup"),
    ("Microsoft.Microsoft3DViewer", "3D Viewer"),
    ("Microsoft.Office.OneNote", "OneNote"),
    ("Microsoft.Win32WebViewHost", "Desktop App Web Viewer"),
    ("Microsoft.WindowsFeedbackHub", "Feedback Hub"),
    ("Microsoft.WindowsStore", "Microsoft Store"),
    ("Microsoft.XboxGamingOverlay", "Xbox Game Bar"),
    ("Microsoft.MicrosoftStickyNotes", "Sticky Notes"),
    ("Microsoft.People", "People"),
];

pub fn package_name(key: &str) -> String {
    let family = key.split('_').next().unwrap_or(key);
    if let Some((_, name)) = KNOWN_PACKAGES
        .iter()
        .find(|(id, _)| id.eq_ignore_ascii_case(family))
    {
        return (*name).to_owned();
    }
    let mut parts: Vec<&str> = family.split('.').filter(|p| !p.is_empty()).collect();
    if parts.len() > 1 {
        parts.remove(0);
    }
    let named: Vec<&str> = parts
        .iter()
        .copied()
        .filter(|p| !looks_like_id(p))
        .collect();
    let words = if named.is_empty() { parts } else { named }
        .iter()
        .map(|p| split_words(p))
        .collect::<Vec<_>>()
        .join(" ");
    let words = clean_name(&words);
    if words.is_empty() {
        clean_name(family)
    } else {
        words
    }
}

fn looks_like_id(part: &str) -> bool {
    part.len() >= 8
        && part.bytes().all(|b| b.is_ascii_alphanumeric())
        && part.bytes().any(|b| b.is_ascii_digit())
        && !part.bytes().any(|b| b.is_ascii_lowercase())
}

fn split_words(part: &str) -> String {
    let chars: Vec<char> = part.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 {
            let prev = chars[i - 1];
            let next = chars.get(i + 1).copied();
            let after_lower = prev.is_lowercase() && c.is_uppercase();
            let acronym_end =
                prev.is_uppercase() && c.is_uppercase() && next.is_some_and(char::is_lowercase);
            if after_lower || acronym_end {
                out.push(' ');
            }
        }
        out.push(*c);
    }
    out
}

fn clean_name(text: &str) -> String {
    let kept: String = text.chars().filter(|c| !c.is_control()).collect();
    let words = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    words.chars().take(MAX_NAME).collect()
}

pub fn valid_package_key(key: &str) -> bool {
    (3..=128).contains(&key.len())
        && key.matches('_').count() == 1
        && !key.starts_with('_')
        && !key.ends_with('_')
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// A desktop app's key is its path with `#` in place of `\`.
pub fn desktop_path(key: &str) -> String {
    key.replace('#', "\\")
}

pub fn file_stem(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let lower = file.to_ascii_lowercase();
    let stem = if lower.ends_with(".exe") {
        &file[..file.len() - 4]
    } else {
        file
    };
    clean_name(stem)
}

pub fn names_summary(names: &[&str]) -> (Vec<String>, usize) {
    let mut shown: Vec<String> = Vec::new();
    let mut used = 0;
    for name in names.iter().take(NAMES_SHOWN) {
        let width = name.chars().count() + if shown.is_empty() { 0 } else { 2 };
        if !shown.is_empty() && used + width > NAMES_BUDGET {
            break;
        }
        used += width;
        shown.push((*name).to_owned());
    }
    let more = names.len() - shown.len();
    (shown, more)
}

/// Only letter-drive paths are opened for a name; a network path could stall the launcher.
pub fn local_exe_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() > 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == b'\\'
        && !path.contains("..")
        && path.len() <= 520
        && !path.chars().any(char::is_control)
}

pub trait ConsentStore {
    fn subkeys(&self, path: &str, limit: usize) -> Vec<String>;
    fn string(&self, path: &str, name: &str) -> Option<String>;
    fn qword(&self, path: &str, name: &str) -> Option<u64>;
    fn set_string(&mut self, path: &str, name: &str, value: &str) -> Result<()>;
    fn machine_string(&self, path: &str, name: &str) -> Option<String>;
    fn machine_dword(&self, path: &str, name: &str) -> Option<u32>;
}

fn base_path(capability: Capability) -> String {
    format!(r"{CONSENT_STORE}\{}", capability.key())
}

fn is_allowed(value: Option<String>) -> bool {
    !value.is_some_and(|v| v.trim().eq_ignore_ascii_case("deny"))
}

fn policy_name(capability: Capability) -> &'static str {
    match capability {
        Capability::Camera => "LetAppsAccessCamera",
        Capability::Microphone => "LetAppsAccessMicrophone",
        Capability::Location => "LetAppsAccessLocation",
    }
}

fn device_off(store: &dyn ConsentStore, capability: Capability) -> bool {
    store
        .machine_string(&base_path(capability), "Value")
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("deny"))
}

fn is_controlled(store: &dyn ConsentStore, capability: Capability) -> bool {
    let policy = store.machine_dword(APP_PRIVACY_POLICY, policy_name(capability));
    device_off(store, capability) || matches!(policy, Some(1 | 2))
}

/// The switch for every account and an organization's rule both outrank the person's own choice.
fn effective_master(store: &dyn ConsentStore, capability: Capability, own: bool) -> bool {
    if device_off(store, capability) {
        return false;
    }
    match store.machine_dword(APP_PRIVACY_POLICY, policy_name(capability)) {
        Some(1) => true,
        Some(2) => false,
        _ => own,
    }
}

struct Times {
    in_use: bool,
    last_used: Option<u64>,
    seen: bool,
}

/// Windows leaves the stop time empty on a forced close or power loss, so a start from before boot is not still running.
fn times(store: &dyn ConsentStore, path: &str, boot: Option<u64>) -> Times {
    let start = store.qword(path, "LastUsedTimeStart").unwrap_or(0);
    let stop = store.qword(path, "LastUsedTimeStop").unwrap_or(0);
    let started_since_boot = filetime_to_unix(start).is_some_and(|at| boot.is_none_or(|b| at >= b));
    Times {
        in_use: start != 0 && stop == 0 && started_since_boot,
        last_used: filetime_to_unix(start)
            .into_iter()
            .chain(filetime_to_unix(stop))
            .max(),
        seen: start != 0 || stop != 0,
    }
}

/// Reads one capability. `boot` is when Windows last started, in seconds since 1970.
pub fn read_listing(
    store: &dyn ConsentStore,
    capability: Capability,
    boot: Option<u64>,
    describe: &dyn Fn(&str) -> Option<String>,
) -> Listing {
    let base = base_path(capability);
    let mut listing = Listing::empty(capability);
    listing.master = effective_master(store, capability, is_allowed(store.string(&base, "Value")));
    listing.controlled = is_controlled(store, capability);

    for key in store.subkeys(&base, MAX_APPS * 2) {
        if listing.apps.len() >= MAX_APPS {
            break;
        }
        if key.eq_ignore_ascii_case(NON_PACKAGED) || !valid_package_key(&key) {
            continue;
        }
        let path = format!(r"{base}\{key}");
        let value = store.string(&path, "Value");
        let used = times(store, &path, boot);
        if value.is_none() && !used.seen {
            continue;
        }
        listing.apps.push(Entry {
            name: package_name(&key),
            key,
            allowed: is_allowed(value),
            in_use: used.in_use,
            last_used: used.last_used,
        });
    }
    sort_entries(&mut listing.apps);

    let non_packaged = format!(r"{base}\{NON_PACKAGED}");
    listing.desktop_allowed = is_allowed(store.string(&non_packaged, "Value"));
    let mut desktop = Vec::new();
    for key in store.subkeys(&non_packaged, MAX_DESKTOP) {
        let used = times(store, &format!(r"{non_packaged}\{key}"), boot);
        if !used.seen || key.chars().any(char::is_control) {
            continue;
        }
        desktop.push(Entry {
            name: String::new(),
            key,
            allowed: listing.desktop_allowed,
            in_use: used.in_use,
            last_used: used.last_used,
        });
    }
    sort_entries(&mut desktop);
    for (i, entry) in desktop.iter_mut().enumerate() {
        let path = desktop_path(&entry.key);
        let described = (i < NAMED_DESKTOP && local_exe_path(&path))
            .then(|| describe(&path))
            .flatten()
            .map(|d| clean_name(&d))
            .filter(|d| !d.is_empty());
        entry.name = described.unwrap_or_else(|| file_stem(&path));
    }
    desktop.retain(|e| !e.name.is_empty());
    let mut seen = std::collections::HashSet::new();
    desktop.retain(|e| seen.insert(e.name.to_lowercase()));
    desktop.truncate(MAX_APPS);
    listing.desktop = desktop;
    listing
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetError {
    Unknown,
    Controlled,
    Failed,
}

/// `target` must be in `listing` (the launcher's own copy) and still in the registry, so nothing outside the consent store is written.
pub fn set_access(
    store: &mut dyn ConsentStore,
    listing: &Listing,
    target: Target,
    tag: u8,
    allow: bool,
) -> Result<(), SetError> {
    if listing.controlled {
        return Err(SetError::Controlled);
    }
    let base = base_path(listing.capability);
    let exists = |store: &dyn ConsentStore, key: &str| {
        store
            .subkeys(&base, MAX_APPS * 2)
            .iter()
            .any(|k| k.eq_ignore_ascii_case(key))
    };
    let path = match target {
        Target::Master => base.clone(),
        Target::DesktopApps => {
            if listing.desktop.is_empty() || !exists(store, NON_PACKAGED) {
                return Err(SetError::Unknown);
            }
            format!(r"{base}\{NON_PACKAGED}")
        }
        Target::App(index) => {
            let app = listing
                .apps
                .get(usize::from(index))
                .filter(|a| valid_package_key(&a.key) && app_tag(&a.key) == tag)
                .ok_or(SetError::Unknown)?;
            if !exists(store, &app.key) {
                return Err(SetError::Unknown);
            }
            format!(r"{base}\{}", app.key)
        }
    };
    let value = if allow { "Allow" } else { "Deny" };
    store
        .set_string(&path, "Value", value)
        .map_err(|_| SetError::Failed)?;
    if store.string(&path, "Value").as_deref() == Some(value) {
        Ok(())
    } else {
        Err(SetError::Failed)
    }
}

pub fn handoff_path(capability: Capability) -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty())?;
    Some(
        PathBuf::from(base)
            .join("Secblitz")
            .join(format!("app-access-{}.json", capability.key())),
    )
}

pub fn write_handoff(path: &std::path::Path, listing: &Listing) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec(listing)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Reads and removes a listing left by the launcher; nothing in it is trusted.
pub fn take_handoff(path: &std::path::Path, capability: Capability) -> Result<Listing> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_HANDOFF + 1)
        .read_to_end(&mut bytes)?;
    let _ = std::fs::remove_file(path);
    if bytes.len() as u64 > MAX_HANDOFF {
        bail!("The list is too large");
    }
    let listing: Listing = serde_json::from_slice(&bytes)?;
    if listing.capability != capability {
        bail!("The list is for something else");
    }
    Ok(listing.sanitized())
}

/// The launcher refuses the switch when the app at that place is not the one clicked.
pub fn app_tag(key: &str) -> u8 {
    let hash = key.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b.to_ascii_lowercase())).wrapping_mul(0x0100_0193)
    });
    ((hash ^ (hash >> 16)) & TAG_MASK) as u8
}

const TAG_MASK: u32 = 0b1_1111;

pub fn encode_set(capability: Capability, target: Target, tag: u8, allow: bool) -> (u8, u8) {
    (
        capability.to_byte() | (u8::from(allow) << 2) | ((tag & TAG_MASK as u8) << 3),
        target.to_byte(),
    )
}

pub fn decode_set(lo: u8, hi: u8) -> Option<(Capability, Target, u8, bool)> {
    let target = Target::from_byte(hi)?;
    let tag = lo >> 3;
    if lo & 0b11 == 3 || (tag != 0 && !matches!(target, Target::App(_))) {
        return None;
    }
    Some((
        Capability::from_byte(lo & 0b11)?,
        target,
        tag,
        lo & 0b100 != 0,
    ))
}

#[cfg(windows)]
pub use system::{describe_exe, SystemStore};

#[cfg(windows)]
mod system {
    use super::ConsentStore;
    use anyhow::{bail, Result};
    use std::ptr::null_mut;
    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW,
        RegSetValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE,
        REG_DWORD, REG_QWORD, REG_SZ,
    };

    const DRIVE_FIXED: u32 = 3;

    pub struct SystemStore;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn open_in(root: HKEY, path: &str, access: u32) -> Option<HKEY> {
        let path = wide(path);
        let mut handle: HKEY = null_mut();
        let status = unsafe { RegOpenKeyExW(root, path.as_ptr(), 0, access, &mut handle) };
        (status == 0).then_some(handle)
    }

    fn open(path: &str, access: u32) -> Option<HKEY> {
        open_in(HKEY_CURRENT_USER, path, access)
    }

    fn read_string(root: HKEY, path: &str, name: &str) -> Option<String> {
        let handle = open_in(root, path, KEY_READ)?;
        let name = wide(name);
        let mut kind = 0u32;
        let mut buf = [0u16; 128];
        let mut size = (buf.len() * 2) as u32;
        let status = unsafe {
            RegQueryValueExW(
                handle,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        unsafe { RegCloseKey(handle) };
        if status != 0 || kind != REG_SZ {
            return None;
        }
        let len = (size as usize / 2).min(buf.len());
        Some(
            String::from_utf16_lossy(&buf[..len])
                .trim_end_matches('\0')
                .to_owned(),
        )
    }

    impl ConsentStore for SystemStore {
        fn subkeys(&self, path: &str, limit: usize) -> Vec<String> {
            let Some(handle) = open(path, KEY_READ) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            for index in 0..limit as u32 {
                let mut buf = [0u16; 256];
                let mut len = buf.len() as u32;
                let status = unsafe {
                    RegEnumKeyExW(
                        handle,
                        index,
                        buf.as_mut_ptr(),
                        &mut len,
                        std::ptr::null(),
                        null_mut(),
                        null_mut(),
                        null_mut(),
                    )
                };
                if status != 0 {
                    break;
                }
                out.push(String::from_utf16_lossy(&buf[..len as usize]));
            }
            unsafe { RegCloseKey(handle) };
            out
        }

        fn string(&self, path: &str, name: &str) -> Option<String> {
            read_string(HKEY_CURRENT_USER, path, name)
        }

        fn qword(&self, path: &str, name: &str) -> Option<u64> {
            let handle = open(path, KEY_READ)?;
            let name = wide(name);
            let mut kind = 0u32;
            let mut data = [0u8; 8];
            let mut size = 8u32;
            let status = unsafe {
                RegQueryValueExW(
                    handle,
                    name.as_ptr(),
                    std::ptr::null(),
                    &mut kind,
                    data.as_mut_ptr(),
                    &mut size,
                )
            };
            unsafe { RegCloseKey(handle) };
            (status == 0 && kind == REG_QWORD && size == 8).then(|| u64::from_le_bytes(data))
        }

        fn machine_string(&self, path: &str, name: &str) -> Option<String> {
            read_string(HKEY_LOCAL_MACHINE, path, name)
        }

        fn machine_dword(&self, path: &str, name: &str) -> Option<u32> {
            let handle = open_in(HKEY_LOCAL_MACHINE, path, KEY_READ)?;
            let name = wide(name);
            let mut kind = 0u32;
            let mut data = [0u8; 4];
            let mut size = 4u32;
            let status = unsafe {
                RegQueryValueExW(
                    handle,
                    name.as_ptr(),
                    std::ptr::null(),
                    &mut kind,
                    data.as_mut_ptr(),
                    &mut size,
                )
            };
            unsafe { RegCloseKey(handle) };
            (status == 0 && kind == REG_DWORD && size == 4).then(|| u32::from_le_bytes(data))
        }

        fn set_string(&mut self, path: &str, name: &str, value: &str) -> Result<()> {
            let key = wide(path);
            let mut handle: HKEY = null_mut();
            let status = unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    0,
                    std::ptr::null(),
                    0,
                    KEY_SET_VALUE,
                    std::ptr::null(),
                    &mut handle,
                    null_mut(),
                )
            };
            if status != 0 {
                bail!("cannot open the key for writing ({status})");
            }
            let name = wide(name);
            let data = wide(value);
            let status = unsafe {
                RegSetValueExW(
                    handle,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            };
            unsafe { RegCloseKey(handle) };
            if status != 0 {
                bail!("cannot write the value ({status})");
            }
            Ok(())
        }
    }

    pub fn describe_exe(path: &str) -> Option<String> {
        let root = wide(&path.chars().take(3).collect::<String>());
        if unsafe { GetDriveTypeW(root.as_ptr()) } != DRIVE_FIXED {
            return None;
        }
        let file = wide(path);
        let mut ignored = 0u32;
        let size = unsafe { GetFileVersionInfoSizeW(file.as_ptr(), &mut ignored) };
        if size == 0 || size > 4 * 1024 * 1024 {
            return None;
        }
        let mut block = vec![0u8; size as usize];
        if unsafe { GetFileVersionInfoW(file.as_ptr(), 0, size, block.as_mut_ptr().cast()) } == 0 {
            return None;
        }
        let query = |sub: &str| -> Option<String> {
            let sub = wide(sub);
            let mut value: *mut core::ffi::c_void = null_mut();
            let mut len = 0u32;
            let ok = unsafe {
                VerQueryValueW(block.as_ptr().cast(), sub.as_ptr(), &mut value, &mut len)
            };
            if ok == 0 || value.is_null() || len == 0 {
                return None;
            }
            // SAFETY: VerQueryValueW points `value` at `len` UTF-16 units inside `block`, which is alive.
            let units = unsafe { std::slice::from_raw_parts(value as *const u16, len as usize) };
            let text = String::from_utf16_lossy(units);
            let text = text.trim_end_matches('\0').trim().to_owned();
            (!text.is_empty()).then_some(text)
        };
        let mut tables: Vec<String> = Vec::new();
        let sub = wide("\\VarFileInfo\\Translation");
        let mut value: *mut core::ffi::c_void = null_mut();
        let mut len = 0u32;
        let found =
            unsafe { VerQueryValueW(block.as_ptr().cast(), sub.as_ptr(), &mut value, &mut len) };
        if found != 0 && !value.is_null() && len >= 4 {
            // SAFETY: the translation table is `len` bytes of (language, code page) u16 pairs inside `block`.
            let pairs =
                unsafe { std::slice::from_raw_parts(value as *const u16, (len / 2) as usize) };
            for pair in pairs.chunks_exact(2).take(4) {
                tables.push(format!("{:04x}{:04x}", pair[0], pair[1]));
            }
        }
        tables.extend(["040904b0".to_owned(), "040904e4".to_owned()]);
        tables
            .iter()
            .find_map(|t| query(&format!("\\StringFileInfo\\{t}\\FileDescription")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn ft(unix: u64) -> u64 {
        (unix + FILETIME_UNIX_OFFSET) * 10_000_000
    }

    #[derive(Default)]
    struct Fake {
        strings: HashMap<(String, String), String>,
        qwords: HashMap<(String, String), u64>,
        keys: HashMap<String, Vec<String>>,
        machine_strings: HashMap<(String, String), String>,
        machine_dwords: HashMap<(String, String), u32>,
        fail_writes: bool,
        drop_writes: bool,
    }

    fn id(path: &str, name: &str) -> (String, String) {
        (path.to_lowercase(), name.to_lowercase())
    }

    impl Fake {
        fn base(cap: Capability) -> String {
            base_path(cap)
        }
        fn app(&mut self, cap: Capability, key: &str, value: Option<&str>, times: (u64, u64)) {
            let base = Self::base(cap);
            self.keys.entry(base.clone()).or_default().push(key.into());
            let path = format!(r"{base}\{key}");
            if let Some(v) = value {
                self.strings.insert(id(&path, "Value"), v.into());
            }
            self.qwords.insert(id(&path, "LastUsedTimeStart"), times.0);
            self.qwords.insert(id(&path, "LastUsedTimeStop"), times.1);
        }
        fn desktop(&mut self, cap: Capability, key: &str, times: (u64, u64)) {
            let np = format!(r"{}\{NON_PACKAGED}", Self::base(cap));
            let base = Self::base(cap);
            let list = self.keys.entry(base).or_default();
            if !list.iter().any(|k| k == NON_PACKAGED) {
                list.push(NON_PACKAGED.into());
            }
            self.keys.entry(np.clone()).or_default().push(key.into());
            let path = format!(r"{np}\{key}");
            self.qwords.insert(id(&path, "LastUsedTimeStart"), times.0);
            self.qwords.insert(id(&path, "LastUsedTimeStop"), times.1);
        }
    }

    impl ConsentStore for Fake {
        fn subkeys(&self, path: &str, limit: usize) -> Vec<String> {
            self.keys
                .get(path)
                .map(|k| k.iter().take(limit).cloned().collect())
                .unwrap_or_default()
        }
        fn string(&self, path: &str, name: &str) -> Option<String> {
            self.strings.get(&id(path, name)).cloned()
        }
        fn qword(&self, path: &str, name: &str) -> Option<u64> {
            self.qwords.get(&id(path, name)).copied()
        }
        fn machine_string(&self, path: &str, name: &str) -> Option<String> {
            self.machine_strings.get(&id(path, name)).cloned()
        }
        fn machine_dword(&self, path: &str, name: &str) -> Option<u32> {
            self.machine_dwords.get(&id(path, name)).copied()
        }
        fn set_string(&mut self, path: &str, name: &str, value: &str) -> Result<()> {
            if self.fail_writes {
                bail!("denied");
            }
            if !self.drop_writes {
                self.strings.insert(id(path, name), value.into());
            }
            Ok(())
        }
    }

    const NOW: u64 = 1_800_000_000;
    const CAM: Capability = Capability::Camera;

    fn no_names(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn filetimes_become_unix_seconds() {
        assert_eq!(filetime_to_unix(0), None);
        assert_eq!(filetime_to_unix(ft(NOW)), Some(NOW));
        assert_eq!(filetime_to_unix(ft(0)), Some(0));
        assert_eq!(filetime_to_unix(10_000_000 * 100), None);
        assert_eq!(filetime_to_unix(u64::MAX), None);
    }

    #[test]
    fn friendly_names_for_store_apps() {
        for (key, name) in [
            ("Microsoft.WindowsCamera_8wekyb3d8bbwe", "Windows Camera"),
            ("Microsoft.Windows.Photos_8wekyb3d8bbwe", "Photos"),
            (
                "microsoft.windowscommunicationsapps_8wekyb3d8bbwe",
                "Mail and Calendar",
            ),
            ("SpotifyAB.SpotifyMusic_zpdnekdrzrea0", "Spotify Music"),
            ("Microsoft.BingNews_8wekyb3d8bbwe", "Bing News"),
            (
                "Microsoft.Windows.XGpuEjectDialog_cw5n1h2txyewy",
                "Windows X Gpu Eject Dialog",
            ),
            ("Microsoft.549981C3F5F10_8wekyb3d8bbwe", "Cortana"),
            ("Microsoft.Microsoft3DViewer_8wekyb3d8bbwe", "3D Viewer"),
            ("Microsoft.Office.OneNote_8wekyb3d8bbwe", "OneNote"),
            (
                "Microsoft.Win32WebViewHost_cw5n1h2txyewy",
                "Desktop App Web Viewer",
            ),
            ("Vendor.123ABCDEF45_abcdefghijklm", "123ABCDEF45"),
            ("weird_x", "weird"),
        ] {
            assert_eq!(package_name(key), name, "{key}");
        }
        assert!(package_name("A_b").len() <= MAX_NAME);
    }

    #[test]
    fn desktop_names_fall_back_to_the_file_name() {
        assert_eq!(file_stem(r"C:\Program Files\Zoom\bin\Zoom.exe"), "Zoom");
        assert_eq!(file_stem(r"C:\x\tool.EXE"), "tool");
        assert_eq!(file_stem(r"C:\x\readme.txt"), "readme.txt");
        assert_eq!(
            desktop_path("C:#Program Files#Zoom#Zoom.exe"),
            r"C:\Program Files\Zoom\Zoom.exe"
        );
        assert!(local_exe_path(r"C:\Program Files\Zoom\Zoom.exe"));
        assert!(!local_exe_path(r"\\server\share\x.exe"));
        assert!(!local_exe_path(r"C:\a\..\b.exe"));
        assert!(!local_exe_path("C:"));
    }

    #[test]
    fn wording_steps() {
        assert_eq!(recency(true, Some(NOW), NOW), Recency::InUse);
        assert_eq!(recency(false, None, NOW), Recency::Never);
        assert_eq!(recency(false, Some(NOW - 30), NOW), Recency::JustNow);
        assert_eq!(recency(false, Some(NOW + 500), NOW), Recency::JustNow);
        assert_eq!(recency(false, Some(NOW - 60), NOW), Recency::Minutes(1));
        assert_eq!(recency(false, Some(NOW - 3599), NOW), Recency::Minutes(59));
        assert_eq!(recency(false, Some(NOW - 3600), NOW), Recency::Hours(1));
        assert_eq!(recency(false, Some(NOW - 86_400), NOW), Recency::Days(1));
        assert_eq!(
            recency(false, Some(NOW - 86_400 * 9), NOW),
            Recency::Days(9)
        );
    }

    #[test]
    fn listing_is_read_sorted_and_named() {
        let mut fake = Fake::default();
        fake.strings
            .insert(id(&base_path(CAM), "Value"), "Allow".into());
        fake.app(
            CAM,
            "Microsoft.WindowsCamera_8wekyb3d8bbwe",
            Some("Allow"),
            (ft(NOW - 600), ft(NOW - 500)),
        );
        fake.app(
            CAM,
            "SpotifyAB.SpotifyMusic_zpdnekdrzrea0",
            Some("Deny"),
            (0, 0),
        );
        fake.app(
            CAM,
            "Microsoft.SkypeApp_kzf8qxf38zg5c",
            Some("Allow"),
            (ft(NOW - 10), 0),
        );
        fake.app(CAM, "Microsoft.BingNews_8wekyb3d8bbwe", None, (0, 0));
        fake.app(CAM, "not a package", Some("Allow"), (ft(NOW), 0));
        fake.desktop(
            CAM,
            "C:#Apps#Old#old.exe",
            (ft(NOW - 90_000), ft(NOW - 89_000)),
        );
        fake.desktop(CAM, "C:#Apps#Zoom#Zoom.exe", (ft(NOW - 100), ft(NOW - 50)));
        fake.desktop(CAM, "C:#Other#Zoom.exe", (ft(NOW - 5000), ft(NOW - 4000)));
        fake.desktop(CAM, "C:#Apps#unused#unused.exe", (0, 0));
        let listing = read_listing(&fake, CAM, None, &|path| {
            path.ends_with("Zoom.exe")
                .then(|| "Zoom Meetings".to_owned())
        });
        assert!(listing.master);
        let names: Vec<_> = listing.apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            ["Skype", "Windows Camera", "Spotify Music"],
            "in use first, then newest, then never used"
        );
        assert!(listing.apps[0].in_use);
        assert_eq!(listing.apps[1].last_used, Some(NOW - 500));
        assert!(!listing.apps[2].allowed);
        assert_eq!(listing.apps[2].last_used, None);
        let desktop: Vec<_> = listing.desktop.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(desktop, ["Zoom Meetings", "old"]);
        assert!(listing.desktop_allowed);
    }

    #[test]
    fn master_and_desktop_switches_read_deny() {
        let mut fake = Fake::default();
        fake.strings
            .insert(id(&base_path(CAM), "Value"), "Deny".into());
        fake.strings.insert(
            id(&format!(r"{}\{NON_PACKAGED}", base_path(CAM)), "Value"),
            "Deny".into(),
        );
        let listing = read_listing(&fake, CAM, None, &no_names);
        assert!(!listing.master);
        assert!(!listing.desktop_allowed);
        let other = read_listing(&fake, Capability::Location, None, &no_names);
        assert!(other.master, "a missing value is the Windows default, on");
        assert!(other.is_empty());
    }

    fn skype_tag() -> u8 {
        app_tag("Microsoft.SkypeApp_kzf8qxf38zg5c")
    }

    fn sample() -> (Fake, Listing) {
        let mut fake = Fake::default();
        fake.app(
            CAM,
            "Microsoft.SkypeApp_kzf8qxf38zg5c",
            Some("Allow"),
            (ft(NOW - 10), 0),
        );
        fake.desktop(CAM, "C:#Apps#Zoom#Zoom.exe", (ft(NOW - 100), ft(NOW - 50)));
        let listing = read_listing(&fake, CAM, None, &no_names);
        (fake, listing)
    }

    #[test]
    fn switches_write_allow_and_deny_where_they_belong() {
        let (mut fake, listing) = sample();
        let base = base_path(CAM);
        set_access(&mut fake, &listing, Target::Master, 0, false).unwrap();
        assert_eq!(fake.string(&base, "Value").as_deref(), Some("Deny"));
        set_access(&mut fake, &listing, Target::Master, 0, true).unwrap();
        assert_eq!(fake.string(&base, "Value").as_deref(), Some("Allow"));
        set_access(&mut fake, &listing, Target::App(0), skype_tag(), false).unwrap();
        assert_eq!(
            fake.string(
                &format!(r"{base}\Microsoft.SkypeApp_kzf8qxf38zg5c"),
                "Value"
            )
            .as_deref(),
            Some("Deny")
        );
        set_access(&mut fake, &listing, Target::DesktopApps, 0, false).unwrap();
        assert_eq!(
            fake.string(&format!(r"{base}\NonPackaged"), "Value")
                .as_deref(),
            Some("Deny")
        );
    }

    #[test]
    fn switches_refuse_targets_outside_the_listing() {
        let (mut fake, listing) = sample();
        assert_eq!(
            set_access(&mut fake, &listing, Target::App(1), 0, false),
            Err(SetError::Unknown)
        );
        assert_eq!(
            set_access(&mut fake, &listing, Target::App(250), 0, false),
            Err(SetError::Unknown)
        );
        let mut gone = Fake::default();
        assert_eq!(
            set_access(&mut gone, &listing, Target::App(0), skype_tag(), false),
            Err(SetError::Unknown)
        );
        assert_eq!(
            set_access(&mut gone, &listing, Target::DesktopApps, 0, false),
            Err(SetError::Unknown)
        );
        let mut forged = listing.clone();
        forged.apps[0].key = r"..\..\Run_x".into();
        assert_eq!(
            set_access(&mut fake, &forged, Target::App(0), skype_tag(), false),
            Err(SetError::Unknown)
        );
        let none = Listing::empty(CAM);
        assert_eq!(
            set_access(&mut fake, &none, Target::DesktopApps, 0, false),
            Err(SetError::Unknown)
        );
    }

    #[test]
    fn a_write_that_does_not_stick_is_a_failure() {
        let (mut fake, listing) = sample();
        fake.fail_writes = true;
        assert_eq!(
            set_access(&mut fake, &listing, Target::Master, 0, false),
            Err(SetError::Failed)
        );
        fake.fail_writes = false;
        fake.drop_writes = true;
        assert_eq!(
            set_access(&mut fake, &listing, Target::Master, 0, false),
            Err(SetError::Failed)
        );
    }

    #[test]
    fn set_bytes_round_trip_and_reject_junk() {
        for cap in Capability::ALL {
            for allow in [false, true] {
                for (target, tag) in [
                    (Target::Master, 0),
                    (Target::DesktopApps, 0),
                    (Target::App(0), 0),
                    (Target::App(100), 17),
                    (Target::App(MAX_APPS as u8 - 1), 31),
                ] {
                    let (lo, hi) = encode_set(cap, target, tag, allow);
                    assert_eq!(decode_set(lo, hi), Some((cap, target, tag, allow)));
                }
            }
        }
        assert_eq!(decode_set(3, 0), None);
        assert_eq!(decode_set(0, 255), None);
        assert_eq!(decode_set(0, 202), None);
        assert_eq!(decode_set(0, 201), Some((CAM, Target::App(199), 0, false)));
        assert_eq!(decode_set(1 << 3, 0), None, "only an app carries a tag");
        assert_eq!(decode_set(1 << 3, 1), None);
        assert!(decode_set(1 << 3, 2).is_some());
    }

    #[test]
    fn a_switch_for_a_different_app_than_the_one_clicked_is_refused() {
        let (mut fake, listing) = sample();
        let wrong = (skype_tag() + 1) & 0b1_1111;
        assert_eq!(
            set_access(&mut fake, &listing, Target::App(0), wrong, false),
            Err(SetError::Unknown)
        );
        let base = base_path(CAM);
        assert_eq!(
            fake.string(
                &format!(r"{base}\Microsoft.SkypeApp_kzf8qxf38zg5c"),
                "Value"
            ),
            Some("Allow".to_owned())
        );
        let other = [
            "Microsoft.WindowsCamera_8wekyb3d8bbwe",
            "SpotifyAB.SpotifyMusic_zpdnekdrzrea0",
            "Microsoft.SkypeApp_kzf8qxf38zg5c",
        ]
        .map(app_tag);
        assert!(
            other.iter().collect::<std::collections::HashSet<_>>().len() > 1,
            "the tag tells apps apart"
        );
        assert!(other.iter().all(|t| *t < 32));
    }

    #[test]
    fn an_app_left_running_by_a_crash_or_power_loss_is_not_in_use() {
        let mut fake = Fake::default();
        fake.app(
            CAM,
            "Microsoft.SkypeApp_kzf8qxf38zg5c",
            Some("Allow"),
            (ft(NOW - 5000), 0),
        );
        fake.app(
            CAM,
            "Microsoft.WindowsCamera_8wekyb3d8bbwe",
            Some("Allow"),
            (ft(NOW - 50), 0),
        );
        fake.desktop(CAM, "C:#Apps#Zoom#Zoom.exe", (ft(NOW - 5000), 0));
        let boot = Some(NOW - 1000);
        let listing = read_listing(&fake, CAM, boot, &no_names);
        let by_name = |n: &str| listing.apps.iter().find(|a| a.name == n).unwrap();
        assert!(by_name("Windows Camera").in_use);
        let skype = by_name("Skype");
        assert!(!skype.in_use);
        assert_eq!(skype.last_used, Some(NOW - 5000));
        assert_eq!(skype.recency(NOW), Recency::Hours(1));
        assert!(!listing.desktop[0].in_use);
        assert_eq!(
            listing.apps[0].name, "Windows Camera",
            "the live one sorts first"
        );
        let unknown = read_listing(&fake, CAM, None, &no_names);
        assert!(unknown.apps.iter().all(|a| a.in_use));
    }

    #[test]
    fn the_main_switch_shows_what_windows_really_allows() {
        let (mut fake, listing) = sample();
        assert!(listing.master);
        let base = base_path(CAM);
        fake.machine_strings
            .insert(id(&base, "Value"), "Deny".into());
        assert!(
            !read_listing(&fake, CAM, None, &no_names).master,
            "off for every account"
        );

        fake.machine_strings
            .insert(id(&base, "Value"), "Allow".into());
        fake.machine_dwords
            .insert(id(APP_PRIVACY_POLICY, "LetAppsAccessCamera"), 2);
        assert!(
            !read_listing(&fake, CAM, None, &no_names).master,
            "forced off"
        );

        fake.strings.insert(id(&base, "Value"), "Deny".into());
        fake.machine_dwords
            .insert(id(APP_PRIVACY_POLICY, "LetAppsAccessCamera"), 1);
        assert!(
            read_listing(&fake, CAM, None, &no_names).master,
            "forced on"
        );
    }

    #[test]
    fn a_setting_the_pc_or_organization_controls_is_marked() {
        let (mut fake, listing) = sample();
        assert!(!listing.controlled);
        let base = base_path(CAM);
        fake.machine_strings
            .insert(id(&base, "Value"), "Allow".into());
        fake.machine_dwords
            .insert(id(APP_PRIVACY_POLICY, "LetAppsAccessCamera"), 0);
        assert!(!read_listing(&fake, CAM, None, &no_names).controlled);

        fake.machine_strings
            .insert(id(&base, "Value"), "Deny".into());
        let off = read_listing(&fake, CAM, None, &no_names);
        assert!(off.controlled);
        assert_eq!(
            set_access(&mut fake, &off, Target::Master, 0, false),
            Err(SetError::Controlled)
        );
        assert_eq!(fake.string(&base, "Value"), None, "nothing was written");
        assert!(!read_listing(&fake, Capability::Location, None, &no_names).controlled);

        fake.machine_strings
            .insert(id(&base, "Value"), "Allow".into());
        for forced in [1, 2] {
            fake.machine_dwords
                .insert(id(APP_PRIVACY_POLICY, "LetAppsAccessCamera"), forced);
            assert!(read_listing(&fake, CAM, None, &no_names).controlled);
        }
        fake.machine_dwords
            .insert(id(APP_PRIVACY_POLICY, "LetAppsAccessMicrophone"), 2);
        assert!(read_listing(&fake, Capability::Microphone, None, &no_names).controlled);
    }

    #[test]
    fn the_desktop_app_names_stay_short() {
        let (shown, more) = names_summary(&["Zoom", "Firefox", "OBS Studio", "Discord", "Teams"]);
        assert_eq!(shown, ["Zoom", "Firefox", "OBS Studio"]);
        assert_eq!(more, 2);
        let long = "x".repeat(MAX_NAME);
        let (shown, more) = names_summary(&[&long, &long, "Zoom"]);
        assert_eq!(
            (shown.len(), more),
            (1, 2),
            "one long name uses the whole line"
        );
        let (shown, more) = names_summary(&["Zoom"]);
        assert_eq!((shown.len(), more), (1, 0));
        assert_eq!(names_summary(&[]), (Vec::<String>::new(), 0));
        let total: usize = shown.iter().map(|n| n.chars().count()).sum();
        assert!(total <= NAMES_BUDGET);
    }

    #[test]
    fn handoff_files_round_trip_and_are_cleaned() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("app-access-webcam.json");
        let (_, mut listing) = sample();
        write_handoff(&path, &listing).unwrap();
        assert_eq!(take_handoff(&path, CAM).unwrap(), listing);
        assert!(!path.exists(), "the file is removed once read");

        listing.apps[0].name = format!("Evil\u{7}\n{}", "x".repeat(500));
        listing.apps.push(Entry {
            key: "bad key".into(),
            name: "x".into(),
            allowed: true,
            in_use: false,
            last_used: None,
        });
        write_handoff(&path, &listing).unwrap();
        let read = take_handoff(&path, CAM).unwrap();
        assert_eq!(read.apps.len(), 1);
        assert!(read.apps[0].name.chars().count() <= MAX_NAME);
        assert!(!read.apps[0].name.chars().any(char::is_control));

        write_handoff(&path, &listing).unwrap();
        assert!(take_handoff(&path, Capability::Location).is_err());
        std::fs::write(&path, b"not json").unwrap();
        assert!(take_handoff(&path, CAM).is_err());
        std::fs::write(&path, vec![b' '; MAX_HANDOFF as usize + 10]).unwrap();
        assert!(take_handoff(&path, CAM).is_err());
        assert!(take_handoff(&path, CAM).is_err(), "nothing left to read");
    }
}
