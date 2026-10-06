//! Per-user (HKCU) settings, read and changed by the unelevated launcher.
//!
//! The elevated GUI cannot touch the signed-in person's own registry hive
//! (the administrator's hive may be a different account), so it asks the
//! launcher through the broker. A request carries only a [`Setting`] and an
//! [`Op`]; every registry path, value name and value lives in this file.
//!
//! Every change reads first, journals the prior value (in the person's own
//! `%LOCALAPPDATA%\Secblitz`), writes, and reads again. If the value did not
//! stick, the prior value is put back and the change counts as failed. Undo
//! restores the journalled value exactly (including "was not set at all").
#![cfg_attr(not(windows), allow(dead_code))]

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The closed set of per-user settings. The wire byte is the position in
/// [`Setting::ALL`] and never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// `smartscreen.store_apps`
    StoreAppsWebCheck,
    /// `files.show_extensions`
    ShowExtensions,
    /// `net.nearby_sharing`
    NearbySharing,
    /// `privacy.tailored_experiences`
    TailoredExperiences,
    /// `office.internet_macros`
    OfficeMacros,
    /// `debloat.suggested_apps`: Windows' own "suggestions" in the Start menu
    /// and Settings. Not listed on the personal page (the Apps page owns it).
    SuggestedApps,
}

impl Setting {
    pub const ALL: [Setting; 6] = [
        Setting::StoreAppsWebCheck,
        Setting::ShowExtensions,
        Setting::NearbySharing,
        Setting::TailoredExperiences,
        Setting::OfficeMacros,
        Setting::SuggestedApps,
    ];

    /// The settings the personal page lists: everything except
    /// `SuggestedApps`, which has its own place on the Apps page.
    pub const PERSONAL: [Setting; 5] = [
        Setting::StoreAppsWebCheck,
        Setting::ShowExtensions,
        Setting::NearbySharing,
        Setting::TailoredExperiences,
        Setting::OfficeMacros,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Setting::StoreAppsWebCheck => "smartscreen.store_apps",
            Setting::ShowExtensions => "files.show_extensions",
            Setting::NearbySharing => "net.nearby_sharing",
            Setting::TailoredExperiences => "privacy.tailored_experiences",
            Setting::OfficeMacros => "office.internet_macros",
            Setting::SuggestedApps => "debloat.suggested_apps",
        }
    }

    pub fn to_byte(self) -> u8 {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0) as u8
    }

    pub fn from_byte(byte: u8) -> Option<Self> {
        Self::ALL.get(usize::from(byte)).copied()
    }
}

/// What the GUI asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Read only.
    Query,
    /// Move to the safer value.
    Apply,
    /// Put the journalled prior value back.
    Undo,
}

impl Op {
    pub fn to_byte(self) -> u8 {
        match self {
            Op::Query => 0,
            Op::Apply => 1,
            Op::Undo => 2,
        }
    }
    pub fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            0 => Op::Query,
            1 => Op::Apply,
            2 => Op::Undo,
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// Registry access (a trait so the logic is testable on any host)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    CurrentUser,
    Machine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Absent,
    Dword(u32),
    /// Present, but not a plain number: never overwritten.
    Other,
}

pub trait Registry {
    fn get(&self, hive: Hive, key: &str, name: &str) -> Result<Value>;
    /// Current-user hive only.
    fn set_dword(&mut self, key: &str, name: &str, value: u32) -> Result<()>;
    /// Current-user hive only; a value that is already gone is fine.
    fn delete(&mut self, key: &str, name: &str) -> Result<()>;
    fn get_string(&self, hive: Hive, key: &str, name: &str) -> Option<String>;
    fn subkeys(&self, hive: Hive, key: &str) -> Vec<String>;
    fn key_exists(&self, hive: Hive, key: &str) -> bool;
    /// Tell Explorer that file-type display settings changed.
    fn notify_file_view_changed(&mut self) {}
}

// ---------------------------------------------------------------------------
// The allowlist: every key this module may touch
// ---------------------------------------------------------------------------

struct Target {
    key: String,
    name: &'static str,
    safe: u32,
    /// A missing value already behaves like `safe`.
    absent_safe: bool,
}

fn target(key: &str, name: &'static str, safe: u32, absent_safe: bool) -> Target {
    Target {
        key: key.to_owned(),
        name,
        safe,
        absent_safe,
    }
}

const OFFICE_APPS: [(&str, &str); 3] = [
    ("word", "Word"),
    ("excel", "Excel"),
    ("powerpoint", "PowerPoint"),
];
const CONTENT_DELIVERY: &str = r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager";
const SUGGESTION_VALUES: [&str; 7] = [
    "SilentInstalledAppsEnabled",
    "PreInstalledAppsEnabled",
    "OemPreInstalledAppsEnabled",
    "SubscribedContent-338388Enabled",
    "SubscribedContent-338389Enabled",
    "SubscribedContent-353694Enabled",
    "SubscribedContent-353696Enabled",
];
const PV_VALUES: [&str; 3] = [
    "DisableInternetFilesInPV",
    "DisableAttachmentsInPV",
    "DisableUnsafeLocationsInPV",
];

fn targets(setting: Setting) -> Vec<Target> {
    match setting {
        Setting::StoreAppsWebCheck => vec![target(
            r"Software\Microsoft\Windows\CurrentVersion\AppHost",
            "EnableWebContentEvaluation",
            1,
            false,
        )],
        Setting::ShowExtensions => vec![target(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
            "HideFileExt",
            0,
            false,
        )],
        // 2 = everyone nearby. Only that value is ever changed (to 1).
        Setting::NearbySharing => vec![target(
            r"Software\Microsoft\Windows\CurrentVersion\CDP",
            "CdpSessionUserAuthzPolicy",
            1,
            false,
        )],
        Setting::TailoredExperiences => vec![target(
            r"Software\Microsoft\Windows\CurrentVersion\Privacy",
            "TailoredExperiencesWithDiagnosticDataEnabled",
            0,
            false,
        )],
        Setting::OfficeMacros => {
            let mut out = Vec::new();
            for (policy, display) in OFFICE_APPS {
                out.push(target(
                    &format!(r"Software\Policies\Microsoft\Office\16.0\{policy}\security"),
                    "blockcontentexecutionfrominternet",
                    1,
                    false,
                ));
                for name in PV_VALUES {
                    out.push(target(
                        &format!(
                            r"Software\Microsoft\Office\16.0\{display}\Security\ProtectedView"
                        ),
                        name,
                        0,
                        true,
                    ));
                    out.push(target(
                        &format!(
                            r"Software\Policies\Microsoft\Office\16.0\{policy}\security\protectedview"
                        ),
                        name,
                        0,
                        true,
                    ));
                }
            }
            out
        }
        // A missing value lets Windows suggest, so absent is not safe.
        Setting::SuggestedApps => SUGGESTION_VALUES
            .iter()
            .map(|name| target(CONTENT_DELIVERY, name, 0, false))
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Safe,
    Unsafe,
    /// Nothing to do on this PC (for example Office is not installed).
    NotApplicable,
    /// Could not be read, or has a value we do not recognise. Never "safe".
    Unknown,
}

fn office_installed(reg: &dyn Registry) -> bool {
    reg.key_exists(
        Hive::Machine,
        r"SOFTWARE\Microsoft\Office\ClickToRun\Configuration",
    ) || reg.key_exists(
        Hive::Machine,
        r"SOFTWARE\Microsoft\Office\16.0\Common\InstallRoot",
    ) || reg.key_exists(
        Hive::Machine,
        r"SOFTWARE\WOW6432Node\Microsoft\Office\16.0\Common\InstallRoot",
    )
}

/// Domain-joined or work/school-managed PCs get Office policy from their
/// organisation; the policy path is not ours to write there.
fn managed_by_organisation(reg: &dyn Registry) -> bool {
    let domain = reg
        .get_string(
            Hive::Machine,
            r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters",
            "Domain",
        )
        .is_some_and(|d| !d.trim().is_empty());
    if domain {
        return true;
    }
    reg.subkeys(Hive::Machine, r"SOFTWARE\Microsoft\Enrollments")
        .iter()
        .any(|sub| {
            let key = format!(r"SOFTWARE\Microsoft\Enrollments\{sub}");
            reg.get_string(Hive::Machine, &key, "ProviderID")
                .is_some_and(|p| !p.trim().is_empty())
        })
}

pub fn status(reg: &dyn Registry, setting: Setting) -> Status {
    if setting == Setting::OfficeMacros && !office_installed(reg) {
        return Status::NotApplicable;
    }
    let mut unsafe_seen = false;
    let mut unknown_seen = false;
    for t in targets(setting) {
        match reg.get(Hive::CurrentUser, &t.key, t.name) {
            Err(_) => unknown_seen = true,
            Ok(Value::Other) => unknown_seen = true,
            Ok(Value::Absent) => {
                if setting == Setting::NearbySharing {
                    // Default not documented: say nothing rather than guess.
                    unknown_seen = true;
                } else if !t.absent_safe {
                    unsafe_seen = true;
                }
            }
            Ok(Value::Dword(v)) => {
                if setting == Setting::NearbySharing {
                    match v {
                        0 | 1 => {}
                        2 => unsafe_seen = true,
                        _ => unknown_seen = true,
                    }
                } else if v != t.safe {
                    unsafe_seen = true;
                }
            }
        }
    }
    if unknown_seen {
        Status::Unknown
    } else if unsafe_seen {
        Status::Unsafe
    } else {
        Status::Safe
    }
}

/// What the GUI is told for a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Report {
    Safe,
    /// Safe because Secblitz changed it: undo is available.
    SafeByUs,
    Unsafe,
    NotApplicable,
    Unknown,
}

pub fn report(reg: &dyn Registry, journal: &Path, setting: Setting) -> Report {
    match status(reg, setting) {
        Status::Safe => {
            if load_journal(journal).settings.contains_key(setting.id()) {
                Report::SafeByUs
            } else {
                Report::Safe
            }
        }
        Status::Unsafe => Report::Unsafe,
        Status::NotApplicable => Report::NotApplicable,
        Status::Unknown => Report::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Journal
// ---------------------------------------------------------------------------

/// Journals are tiny; anything bigger is not ours.
const JOURNAL_LIMIT: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Prior {
    /// Index into the setting's target list.
    i: usize,
    /// The value before the change; `None` = the value did not exist.
    prior: Option<u32>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Journal {
    #[serde(default)]
    v: u32,
    #[serde(default)]
    settings: BTreeMap<String, Vec<Prior>>,
}

/// `%LOCALAPPDATA%\Secblitz\user-settings.json` (the signed-in person's own).
pub fn journal_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty())?;
    Some(
        PathBuf::from(base)
            .join("Secblitz")
            .join("user-settings.json"),
    )
}

fn load_journal(path: &Path) -> Journal {
    use std::io::Read;
    let Ok(file) = std::fs::File::open(path) else {
        return Journal::default();
    };
    let mut bytes = Vec::new();
    if file
        .take(JOURNAL_LIMIT + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > JOURNAL_LIMIT
    {
        return Journal::default();
    }
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_journal(path: &Path, journal: &mut Journal) -> Result<()> {
    use std::io::Write;
    journal.v = 1;
    let dir = path.parent().ok_or_else(|| anyhow::anyhow!("no folder"))?;
    std::fs::create_dir_all(dir)?;
    let tmp = path.with_extension("json.tmp");
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(&serde_json::to_vec_pretty(journal)?)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Apply / undo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Done,
    /// Tried and it did not stick (nothing is left half-changed).
    Failed,
    /// Not offered here: managed PC, nothing to change, or nothing to undo.
    Blocked,
    /// Undo only: the person (or something else) changed the value after
    /// Secblitz did, so it was left exactly as it is.
    ChangedSince,
}

fn write_prior(reg: &mut dyn Registry, t: &Target, prior: Option<u32>) -> Result<()> {
    match prior {
        Some(v) => reg.set_dword(&t.key, t.name, v),
        None => reg.delete(&t.key, t.name),
    }
}

fn matches_prior(reg: &dyn Registry, t: &Target, prior: Option<u32>) -> bool {
    match (reg.get(Hive::CurrentUser, &t.key, t.name), prior) {
        (Ok(Value::Absent), None) => true,
        (Ok(Value::Dword(v)), Some(p)) => v == p,
        _ => false,
    }
}

fn restore(reg: &mut dyn Registry, setting: Setting, priors: &[Prior]) -> bool {
    let all = targets(setting);
    let mut ok = true;
    for p in priors {
        let Some(t) = all.get(p.i) else {
            ok = false;
            continue;
        };
        ok &= write_prior(reg, t, p.prior).is_ok() && matches_prior(reg, t, p.prior);
    }
    ok
}

/// What undo found for each recorded value.
#[derive(Debug, Default, PartialEq, Eq)]
struct Undone {
    /// Every value that is still ours is back (or already was).
    ok: bool,
    /// At least one value was changed by someone else and left alone.
    drifted: bool,
}

/// Put back only values that still hold what Secblitz wrote (`safe`). A value
/// that already equals the recorded prior is fine; anything else was changed
/// since, and is never overwritten.
fn restore_unless_changed(reg: &mut dyn Registry, setting: Setting, priors: &[Prior]) -> Undone {
    let all = targets(setting);
    let mut out = Undone {
        ok: true,
        drifted: false,
    };
    for p in priors {
        let Some(t) = all.get(p.i) else {
            out.ok = false;
            continue;
        };
        if matches_prior(reg, t, p.prior) {
            continue;
        }
        match reg.get(Hive::CurrentUser, &t.key, t.name) {
            Ok(Value::Dword(v)) if v == t.safe => {
                out.ok &= write_prior(reg, t, p.prior).is_ok() && matches_prior(reg, t, p.prior);
            }
            Ok(_) => out.drifted = true,
            Err(_) => out.ok = false,
        }
    }
    out
}

pub fn apply(reg: &mut dyn Registry, journal: &Path, setting: Setting) -> Outcome {
    if setting == Setting::OfficeMacros && managed_by_organisation(reg) {
        return Outcome::Blocked;
    }
    match status(reg, setting) {
        Status::Safe => return Outcome::Done,
        Status::Unsafe => {}
        Status::NotApplicable | Status::Unknown => return Outcome::Blocked,
    }
    let all = targets(setting);
    // Read before write: remember exactly what is there now.
    let mut priors = Vec::new();
    for (i, t) in all.iter().enumerate() {
        let current = match reg.get(Hive::CurrentUser, &t.key, t.name) {
            Ok(Value::Absent) => None,
            Ok(Value::Dword(v)) => Some(v),
            _ => return Outcome::Failed,
        };
        let needs_change = match current {
            None => !t.absent_safe,
            Some(v) => v != t.safe,
        };
        if needs_change {
            priors.push(Prior { i, prior: current });
        }
    }
    if priors.is_empty() {
        return Outcome::Failed;
    }
    // Journal first: if it cannot be saved, nothing is changed.
    let mut stored = load_journal(journal);
    // An earlier run (even one cut short) may have already moved other values
    // of this setting to safe. Their original values stay on record, so one
    // undo still puts everything back.
    if let Some(earlier) = stored.settings.get(setting.id()) {
        for old in earlier {
            let still_ours = all.get(old.i).is_some_and(|t| {
                matches!(reg.get(Hive::CurrentUser, &t.key, t.name), Ok(Value::Dword(v)) if v == t.safe)
            });
            if still_ours && !priors.iter().any(|p| p.i == old.i) {
                priors.push(old.clone());
            }
        }
        priors.sort_by_key(|p| p.i);
    }
    stored
        .settings
        .insert(setting.id().to_owned(), priors.clone());
    if save_journal(journal, &mut stored).is_err() {
        return Outcome::Failed;
    }
    let mut wrote = Ok(());
    for p in &priors {
        if let Err(e) = reg.set_dword(&all[p.i].key, all[p.i].name, all[p.i].safe) {
            wrote = Err(e);
            break;
        }
    }
    // Read again: only a value that reads back as safe counts.
    if wrote.is_err() || status(reg, setting) != Status::Safe {
        restore(reg, setting, &priors);
        stored.settings.remove(setting.id());
        let _ = save_journal(journal, &mut stored);
        return Outcome::Failed;
    }
    if setting == Setting::ShowExtensions {
        reg.notify_file_view_changed();
    }
    Outcome::Done
}

pub fn undo(reg: &mut dyn Registry, journal: &Path, setting: Setting) -> Outcome {
    let mut stored = load_journal(journal);
    let Some(priors) = stored.settings.get(setting.id()).cloned() else {
        return Outcome::Blocked;
    };
    let all = targets(setting);
    if priors.iter().any(|p| p.i >= all.len()) {
        // Not a journal we wrote: drop it rather than act on it.
        stored.settings.remove(setting.id());
        let _ = save_journal(journal, &mut stored);
        return Outcome::Blocked;
    }
    let undone = restore_unless_changed(reg, setting, &priors);
    if !undone.ok {
        return Outcome::Failed;
    }
    stored.settings.remove(setting.id());
    if save_journal(journal, &mut stored).is_err() {
        // The value is back; a stale journal entry only offers a second undo.
        return if undone.drifted {
            Outcome::ChangedSince
        } else {
            Outcome::Done
        };
    }
    if setting == Setting::ShowExtensions {
        reg.notify_file_view_changed();
    }
    if undone.drifted {
        Outcome::ChangedSince
    } else {
        Outcome::Done
    }
}

/// Settings Secblitz changed and can still put back, in [`Setting::ALL`] order.
#[allow(dead_code)] // used by the uninstall flow
pub fn undoable(journal: &Path) -> Vec<Setting> {
    let stored = load_journal(journal);
    Setting::ALL
        .into_iter()
        .filter(|s| stored.settings.contains_key(s.id()))
        .collect()
}

/// Put back every journalled setting (the values are independent, so the
/// order does not matter). A failure on one does not stop the others.
#[allow(dead_code)] // used by the uninstall flow
pub fn undo_all(reg: &mut dyn Registry, journal: &Path) -> Vec<(Setting, Outcome)> {
    undoable(journal)
        .into_iter()
        .map(|setting| (setting, undo(reg, journal, setting)))
        .collect()
}

/// One broker request, end to end.
pub fn handle(
    reg: &mut dyn Registry,
    journal: Option<&Path>,
    setting: Setting,
    op: Op,
) -> Result<HandleResult> {
    let Some(journal) = journal else {
        bail!("no per-user data folder");
    };
    Ok(match op {
        Op::Query => HandleResult::Report(report(reg, journal, setting)),
        Op::Apply => HandleResult::Outcome(apply(reg, journal, setting)),
        Op::Undo => HandleResult::Outcome(undo(reg, journal, setting)),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleResult {
    Report(Report),
    Outcome(Outcome),
}

// ---------------------------------------------------------------------------
// The real registry (Windows)
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub struct SystemRegistry;

#[cfg(windows)]
mod sys {
    use super::{Hive, Registry, Value};
    use anyhow::{bail, Result};
    use std::ptr::null_mut;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegEnumKeyExW, RegOpenKeyExW,
        RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ,
        KEY_SET_VALUE, REG_DWORD, REG_SZ,
    };

    const NOT_FOUND: u32 = 2;
    const PATH_NOT_FOUND: u32 = 3;
    const MORE_DATA: u32 = 234;
    const NO_MORE_ITEMS: u32 = 259;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn root(hive: Hive) -> HKEY {
        match hive {
            Hive::CurrentUser => HKEY_CURRENT_USER,
            Hive::Machine => HKEY_LOCAL_MACHINE,
        }
    }

    /// `Ok(None)` when the key does not exist.
    fn open(hive: Hive, key: &str, access: u32) -> Result<Option<HKEY>> {
        let path = wide(key);
        let mut handle: HKEY = null_mut();
        let status = unsafe { RegOpenKeyExW(root(hive), path.as_ptr(), 0, access, &mut handle) };
        match status {
            0 => Ok(Some(handle)),
            NOT_FOUND | PATH_NOT_FOUND => Ok(None),
            other => bail!("cannot open the key ({other})"),
        }
    }

    impl Registry for super::SystemRegistry {
        fn get(&self, hive: Hive, key: &str, name: &str) -> Result<Value> {
            let Some(handle) = open(hive, key, KEY_READ)? else {
                return Ok(Value::Absent);
            };
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
            match status {
                0 if kind == REG_DWORD && size == 4 => Ok(Value::Dword(u32::from_le_bytes(data))),
                0 | MORE_DATA => Ok(Value::Other),
                NOT_FOUND | PATH_NOT_FOUND => Ok(Value::Absent),
                other => bail!("cannot read the value ({other})"),
            }
        }

        fn set_dword(&mut self, key: &str, name: &str, value: u32) -> Result<()> {
            let path = wide(key);
            let mut handle: HKEY = null_mut();
            let status = unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    path.as_ptr(),
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
            let bytes = value.to_le_bytes();
            let status =
                unsafe { RegSetValueExW(handle, name.as_ptr(), 0, REG_DWORD, bytes.as_ptr(), 4) };
            unsafe { RegCloseKey(handle) };
            if status != 0 {
                bail!("cannot write the value ({status})");
            }
            Ok(())
        }

        fn delete(&mut self, key: &str, name: &str) -> Result<()> {
            let Some(handle) = open(Hive::CurrentUser, key, KEY_SET_VALUE)? else {
                return Ok(());
            };
            let name = wide(name);
            let status = unsafe { RegDeleteValueW(handle, name.as_ptr()) };
            unsafe { RegCloseKey(handle) };
            match status {
                0 | NOT_FOUND | PATH_NOT_FOUND => Ok(()),
                other => bail!("cannot remove the value ({other})"),
            }
        }

        fn get_string(&self, hive: Hive, key: &str, name: &str) -> Option<String> {
            let handle = open(hive, key, KEY_READ).ok()??;
            let name = wide(name);
            let mut kind = 0u32;
            let mut buf = [0u16; 512];
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
            let text = String::from_utf16_lossy(&buf[..len]);
            Some(text.trim_end_matches('\0').to_owned())
        }

        fn subkeys(&self, hive: Hive, key: &str) -> Vec<String> {
            let Ok(Some(handle)) = open(hive, key, KEY_READ) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            for index in 0..256u32 {
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
                if status == NO_MORE_ITEMS || status != 0 {
                    break;
                }
                out.push(String::from_utf16_lossy(&buf[..len as usize]));
            }
            unsafe { RegCloseKey(handle) };
            out
        }

        fn key_exists(&self, hive: Hive, key: &str) -> bool {
            match open(hive, key, KEY_READ) {
                Ok(Some(handle)) => {
                    unsafe { RegCloseKey(handle) };
                    true
                }
                _ => false,
            }
        }

        fn notify_file_view_changed(&mut self) {
            use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
            unsafe {
                SHChangeNotify(
                    SHCNE_ASSOCCHANGED as i32,
                    SHCNF_IDLIST,
                    std::ptr::null(),
                    std::ptr::null(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// In-memory registry; `fail_writes` simulates a value that will not stick.
    #[derive(Default)]
    struct Fake {
        cu: HashMap<(String, String), Value>,
        lm: HashMap<(String, String), Value>,
        strings: HashMap<(String, String), String>,
        keys: Vec<String>,
        subs: HashMap<String, Vec<String>>,
        fail_writes: bool,
        silent_drop: bool,
        notified: u32,
    }

    fn k(key: &str, name: &str) -> (String, String) {
        (key.to_lowercase(), name.to_lowercase())
    }

    impl Fake {
        fn put(&mut self, setting: Setting, index: usize, v: Option<u32>) {
            let t = &targets(setting)[index];
            let key = k(&t.key, t.name);
            match v {
                Some(v) => self.cu.insert(key, Value::Dword(v)),
                None => self.cu.remove(&key),
            };
        }
        fn read(&self, setting: Setting, index: usize) -> Value {
            let t = &targets(setting)[index];
            self.get(Hive::CurrentUser, &t.key, t.name).unwrap()
        }
    }

    impl Registry for Fake {
        fn get(&self, hive: Hive, key: &str, name: &str) -> Result<Value> {
            let map = if hive == Hive::CurrentUser {
                &self.cu
            } else {
                &self.lm
            };
            Ok(map.get(&k(key, name)).copied().unwrap_or(Value::Absent))
        }
        fn set_dword(&mut self, key: &str, name: &str, value: u32) -> Result<()> {
            if self.fail_writes {
                bail!("denied");
            }
            if !self.silent_drop {
                self.cu.insert(k(key, name), Value::Dword(value));
            }
            Ok(())
        }
        fn delete(&mut self, key: &str, name: &str) -> Result<()> {
            self.cu.remove(&k(key, name));
            Ok(())
        }
        fn get_string(&self, hive: Hive, key: &str, name: &str) -> Option<String> {
            assert_eq!(hive, Hive::Machine);
            self.strings.get(&k(key, name)).cloned()
        }
        fn subkeys(&self, _: Hive, key: &str) -> Vec<String> {
            self.subs.get(key).cloned().unwrap_or_default()
        }
        fn key_exists(&self, _: Hive, key: &str) -> bool {
            self.keys.iter().any(|x| x.eq_ignore_ascii_case(key))
        }
        fn notify_file_view_changed(&mut self) {
            self.notified += 1;
        }
    }

    fn journal() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Secblitz").join("user-settings.json");
        (dir, path)
    }

    #[test]
    fn wire_bytes_are_stable_and_closed() {
        for (i, s) in Setting::ALL.iter().enumerate() {
            assert_eq!(s.to_byte() as usize, i);
            assert_eq!(Setting::from_byte(i as u8), Some(*s));
        }
        assert_eq!(Setting::from_byte(6), None);
        assert_eq!(Setting::from_byte(255), None);
        for op in [Op::Query, Op::Apply, Op::Undo] {
            assert_eq!(Op::from_byte(op.to_byte()), Some(op));
        }
        assert_eq!(Op::from_byte(3), None);
        let ids: Vec<_> = Setting::ALL.iter().map(|s| s.id()).collect();
        assert_eq!(
            ids,
            [
                "smartscreen.store_apps",
                "files.show_extensions",
                "net.nearby_sharing",
                "privacy.tailored_experiences",
                "office.internet_macros",
                "debloat.suggested_apps"
            ]
        );
    }

    #[test]
    fn allowlist_only_names_current_user_paths_we_know() {
        for s in Setting::ALL {
            for t in targets(s) {
                assert!(t.key.starts_with(r"Software\"), "{}", t.key);
                assert!(!t.key.contains(".."));
                assert!(t.safe <= 1);
            }
        }
        // 3 apps x (1 macro value + 3 values x 2 locations)
        assert_eq!(targets(Setting::OfficeMacros).len(), 21);
        assert_eq!(targets(Setting::NearbySharing).len(), 1);
    }

    #[test]
    fn show_extensions_apply_then_undo_restores_exact_value() {
        let (_d, path) = journal();
        let mut reg = Fake::default();
        reg.put(Setting::ShowExtensions, 0, Some(1));
        assert_eq!(status(&reg, Setting::ShowExtensions), Status::Unsafe);
        assert_eq!(
            apply(&mut reg, &path, Setting::ShowExtensions),
            Outcome::Done
        );
        assert_eq!(reg.read(Setting::ShowExtensions, 0), Value::Dword(0));
        assert_eq!(reg.notified, 1);
        assert_eq!(
            report(&reg, &path, Setting::ShowExtensions),
            Report::SafeByUs
        );
        assert_eq!(
            undo(&mut reg, &path, Setting::ShowExtensions),
            Outcome::Done
        );
        assert_eq!(reg.read(Setting::ShowExtensions, 0), Value::Dword(1));
        assert_eq!(reg.notified, 2);
        assert_eq!(report(&reg, &path, Setting::ShowExtensions), Report::Unsafe);
        // Nothing left to undo.
        assert_eq!(
            undo(&mut reg, &path, Setting::ShowExtensions),
            Outcome::Blocked
        );
    }

    #[test]
    fn undo_leaves_a_value_changed_since_alone_and_says_so() {
        let (_d, path) = journal();
        let mut reg = Fake::default();
        reg.put(Setting::ShowExtensions, 0, Some(1));
        assert_eq!(apply(&mut reg, &path, Setting::ShowExtensions), Outcome::Done);
        // The person changed it again by hand (to something we did not write).
        reg.put(Setting::ShowExtensions, 0, Some(7));
        assert_eq!(
            undo(&mut reg, &path, Setting::ShowExtensions),
            Outcome::ChangedSince
        );
        assert_eq!(reg.read(Setting::ShowExtensions, 0), Value::Dword(7));
        // The stale record is gone, so it never claims the value is ours.
        assert!(!load_journal(&path).settings.contains_key("files.show_extensions"));
        assert_eq!(
            undo(&mut reg, &path, Setting::ShowExtensions),
            Outcome::Blocked
        );
        // Deleted by hand after we set it: also left alone.
        reg.put(Setting::TailoredExperiences, 0, Some(1));
        assert_eq!(apply(&mut reg, &path, Setting::TailoredExperiences), Outcome::Done);
        reg.put(Setting::TailoredExperiences, 0, None);
        assert_eq!(
            undo(&mut reg, &path, Setting::TailoredExperiences),
            Outcome::ChangedSince
        );
        assert_eq!(reg.read(Setting::TailoredExperiences, 0), Value::Absent);
    }

    #[test]
    fn a_second_apply_keeps_the_first_runs_originals_on_record() {
        let (_d, path) = journal();
        let mut reg = Fake::default();
        let n = targets(Setting::SuggestedApps).len();
        for i in 0..n {
            reg.put(Setting::SuggestedApps, i, Some(1));
        }
        assert_eq!(apply(&mut reg, &path, Setting::SuggestedApps), Outcome::Done);
        // One suggestion is switched back on by hand, then Secblitz runs again.
        reg.put(Setting::SuggestedApps, 3, Some(1));
        assert_eq!(apply(&mut reg, &path, Setting::SuggestedApps), Outcome::Done);
        assert_eq!(undo(&mut reg, &path, Setting::SuggestedApps), Outcome::Done);
        for i in 0..n {
            assert_eq!(reg.read(Setting::SuggestedApps, i), Value::Dword(1), "{i}");
        }
    }

    #[test]
    fn undo_with_one_value_changed_restores_the_others() {
        let (_d, path) = journal();
        let mut reg = Fake::default();
        for i in 0..targets(Setting::SuggestedApps).len() {
            reg.put(Setting::SuggestedApps, i, Some(1));
        }
        assert_eq!(apply(&mut reg, &path, Setting::SuggestedApps), Outcome::Done);
        reg.put(Setting::SuggestedApps, 2, Some(1)); // set back on by hand
        assert_eq!(
            undo(&mut reg, &path, Setting::SuggestedApps),
            Outcome::Done
        );
        for i in 0..targets(Setting::SuggestedApps).len() {
            assert_eq!(reg.read(Setting::SuggestedApps, i), Value::Dword(1));
        }
    }

    #[test]
    fn undo_of_a_value_that_did_not_exist_deletes_it() {
        let (_d, path) = journal();
        let mut reg = Fake::default();
        assert_eq!(status(&reg, Setting::TailoredExperiences), Status::Unsafe);
        assert_eq!(
            apply(&mut reg, &path, Setting::TailoredExperiences),
            Outcome::Done
        );
        assert_eq!(reg.read(Setting::TailoredExperiences, 0), Value::Dword(0));
        assert_eq!(
            undo(&mut reg, &path, Setting::TailoredExperiences),
            Outcome::Done
        );
        assert_eq!(reg.read(Setting::TailoredExperiences, 0), Value::Absent);
    }

    #[test]
    fn apply_when_already_safe_changes_nothing_and_journals_nothing() {
        let (_d, path) = journal();
        let mut reg = Fake::default();
        reg.put(Setting::StoreAppsWebCheck, 0, Some(1));
        assert_eq!(
            apply(&mut reg, &path, Setting::StoreAppsWebCheck),
            Outcome::Done
        );
        assert!(!path.exists());
        assert_eq!(
            report(&reg, &path, Setting::StoreAppsWebCheck),
            Report::Safe
        );
    }

    #[test]
    fn a_value_that_does_not_stick_is_rolled_back_and_failed() {
        let (_d, path) = journal();
        let mut reg = Fake::default();
        reg.put(Setting::StoreAppsWebCheck, 0, Some(0));
        reg.silent_drop = true;
        assert_eq!(
            apply(&mut reg, &path, Setting::StoreAppsWebCheck),
            Outcome::Failed
        );
        assert_eq!(reg.read(Setting::StoreAppsWebCheck, 0), Value::Dword(0));
        assert!(!load_journal(&path)
            .settings
            .contains_key("smartscreen.store_apps"));
        reg.silent_drop = false;
        reg.fail_writes = true;
        assert_eq!(
            apply(&mut reg, &path, Setting::StoreAppsWebCheck),
            Outcome::Failed
        );
    }

    #[test]
    fn unwritable_journal_means_no_change() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the folder should be.
        let blocker = dir.path().join("Secblitz");
        std::fs::write(&blocker, b"x").unwrap();
        let path = blocker.join("user-settings.json");
        let mut reg = Fake::default();
        reg.put(Setting::ShowExtensions, 0, Some(1));
        assert_eq!(
            apply(&mut reg, &path, Setting::ShowExtensions),
            Outcome::Failed
        );
        assert_eq!(reg.read(Setting::ShowExtensions, 0), Value::Dword(1));
    }

    #[test]
    fn non_numeric_values_are_unknown_and_never_overwritten() {
        let (_d, path) = journal();
        let mut reg = Fake::default();
        let t = &targets(Setting::ShowExtensions)[0];
        reg.cu.insert(k(&t.key, t.name), Value::Other);
        assert_eq!(status(&reg, Setting::ShowExtensions), Status::Unknown);
        assert_eq!(
            apply(&mut reg, &path, Setting::ShowExtensions),
            Outcome::Blocked
        );
        assert_eq!(reg.read(Setting::ShowExtensions, 0), Value::Other);
    }

    #[test]
    fn nearby_sharing_only_changes_everyone_to_my_devices() {
        let (_d, path) = journal();
        let mut reg = Fake::default();
        // Not set: unknown, not offered, not "protected".
        assert_eq!(status(&reg, Setting::NearbySharing), Status::Unknown);
        assert_eq!(
            apply(&mut reg, &path, Setting::NearbySharing),
            Outcome::Blocked
        );
        assert_eq!(reg.read(Setting::NearbySharing, 0), Value::Absent);
        for safe in [0, 1] {
            reg.put(Setting::NearbySharing, 0, Some(safe));
            assert_eq!(status(&reg, Setting::NearbySharing), Status::Safe);
            assert_eq!(
                apply(&mut reg, &path, Setting::NearbySharing),
                Outcome::Done
            );
            assert_eq!(reg.read(Setting::NearbySharing, 0), Value::Dword(safe));
        }
        reg.put(Setting::NearbySharing, 0, Some(7));
        assert_eq!(status(&reg, Setting::NearbySharing), Status::Unknown);
        assert_eq!(
            apply(&mut reg, &path, Setting::NearbySharing),
            Outcome::Blocked
        );
        reg.put(Setting::NearbySharing, 0, Some(2));
        assert_eq!(status(&reg, Setting::NearbySharing), Status::Unsafe);
        assert_eq!(
            apply(&mut reg, &path, Setting::NearbySharing),
            Outcome::Done
        );
        assert_eq!(reg.read(Setting::NearbySharing, 0), Value::Dword(1));
        assert_eq!(undo(&mut reg, &path, Setting::NearbySharing), Outcome::Done);
        assert_eq!(reg.read(Setting::NearbySharing, 0), Value::Dword(2));
    }

    fn with_office(reg: &mut Fake) {
        reg.keys
            .push(r"SOFTWARE\Microsoft\Office\ClickToRun\Configuration".into());
    }

    #[test]
    fn office_is_not_applicable_without_office() {
        let (_d, path) = journal();
        let mut reg = Fake::default();
        assert_eq!(status(&reg, Setting::OfficeMacros), Status::NotApplicable);
        assert_eq!(
            apply(&mut reg, &path, Setting::OfficeMacros),
            Outcome::Blocked
        );
    }

    #[test]
    fn office_blocks_internet_macros_and_resets_protected_view_then_undoes() {
        let (_d, path) = journal();
        let mut reg = Fake::default();
        with_office(&mut reg);
        // Word: macro value missing; Protected View weakened by Disable*InPV=1.
        let all = targets(Setting::OfficeMacros);
        let pv = all
            .iter()
            .position(|t| t.name == "DisableInternetFilesInPV" && !t.key.contains("Policies"))
            .unwrap();
        reg.put(Setting::OfficeMacros, pv, Some(1));
        assert_eq!(status(&reg, Setting::OfficeMacros), Status::Unsafe);
        assert_eq!(apply(&mut reg, &path, Setting::OfficeMacros), Outcome::Done);
        assert_eq!(reg.read(Setting::OfficeMacros, pv), Value::Dword(0));
        for (i, t) in all.iter().enumerate() {
            if t.name == "blockcontentexecutionfrominternet" {
                assert_eq!(reg.read(Setting::OfficeMacros, i), Value::Dword(1));
            } else if i != pv {
                // Untouched Protected View values are never created.
                assert_eq!(reg.read(Setting::OfficeMacros, i), Value::Absent);
            }
        }
        assert_eq!(undo(&mut reg, &path, Setting::OfficeMacros), Outcome::Done);
        assert_eq!(reg.read(Setting::OfficeMacros, pv), Value::Dword(1));
        for (i, t) in all.iter().enumerate() {
            if t.name == "blockcontentexecutionfrominternet" {
                assert_eq!(reg.read(Setting::OfficeMacros, i), Value::Absent);
            }
        }
    }

    #[test]
    fn office_policy_writes_are_refused_on_managed_pcs() {
        let (_d, path) = journal();
        let mut reg = Fake::default();
        with_office(&mut reg);
        reg.strings.insert(
            k(
                r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters",
                "Domain",
            ),
            "corp.example".into(),
        );
        assert_eq!(
            apply(&mut reg, &path, Setting::OfficeMacros),
            Outcome::Blocked
        );
        assert!(!path.exists());

        let mut reg = Fake::default();
        with_office(&mut reg);
        reg.subs.insert(
            r"SOFTWARE\Microsoft\Enrollments".into(),
            vec!["{GUID}".into()],
        );
        reg.strings.insert(
            k(r"SOFTWARE\Microsoft\Enrollments\{GUID}", "ProviderID"),
            "MS DM Server".into(),
        );
        assert_eq!(
            apply(&mut reg, &path, Setting::OfficeMacros),
            Outcome::Blocked
        );
        // An empty domain value and an empty enrolment are not "managed".
        let mut reg = Fake::default();
        with_office(&mut reg);
        reg.strings.insert(
            k(
                r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters",
                "Domain",
            ),
            "  ".into(),
        );
        assert_eq!(apply(&mut reg, &path, Setting::OfficeMacros), Outcome::Done);
    }

    #[test]
    fn journal_round_trips_and_rejects_garbage() {
        let (_d, path) = journal();
        let mut j = Journal::default();
        j.settings.insert(
            "files.show_extensions".into(),
            vec![
                Prior {
                    i: 0,
                    prior: Some(1),
                },
                Prior { i: 3, prior: None },
            ],
        );
        save_journal(&path, &mut j).unwrap();
        let back = load_journal(&path);
        assert_eq!(back.v, 1);
        assert_eq!(back.settings, j.settings);

        std::fs::write(&path, b"not json").unwrap();
        assert!(load_journal(&path).settings.is_empty());
        std::fs::write(&path, vec![b' '; (JOURNAL_LIMIT + 1) as usize]).unwrap();
        assert!(load_journal(&path).settings.is_empty());
        assert!(load_journal(&path.with_file_name("missing.json"))
            .settings
            .is_empty());
    }

    #[test]
    fn a_journal_with_out_of_range_targets_is_dropped_not_obeyed() {
        let (_d, path) = journal();
        let mut j = Journal::default();
        j.settings.insert(
            "files.show_extensions".into(),
            vec![Prior {
                i: 99,
                prior: Some(5),
            }],
        );
        save_journal(&path, &mut j).unwrap();
        let mut reg = Fake::default();
        assert_eq!(
            undo(&mut reg, &path, Setting::ShowExtensions),
            Outcome::Blocked
        );
        assert!(load_journal(&path).settings.is_empty());
        assert!(reg.cu.is_empty());
    }

    fn put_suggestion(reg: &mut Fake, index: usize, v: Option<u32>) {
        reg.put(Setting::SuggestedApps, index, v);
    }

    /// Three values absent, four set to 1 (Windows' default "suggest" state).
    fn suggesting_reg() -> Fake {
        let mut reg = Fake::default();
        for i in 0..7 {
            put_suggestion(&mut reg, i, if i < 3 { None } else { Some(1) });
        }
        reg
    }

    #[test]
    fn suggested_apps_apply_records_absent_and_values() {
        let (_d, path) = journal();
        let mut reg = suggesting_reg();
        assert_eq!(status(&reg, Setting::SuggestedApps), Status::Unsafe);
        assert_eq!(
            apply(&mut reg, &path, Setting::SuggestedApps),
            Outcome::Done
        );
        for i in 0..7 {
            assert_eq!(reg.read(Setting::SuggestedApps, i), Value::Dword(0));
        }
        let stored = load_journal(&path);
        let priors = &stored.settings["debloat.suggested_apps"];
        assert_eq!(priors.len(), 7);
        for (i, p) in priors.iter().enumerate() {
            assert_eq!(p.i, i);
            assert_eq!(p.prior, if i < 3 { None } else { Some(1) });
        }
        assert_eq!(
            report(&reg, &path, Setting::SuggestedApps),
            Report::SafeByUs
        );
    }

    #[test]
    fn suggested_apps_undo_restores_absent() {
        let (_d, path) = journal();
        let mut reg = suggesting_reg();
        apply(&mut reg, &path, Setting::SuggestedApps);
        assert_eq!(undo(&mut reg, &path, Setting::SuggestedApps), Outcome::Done);
        for i in 0..7 {
            let expected = if i < 3 {
                Value::Absent
            } else {
                Value::Dword(1)
            };
            assert_eq!(reg.read(Setting::SuggestedApps, i), expected);
        }
        assert!(undoable(&path).is_empty());
    }

    #[test]
    fn suggested_apps_already_blocked_is_done_without_journal() {
        let (_d, path) = journal();
        let mut reg = Fake::default();
        for i in 0..7 {
            put_suggestion(&mut reg, i, Some(0));
        }
        assert_eq!(
            apply(&mut reg, &path, Setting::SuggestedApps),
            Outcome::Done
        );
        assert!(undoable(&path).is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn undo_all_undoes_every_journaled_setting() {
        let (_d, path) = journal();
        let mut reg = suggesting_reg();
        reg.put(Setting::ShowExtensions, 0, Some(1));
        apply(&mut reg, &path, Setting::ShowExtensions);
        apply(&mut reg, &path, Setting::SuggestedApps);
        assert_eq!(
            undoable(&path),
            [Setting::ShowExtensions, Setting::SuggestedApps]
        );
        let results = undo_all(&mut reg, &path);
        assert_eq!(
            results,
            [
                (Setting::ShowExtensions, Outcome::Done),
                (Setting::SuggestedApps, Outcome::Done)
            ]
        );
        assert!(undoable(&path).is_empty());
        assert_eq!(reg.read(Setting::ShowExtensions, 0), Value::Dword(1));
    }

    #[test]
    fn personal_excludes_suggested_apps() {
        assert_eq!(Setting::PERSONAL.len(), 5);
        assert!(!Setting::PERSONAL.contains(&Setting::SuggestedApps));
        assert_eq!(Setting::ALL[..5], Setting::PERSONAL);
        assert_eq!(Setting::SuggestedApps.to_byte(), 5);
    }

    #[test]
    fn handle_needs_a_data_folder() {
        let mut reg = Fake::default();
        assert!(handle(&mut reg, None, Setting::ShowExtensions, Op::Query).is_err());
    }
}
