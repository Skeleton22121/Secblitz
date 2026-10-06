//! Safety decisions for the two core protections Secblitz can turn on for the
//! person: Memory integrity (`vbs.memory_integrity`) and Kernel-mode
//! Hardware-enforced Stack Protection (`vbs.kernel_stack_protection`).
//!
//! Everything here is pure and testable on any host. The Windows facts it
//! judges (what the hardware reports, which drivers would load) are collected
//! by `platform/windows.rs` and `platform/vbs.ps1`, read-only and fast: no
//! driver is loaded and nothing is written.
//!
//! What this relies on (Microsoft Learn):
//! * "Enable memory integrity": the `HypervisorEnforcedCodeIntegrity` scenario
//!   values `Enabled` and `WasEnabledBy` (2 lets the Windows Security switch
//!   behave normally), the `Locked` value (never written by Secblitz), and the
//!   Win32_DeviceGuard class: AvailableSecurityProperties (1 hypervisor, 2
//!   Secure Boot), SecurityServicesConfigured and SecurityServicesRunning
//!   (2 memory integrity, 5 kernel-mode stack protection, 6 its audit mode).
//! * "Driver compatibility with memory integrity and VBS": drivers must not
//!   have a section that is both writable and executable, must have section
//!   alignment that is a multiple of 0x1000, and must not place the import
//!   address table in an executable section. Other causes (executable memory
//!   allocations at run time) cannot be seen in a file, so the scan can only
//!   rule drivers out, never prove one works: that is why the person can undo.
//! * "Kernel Mode Hardware-enforced Stack Protection": needs memory integrity
//!   and a processor with shadow stacks (Intel CET or AMD shadow stacks).
//! * "Understanding App Control event IDs": 3111 (a file did not meet the
//!   hypervisor-protected code integrity policy) and 3074 (page hash failure
//!   while it was on) in Microsoft-Windows-CodeIntegrity/Operational.

use crate::model::Finding;
use serde::Deserialize;

pub const MEMORY_INTEGRITY: &str = "vbs.memory_integrity";
pub const STACK_PROTECTION: &str = "vbs.kernel_stack_protection";

pub fn is_vbs(id: &str) -> bool {
    id == MEMORY_INTEGRITY || id == STACK_PROTECTION
}

// Exact "not offered" reasons. advice.rs knows each one and shows a calm line.
pub const NOT_SUPPORTED: &str = "Not offered: this PC's hardware or firmware does not support it";
pub const LOCKED: &str = "Not offered: it is locked in your PC's firmware";
pub const DRIVER: &str = "Not offered: a driver on this PC may not work with it";
pub const DRIVERS_UNREADABLE: &str = "Not offered: we could not check this PC's drivers";
pub const ALREADY_ON: &str = "Not offered: it is already running";
pub const NEEDS_MEMORY_INTEGRITY: &str = "Not offered: it needs memory integrity running first";
pub const NEEDS_RESTART: &str = "Not offered: restart your PC to finish memory integrity first";
pub const NO_SHADOW_STACKS: &str = "Not offered: this PC's processor does not support it";
pub const UNREADABLE: &str = "Not offered: we could not check this PC's protection support";
pub const SET_BY_HAND: &str = "Not offered: this PC's virtualization security was set up by hand";
pub const OLD_WINDOWS: &str = "Not offered: this version of Windows does not support it";

// Finding titles for the check after the restart.
pub const MEMORY_INTEGRITY_NOT_RUNNING: &str = "Memory integrity not running";
pub const STACK_NOT_RUNNING: &str = "Kernel stack protection not running";
/// Memory integrity runs, but Windows refused a driver since the last start.
pub const DEVICE_BLOCKED: &str = "A device may not be working";
/// Prefix of the finding detail that carries driver names.
pub const BLOCKED_PREFIX: &str = "blocked: ";
/// The finding detail carries the start-up time (Unix seconds) so the engine
/// can tell "restarted since the change" from "still waiting for a restart".
pub const BOOT_PREFIX: &str = "boot: ";
/// The engine adds this when the control's change is the newest one that can
/// be undone, so "Undo" really undoes it and nothing else.
pub const UNDO_READY: &str = "undo: latest";

/// The control a verification finding is about.
pub fn finding_control(title: &str) -> Option<&'static str> {
    match title {
        MEMORY_INTEGRITY_NOT_RUNNING | DEVICE_BLOCKED => Some(MEMORY_INTEGRITY),
        STACK_NOT_RUNNING => Some(STACK_PROTECTION),
        _ => None,
    }
}

/// Start-up time carried in a verification finding.
pub fn boot_from_detail(detail: &str) -> Option<i64> {
    let rest = detail.split_once(BOOT_PREFIX)?.1;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Split one fix selection into separate batches: everything ordinary first,
/// then each core protection alone, so undoing one never touches another fix.
pub fn split_batches(ids: &[String]) -> Vec<Vec<String>> {
    let mut batches = Vec::new();
    let ordinary: Vec<String> = ids.iter().filter(|i| !is_vbs(i)).cloned().collect();
    if !ordinary.is_empty() {
        batches.push(ordinary);
    }
    for id in ids.iter().filter(|i| is_vbs(i)) {
        batches.push(vec![id.clone()]);
    }
    batches
}

// ------------------------------------------------------------ PE scan

/// One reason a driver file may not work with memory integrity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeProblem {
    /// Not a readable Windows image.
    NotAnImage,
    /// Section alignment is not a multiple of 0x1000 (page size).
    SectionAlignment,
    /// A section is both writable and executable (the old INIT section is
    /// tolerated: Windows removes its write permission).
    WritableAndExecutable(String),
    /// The import address table sits in an executable section.
    ImportTableExecutable,
}

const PAGE: u32 = 0x1000;
const SCN_EXECUTE: u32 = 0x2000_0000;
const SCN_WRITE: u32 = 0x8000_0000;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at.checked_add(2)?)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

/// Check the headers of a driver image against the static requirements.
/// `bytes` must hold the headers (the first 64 KiB is plenty).
pub fn scan_pe(bytes: &[u8]) -> Vec<PeProblem> {
    match scan_headers(bytes) {
        Some(problems) => problems,
        None => vec![PeProblem::NotAnImage],
    }
}

fn scan_headers(b: &[u8]) -> Option<Vec<PeProblem>> {
    if b.get(..2)? != b"MZ" {
        return None;
    }
    let pe = u32_at(b, 0x3c)? as usize;
    if b.get(pe..pe.checked_add(4)?)? != b"PE\0\0" {
        return None;
    }
    let coff = pe + 4;
    let sections = usize::from(u16_at(b, coff + 2)?);
    let optional_size = usize::from(u16_at(b, coff + 16)?);
    let opt = coff + 20;
    // PE32+ (64-bit) and PE32 differ only in where the directories start.
    let (count_at, dirs_at) = match u16_at(b, opt)? {
        0x20b => (opt + 108, opt + 112),
        0x10b => (opt + 92, opt + 96),
        _ => return None,
    };
    if sections == 0 || sections > 96 {
        return None;
    }
    let alignment = u32_at(b, opt + 32)?;
    let directories = u32_at(b, count_at)?;
    // Data directory 12 is the import address table.
    let iat = if directories > 12 {
        u32_at(b, dirs_at + 12 * 8)?
    } else {
        0
    };
    let table = opt.checked_add(optional_size)?;
    let mut problems = Vec::new();
    if alignment == 0 || alignment % PAGE != 0 {
        problems.push(PeProblem::SectionAlignment);
    }
    for index in 0..sections {
        let at = table.checked_add(index.checked_mul(40)?)?;
        let name_bytes = b.get(at..at.checked_add(8)?)?;
        let virtual_size = u32_at(b, at + 8)?;
        let address = u32_at(b, at + 12)?;
        let raw_size = u32_at(b, at + 16)?;
        let flags = u32_at(b, at + 36)?;
        let name: String = name_bytes
            .iter()
            .take_while(|c| **c != 0)
            .map(|c| {
                if c.is_ascii_graphic() {
                    char::from(*c)
                } else {
                    '?'
                }
            })
            .collect();
        if alignment != 0
            && alignment % PAGE == 0
            && address % PAGE != 0
            && !problems.contains(&PeProblem::SectionAlignment)
        {
            problems.push(PeProblem::SectionAlignment);
        }
        let executable = flags & SCN_EXECUTE != 0;
        if executable && flags & SCN_WRITE != 0 && !name.eq_ignore_ascii_case("INIT") {
            problems.push(PeProblem::WritableAndExecutable(name));
        } else if executable && iat != 0 {
            let size = virtual_size.max(raw_size);
            if iat >= address && u64::from(iat) < u64::from(address) + u64::from(size) {
                problems.push(PeProblem::ImportTableExecutable);
            }
        }
    }
    Some(problems)
}

// ------------------------------------------------------------- drivers

/// A driver image that would be loaded: its short name and where it lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverFile {
    pub name: String,
    pub path: String,
}

/// One kernel driver service as the registry describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceRow {
    pub name: String,
    pub kind: u32,
    pub start: u32,
    pub image: Option<String>,
}

/// Turn a service's image setting into an absolute path. Handles the forms
/// Windows writes: `\SystemRoot\...`, `%SystemRoot%\...`, `\??\C:\...`,
/// `system32\drivers\x.sys` (relative to the Windows folder) and `C:\...`.
/// Anything else is None: the caller must treat that as "could not check".
pub fn resolve_image(raw: &str, windows: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return None;
    }
    // Keep only the file: some entries carry arguments after the name. The
    // name ends at ".sys" followed by the end, a space or a quote, so a
    // folder such as `Foo.System` is never cut in the middle.
    let text = if let Some(quoted) = trimmed.strip_prefix('"') {
        quoted.split('"').next()?
    } else {
        let lower = trimmed.to_ascii_lowercase();
        let mut end = None;
        for (at, _) in lower.match_indices(".sys") {
            let next = lower[at + 4..].chars().next();
            if next.is_none_or(|c| c.is_whitespace() || c == '"') {
                end = Some(at + 4);
                break;
            }
        }
        trimmed.get(..end?)?
    };
    let text = text.trim();
    if !text.to_ascii_lowercase().ends_with(".sys") {
        return None;
    }
    let windows = windows.trim_end_matches('\\');
    let lower = text.to_ascii_lowercase();
    let full = if let Some(rest) = text.strip_prefix(r"\??\") {
        rest.to_owned()
    } else if lower.starts_with(r"\systemroot\") {
        format!("{windows}\\{}", &text[12..])
    } else if lower.starts_with("%systemroot%\\") {
        format!("{windows}\\{}", &text[13..])
    } else if text.len() > 2 && text.as_bytes()[1] == b':' && text.as_bytes()[2] == b'\\' {
        text.to_owned()
    } else if !text.starts_with('\\') && !text.contains(':') {
        format!("{windows}\\{text}")
    } else {
        return None;
    };
    let bytes = full.as_bytes();
    let absolute =
        bytes.len() > 7 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    (absolute && !full.split('\\').any(|part| part == "..")).then_some(full)
}

/// The drivers to scan, and the configured drivers whose file could not be
/// located (those can never be called fine).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DriverList {
    pub files: Vec<DriverFile>,
    pub unresolved: Vec<String>,
}

/// The driver files configured to load (kernel or file-system driver services
/// that start at boot, system start, automatically or on demand) plus the
/// modules that are loaded right now. Only `.sys` files are listed, once each.
pub fn driver_files(services: &[ServiceRow], loaded: &[String], windows: &str) -> DriverList {
    let mut out = DriverList::default();
    let mut push = |name: &str, path: String| {
        let lower = path.to_ascii_lowercase();
        if !lower.ends_with(".sys")
            || out
                .files
                .iter()
                .any(|d| d.path.to_ascii_lowercase() == lower)
        {
            return;
        }
        out.files.push(DriverFile {
            name: file_name(&path)
                .or_else(|| safe_name(name))
                .unwrap_or_else(|| "driver".into()),
            path,
        });
    };
    for service in services {
        if !matches!(service.kind, 1 | 2) || service.start > 3 {
            continue;
        }
        let path = match &service.image {
            Some(image) => resolve_image(image, windows),
            None => Some(format!(
                "{}\\System32\\drivers\\{}.sys",
                windows.trim_end_matches('\\'),
                service.name
            )),
        };
        match path {
            Some(path) => push(&service.name, path),
            None => out.unresolved.push(
                safe_name(&service.name).unwrap_or_else(|| "driver".into()),
            ),
        }
    }
    // Loaded modules include .exe and .dll files (the kernel itself): only
    // drivers are scanned, and those are skipped on purpose.
    for module in loaded {
        if let Some(path) = resolve_image(module, windows) {
            push("", path);
        }
    }
    out
}

fn file_name(path: &str) -> Option<String> {
    let name = path.rsplit('\\').next()?;
    safe_name(name)
}

/// A file name that is safe to show: plain characters only, bounded.
pub fn safe_name(name: &str) -> Option<String> {
    let ok = !name.is_empty()
        && name.chars().count() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    ok.then(|| name.to_owned())
}

/// Result of scanning the driver files.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Scan {
    /// Drivers that failed a documented requirement, with the reasons.
    pub flagged: Vec<(String, Vec<PeProblem>)>,
    /// Drivers that exist but could not be read.
    pub unreadable: Vec<String>,
}

/// Scan every driver whose image exists. A missing file is not a problem (the
/// driver cannot load); one that cannot be read is reported, never assumed fine.
pub fn scan_files(
    files: &[DriverFile],
    read: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
) -> Scan {
    let mut scan = Scan::default();
    for file in files {
        match read(&file.path) {
            Ok(bytes) => {
                let problems = scan_pe(&bytes);
                if !problems.is_empty() {
                    scan.flagged.push((file.name.clone(), problems));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => scan.unreadable.push(file.name.clone()),
        }
    }
    scan
}

// --------------------------------------------------------------- facts

/// What the PC reports, read without changing anything.
#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Facts {
    pub available: Vec<u32>,
    pub required: Vec<u32>,
    pub configured: Vec<u32>,
    pub running: Vec<u32>,
    pub virt_firmware: Option<bool>,
    /// Win32_DeviceGuard VirtualizationBasedSecurityStatus: 2 means running.
    pub vbs_status: Option<u32>,
    /// Windows build number, for features that arrived in a later release.
    pub build: Option<u32>,
    pub lock_vbs: Option<u32>,
    pub lock_hvci: Option<u32>,
    pub lock_stack: Option<u32>,
    /// DeviceGuard `Mandatory`: Windows refuses to start without VBS.
    pub mandatory: Option<u32>,
    /// DeviceGuard `EnableVirtualizationBasedSecurity` set on purpose.
    pub enable_vbs: Option<u32>,
    pub require_platform: Option<u32>,
    pub enabled_hvci: Option<u32>,
    pub enabled_stack: Option<u32>,
    pub boot_unix: Option<i64>,
    /// Driver files Windows blocked since the last start.
    pub blocked: Vec<String>,
    /// Vendor the processor reports for the hypervisor it runs under, filled
    /// in by the caller from CPUID (not by the script). None: no hypervisor.
    #[serde(skip)]
    pub hypervisor_vendor: Option<String>,
}

/// The vendor id Windows' own hypervisor reports.
pub const MICROSOFT_HV: &str = "Microsoft Hv";

/// Whether the PC can start memory integrity: a hypervisor and Secure Boot
/// are available, and either Windows' own hypervisor is already in use or no
/// hypervisor is present and virtualization is on in the firmware. Any other
/// vendor's hypervisor (a virtual machine) cannot host Windows' hypervisor
/// reliably, so it never counts.
pub fn hardware_supports(facts: &Facts) -> bool {
    if !(facts.available.contains(&1) && facts.available.contains(&2)) {
        return false;
    }
    match facts.hypervisor_vendor.as_deref() {
        Some(MICROSOFT_HV) => true,
        Some(_) => facts.vbs_status == Some(2),
        None => facts.virt_firmware == Some(true) || facts.vbs_status == Some(2),
    }
}

/// First build of Windows 11 22H2, which added kernel-mode stack protection.
pub const STACK_PROTECTION_BUILD: u32 = 22621;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Offer,
    NotOffered(String),
}

fn no(reason: &str) -> Decision {
    Decision::NotOffered(reason.to_owned())
}

/// Why the driver scan blocks the offer, naming the drivers (at most five).
pub fn driver_reason(scan: &Scan) -> Option<String> {
    if !scan.flagged.is_empty() {
        let mut names: Vec<&str> = scan.flagged.iter().map(|(n, _)| n.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        let text = names.iter().take(5).copied().collect::<Vec<_>>().join(", ");
        return Some(format!("{DRIVER}: {text}"));
    }
    (!scan.unreadable.is_empty()).then(|| DRIVERS_UNREADABLE.to_owned())
}

/// Settings someone chose on purpose that make a change here risky or
/// pointless: Windows refusing to start without VBS, VBS switched off by hand,
/// or a platform feature that is required but not available.
fn set_up_by_hand(facts: &Facts) -> Option<&'static str> {
    if facts.mandatory == Some(1) || facts.enable_vbs == Some(0) {
        return Some(SET_BY_HAND);
    }
    let missing_required = facts.required.iter().any(|p| !facts.available.contains(p));
    let platform_needs_dma =
        facts.require_platform.is_some_and(|v| v & 2 != 0) && !facts.available.contains(&3);
    (missing_required || platform_needs_dma).then_some(NOT_SUPPORTED)
}

/// Decide whether a fix may be offered. `scan` runs the driver scan and is
/// only called when every cheaper check has passed.
pub fn decide(
    id: &str,
    facts: &Facts,
    shadow_stack_cpu: bool,
    scan: &mut dyn FnMut() -> Scan,
) -> Decision {
    let locked = facts.lock_vbs == Some(1)
        || if id == STACK_PROTECTION {
            facts.lock_stack == Some(1)
        } else {
            facts.lock_hvci == Some(1)
        };
    if id == STACK_PROTECTION {
        if facts.running.contains(&5) {
            return no(ALREADY_ON);
        }
        if locked {
            return no(LOCKED);
        }
        if facts.build.is_some_and(|b| b < STACK_PROTECTION_BUILD) {
            return no(OLD_WINDOWS);
        }
        // Say what cannot change first, so the advice can always be followed.
        if !hardware_supports(facts) && !facts.running.contains(&2) {
            return no(NOT_SUPPORTED);
        }
        if !shadow_stack_cpu {
            return no(NO_SHADOW_STACKS);
        }
        if let Some(reason) = set_up_by_hand(facts) {
            return no(reason);
        }
        if !facts.running.contains(&2) {
            return if facts.configured.contains(&2) {
                no(NEEDS_RESTART)
            } else {
                no(NEEDS_MEMORY_INTEGRITY)
            };
        }
        return Decision::Offer;
    }
    if id != MEMORY_INTEGRITY {
        return no(UNREADABLE);
    }
    if facts.running.contains(&2) {
        return no(ALREADY_ON);
    }
    if locked {
        return no(LOCKED);
    }
    if !hardware_supports(facts) {
        return no(NOT_SUPPORTED);
    }
    if let Some(reason) = set_up_by_hand(facts) {
        return no(reason);
    }
    match driver_reason(&scan()) {
        Some(reason) => Decision::NotOffered(reason),
        None => Decision::Offer,
    }
}

/// True when the PC has started since the setting was written, so the
/// setting has had its chance to take effect. Unknown times never claim it.
pub fn restarted_since(written_unix: Option<i64>, boot_unix: Option<i64>) -> bool {
    matches!((written_unix, boot_unix), (Some(w), Some(b)) if w <= b)
}

/// Candidate findings for the check after a restart. They only say what the
/// PC reports now (plus the start-up time); the engine keeps one only when
/// Secblitz made the change, the PC has restarted since, and says whether
/// undoing it is the next undo.
pub fn verification(facts: &Facts) -> Vec<Finding> {
    let mut out = Vec::new();
    let Some(boot) = facts.boot_unix else {
        return out;
    };
    let names: Vec<String> = facts
        .blocked
        .iter()
        .filter_map(|n| safe_name(n))
        .take(5)
        .collect();
    let detail = |base: &str| {
        let mut text = format!("{base} {BOOT_PREFIX}{boot}.");
        if !names.is_empty() {
            text.push_str(&format!(" {BLOCKED_PREFIX}{}", names.join(", ")));
        }
        text
    };
    let memory_integrity_running = facts.running.contains(&2);
    if facts.enabled_hvci == Some(1) && !memory_integrity_running {
        out.push(Finding {
            title: MEMORY_INTEGRITY_NOT_RUNNING.into(),
            status: "attention".into(),
            detail: detail("Memory integrity is configured but not running."),
        });
    } else if facts.enabled_hvci == Some(1) && memory_integrity_running && !names.is_empty() {
        out.push(Finding {
            title: DEVICE_BLOCKED.into(),
            status: "attention".into(),
            detail: detail("Windows refused a driver while memory integrity was on."),
        });
    } else if facts.enabled_stack == Some(1)
        && memory_integrity_running
        && !facts.running.contains(&5)
    {
        out.push(Finding {
            title: STACK_NOT_RUNNING.into(),
            status: "attention".into(),
            detail: detail("Kernel stack protection is configured but not running."),
        });
    }
    out
}

/// Driver names carried in a verification finding's detail (already
/// sanitized when it was built; checked again here before anything is shown).
pub fn blocked_names(detail: &str) -> Option<String> {
    let rest = detail.split_once(BLOCKED_PREFIX)?.1;
    let names: Vec<String> = rest
        .split(',')
        .filter_map(|n| safe_name(n.trim()))
        .take(5)
        .collect();
    (!names.is_empty()).then(|| names.join(", "))
}

/// Driver names inside an exact driver "not offered" reason.
pub fn reason_names(reason: &str) -> Option<String> {
    let rest = reason.strip_prefix(DRIVER)?.strip_prefix(": ")?;
    let cleaned: String = rest
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ',' | ' '))
        .collect();
    (!cleaned.is_empty() && cleaned == rest && rest.len() <= 200).then(|| rest.to_owned())
}

/// Does the processor report shadow stack (CET) support? CPUID leaf 7, ECX
/// bit 7, the same flag on Intel and AMD. Read-only and instant.
pub fn cpu_has_shadow_stacks() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        cpuid_leaf7_ecx().is_some_and(|ecx| ecx & (1 << 7) != 0)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

#[cfg(target_arch = "x86_64")]
#[allow(unused_unsafe)]
fn cpuid_leaf7_ecx() -> Option<u32> {
    use std::arch::x86_64::{__cpuid, __cpuid_count};
    // SAFETY: CPUID exists on every x86_64 processor and only reads.
    unsafe {
        if __cpuid(0).eax < 7 {
            return None;
        }
        Some(__cpuid_count(7, 0).ecx)
    }
}

/// The vendor id of the hypervisor this processor runs under (CPUID leaf 1
/// bit 31, then leaf 0x40000000), or None on bare hardware. Read-only.
pub fn cpu_hypervisor_vendor() -> Option<String> {
    #[cfg(target_arch = "x86_64")]
    {
        hypervisor_vendor_x86()
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        None
    }
}

#[cfg(target_arch = "x86_64")]
#[allow(unused_unsafe)]
fn hypervisor_vendor_x86() -> Option<String> {
    use std::arch::x86_64::{__cpuid, __cpuid_count};
    // SAFETY: CPUID exists on every x86_64 processor and only reads.
    unsafe {
        if __cpuid(1).ecx & (1 << 31) == 0 {
            return None;
        }
        let leaf = __cpuid_count(0x4000_0000, 0);
        let mut raw = Vec::with_capacity(12);
        for part in [leaf.ebx, leaf.ecx, leaf.edx] {
            raw.extend_from_slice(&part.to_le_bytes());
        }
        let text: String = raw
            .iter()
            .take_while(|b| **b != 0)
            .map(|b| if b.is_ascii_graphic() || *b == b' ' { char::from(*b) } else { '?' })
            .collect();
        // A flag without a readable vendor still means some hypervisor.
        Some(if text.is_empty() { "unknown".into() } else { text })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Pe {
        alignment: u32,
        iat: u32,
        /// (name, address, size, flags)
        sections: Vec<(&'static str, u32, u32, u32)>,
        pe32: bool,
    }

    impl Default for Pe {
        fn default() -> Self {
            Pe {
                alignment: 0x1000,
                iat: 0,
                sections: vec![
                    (".text", 0x1000, 0x800, 0x6000_0020),
                    (".data", 0x2000, 0x400, 0xC000_0040),
                    ("PAGE", 0x3000, 0x400, 0x6000_0020),
                ],
                pe32: false,
            }
        }
    }

    fn put16(b: &mut [u8], at: usize, v: u16) {
        b[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn put32(b: &mut [u8], at: usize, v: u32) {
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn build(pe: &Pe) -> Vec<u8> {
        let mut b = vec![0u8; 0x400];
        b[..2].copy_from_slice(b"MZ");
        put32(&mut b, 0x3c, 0x80);
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        put16(&mut b, coff, 0x8664);
        put16(&mut b, coff + 2, pe.sections.len() as u16);
        let optional = if pe.pe32 { 224 } else { 240 };
        put16(&mut b, coff + 16, optional);
        let opt = coff + 20;
        put16(&mut b, opt, if pe.pe32 { 0x10b } else { 0x20b });
        put32(&mut b, opt + 32, pe.alignment);
        put32(&mut b, opt + 36, 0x200);
        let (count_at, dirs_at) = if pe.pe32 {
            (opt + 92, opt + 96)
        } else {
            (opt + 108, opt + 112)
        };
        put32(&mut b, count_at, 16);
        put32(&mut b, dirs_at + 12 * 8, pe.iat);
        let table = opt + usize::from(optional);
        for (i, (name, address, size, flags)) in pe.sections.iter().enumerate() {
            let at = table + i * 40;
            b[at..at + name.len()].copy_from_slice(name.as_bytes());
            put32(&mut b, at + 8, *size);
            put32(&mut b, at + 12, *address);
            put32(&mut b, at + 16, *size);
            put32(&mut b, at + 36, *flags);
        }
        b
    }

    #[test]
    fn a_clean_driver_passes_in_both_image_formats() {
        for pe32 in [false, true] {
            let pe = Pe {
                pe32,
                iat: 0x2010,
                ..Pe::default()
            };
            assert_eq!(scan_pe(&build(&pe)), vec![], "pe32={pe32}");
        }
    }

    #[test]
    fn section_alignment_must_be_a_whole_page() {
        for alignment in [0, 0x20, 0x200, 0x800, 0x1800] {
            let pe = Pe {
                alignment,
                ..Pe::default()
            };
            assert!(
                scan_pe(&build(&pe)).contains(&PeProblem::SectionAlignment),
                "{alignment:#x}"
            );
        }
        for alignment in [0x1000, 0x2000] {
            let pe = Pe {
                alignment,
                ..Pe::default()
            };
            assert_eq!(scan_pe(&build(&pe)), vec![], "{alignment:#x}");
        }
        // A section that does not start on a page boundary.
        let pe = Pe {
            sections: vec![(".text", 0x1200, 0x800, 0x6000_0020)],
            ..Pe::default()
        };
        assert_eq!(scan_pe(&build(&pe)), vec![PeProblem::SectionAlignment]);
    }

    #[test]
    fn a_section_that_is_writable_and_executable_is_flagged() {
        let pe = Pe {
            sections: vec![
                (".text", 0x1000, 0x800, 0x6000_0020),
                (".hack", 0x2000, 0x400, 0xE000_0020),
            ],
            ..Pe::default()
        };
        assert_eq!(
            scan_pe(&build(&pe)),
            vec![PeProblem::WritableAndExecutable(".hack".into())]
        );
        // Write only, or execute only, is fine.
        let pe = Pe {
            sections: vec![
                (".a", 0x1000, 0x800, 0xC000_0040),
                (".b", 0x2000, 0x400, 0x2000_0020),
            ],
            ..Pe::default()
        };
        assert_eq!(scan_pe(&build(&pe)), vec![]);
    }

    #[test]
    fn the_old_init_section_is_tolerated_because_windows_removes_its_write_permission() {
        for name in ["INIT", "init"] {
            let pe = Pe {
                sections: vec![
                    (".text", 0x1000, 0x800, 0x6000_0020),
                    (name, 0x2000, 0x400, 0xE000_0020),
                ],
                ..Pe::default()
            };
            assert_eq!(scan_pe(&build(&pe)), vec![], "{name}");
        }
    }

    #[test]
    fn the_import_table_must_not_be_in_an_executable_section() {
        // The IAT lands inside .text (a merged .rdata).
        let pe = Pe {
            iat: 0x1100,
            ..Pe::default()
        };
        assert_eq!(scan_pe(&build(&pe)), vec![PeProblem::ImportTableExecutable]);
        // The last byte of the section counts, the first byte after does not.
        let pe = Pe {
            iat: 0x1000 + 0x800 - 1,
            ..Pe::default()
        };
        assert_eq!(scan_pe(&build(&pe)), vec![PeProblem::ImportTableExecutable]);
        let pe = Pe {
            iat: 0x1000 + 0x800,
            ..Pe::default()
        };
        assert_eq!(scan_pe(&build(&pe)), vec![]);
        // In a data section: fine. No table at all: fine.
        let pe = Pe {
            iat: 0x2008,
            ..Pe::default()
        };
        assert_eq!(scan_pe(&build(&pe)), vec![]);
    }

    #[test]
    fn broken_or_truncated_files_are_reported_never_waved_through() {
        let good = build(&Pe::default());
        assert_eq!(scan_pe(&[]), vec![PeProblem::NotAnImage]);
        assert_eq!(scan_pe(b"MZ"), vec![PeProblem::NotAnImage]);
        assert_eq!(scan_pe(&good[..0x90]), vec![PeProblem::NotAnImage]);
        // Section table cut short.
        assert_eq!(scan_pe(&good[..0x170]), vec![PeProblem::NotAnImage]);
        let mut bad = good.clone();
        bad[0x80..0x84].copy_from_slice(b"PX\0\0");
        assert_eq!(scan_pe(&bad), vec![PeProblem::NotAnImage]);
        let mut bad = good.clone();
        put16(&mut bad, 0x84 + 20, 0x1234);
        assert_eq!(scan_pe(&bad), vec![PeProblem::NotAnImage]);
        let mut bad = good.clone();
        put32(&mut bad, 0x3c, 0xFFFF_FFF0);
        assert_eq!(scan_pe(&bad), vec![PeProblem::NotAnImage]);
        let mut bad = good.clone();
        put16(&mut bad, 0x84 + 2, 0);
        assert_eq!(scan_pe(&bad), vec![PeProblem::NotAnImage]);
        let mut bad = good;
        put16(&mut bad, 0x84 + 2, 500);
        assert_eq!(scan_pe(&bad), vec![PeProblem::NotAnImage]);
    }

    #[test]
    fn images_resolve_to_absolute_paths_in_every_form_windows_writes() {
        let w = r"C:\Windows";
        for (raw, want) in [
            (
                r"\SystemRoot\System32\drivers\a.sys",
                Some(r"C:\Windows\System32\drivers\a.sys"),
            ),
            (
                r"%SystemRoot%\system32\drivers\a.sys",
                Some(r"C:\Windows\system32\drivers\a.sys"),
            ),
            (
                r"System32\drivers\a.sys",
                Some(r"C:\Windows\System32\drivers\a.sys"),
            ),
            (r"\??\D:\x\a.sys", Some(r"D:\x\a.sys")),
            (r#""D:\x y\a.sys""#, Some(r"D:\x y\a.sys")),
            (r"D:\x\a.sys -flag", Some(r"D:\x\a.sys")),
            // A ".sys" inside a folder name is not the end of the file name.
            (
                r"C:\Tools\Foo.System\drv.sys",
                Some(r"C:\Tools\Foo.System\drv.sys"),
            ),
            (r"C:\Tools\a.sys.exe", None),
            (r"\Windows\System32\drivers\a.sys", None),
            (r"\SYSTEMROOT\SYSTEM32\A.SYS", Some(r"C:\Windows\SYSTEM32\A.SYS")),
            (r"\Device\HarddiskVolume3\a.sys", None),
            (r"C:\a.exe", None),
            (r"C:\x\..\a.sys", None),
            (r"\\server\share\a.sys", None),
            ("", None),
            ("a\0.sys", None),
        ] {
            assert_eq!(resolve_image(raw, w).as_deref(), want, "{raw}");
        }
    }

    #[test]
    fn the_driver_list_covers_configured_and_loaded_drivers_once_each() {
        let svc = |name: &str, kind, start, image: Option<&str>| ServiceRow {
            name: name.into(),
            kind,
            start,
            image: image.map(Into::into),
        };
        let services = vec![
            svc("boot", 1, 0, Some(r"\SystemRoot\System32\drivers\boot.sys")),
            svc("auto", 1, 2, None),
            svc("fs", 2, 3, Some(r"system32\drivers\fs.sys")),
            svc("off", 1, 4, Some(r"system32\drivers\off.sys")),
            svc("svc", 16, 2, Some(r"C:\x\svc.exe")),
            svc("odd", 1, 3, Some(r"\Device\x\odd.sys")),
        ];
        let loaded = vec![
            r"\SystemRoot\system32\ntoskrnl.exe".to_owned(),
            r"\SystemRoot\System32\drivers\BOOT.SYS".to_owned(),
            r"\??\C:\Windows\System32\drivers\extra.sys".to_owned(),
        ];
        let list = driver_files(&services, &loaded, r"C:\Windows\");
        let names: Vec<&str> = list.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["boot.sys", "auto.sys", "fs.sys", "extra.sys"]);
        // A configured driver whose file cannot be located is never skipped
        // quietly: it is listed so the check says "could not check".
        assert_eq!(list.unresolved, ["odd"]);
        // An odd service name never reaches the screen.
        let odd = driver_files(&[svc("bad name;", 1, 3, Some(r"\Device\x\a.sys"))], &[], r"C:\Windows");
        assert_eq!(odd.unresolved, ["driver"]);
    }

    fn read_from(
        map: Vec<(&'static str, std::io::Result<Vec<u8>>)>,
    ) -> impl Fn(&str) -> std::io::Result<Vec<u8>> {
        let map = std::cell::RefCell::new(map);
        move |path: &str| {
            let mut m = map.borrow_mut();
            let i = m.iter().position(|(p, _)| *p == path).expect(path);
            let (_, r) = m.remove(i);
            r
        }
    }

    #[test]
    fn scanning_flags_bad_drivers_skips_missing_ones_and_reports_unreadable_ones() {
        let files: Vec<DriverFile> = ["good", "bad", "gone", "locked"]
            .iter()
            .map(|n| DriverFile {
                name: format!("{n}.sys"),
                path: (*n).into(),
            })
            .collect();
        let bad = build(&Pe {
            alignment: 0x200,
            ..Pe::default()
        });
        let read = read_from(vec![
            ("good", Ok(build(&Pe::default()))),
            ("bad", Ok(bad)),
            (
                "gone",
                Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
            ),
            (
                "locked",
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
            ),
        ]);
        let scan = scan_files(&files, &read);
        assert_eq!(
            scan.flagged,
            vec![("bad.sys".to_owned(), vec![PeProblem::SectionAlignment])]
        );
        assert_eq!(scan.unreadable, vec!["locked.sys".to_owned()]);
    }

    fn supported() -> Facts {
        Facts {
            available: vec![1, 2, 3, 7],
            virt_firmware: Some(true),
            build: Some(26100),
            ..Facts::default()
        }
    }

    fn clean() -> Scan {
        Scan::default()
    }

    fn decide_with(id: &str, facts: &Facts, cpu: bool, scan: Scan) -> Decision {
        decide(id, facts, cpu, &mut || scan.clone())
    }

    #[test]
    fn memory_integrity_is_offered_only_when_every_safety_check_passes() {
        assert_eq!(
            decide_with(MEMORY_INTEGRITY, &supported(), false, clean()),
            Decision::Offer
        );
        // Already running: nothing to do.
        let mut f = supported();
        f.running = vec![2];
        assert_eq!(
            decide_with(MEMORY_INTEGRITY, &f, false, clean()),
            no(ALREADY_ON)
        );
        // Locked by the firmware, by either lock.
        for set in [
            |f: &mut Facts| f.lock_vbs = Some(1),
            |f: &mut Facts| f.lock_hvci = Some(1),
        ] {
            let mut f = supported();
            set(&mut f);
            assert_eq!(decide_with(MEMORY_INTEGRITY, &f, false, clean()), no(LOCKED));
        }
        // Lock value 0 is not a lock.
        let mut f = supported();
        f.lock_vbs = Some(0);
        f.lock_hvci = Some(0);
        assert_eq!(decide_with(MEMORY_INTEGRITY, &f, false, clean()), Decision::Offer);
    }

    #[test]
    fn missing_hardware_support_reads_as_a_calm_not_offered() {
        // A virtual machine without virtualization support reports nothing.
        let f = Facts::default();
        assert_eq!(
            decide_with(MEMORY_INTEGRITY, &f, true, clean()),
            no(NOT_SUPPORTED)
        );
        for broken in [
            |f: &mut Facts| f.available = vec![2],
            |f: &mut Facts| f.available = vec![1],
            |f: &mut Facts| f.virt_firmware = Some(false),
            |f: &mut Facts| f.virt_firmware = None,
            // A virtual machine's own hypervisor (VirtualBox, VMware) never
            // counts, even when it passes virtualization through.
            |f: &mut Facts| f.hypervisor_vendor = Some("VBoxVBoxVBox".into()),
            |f: &mut Facts| {
                f.hypervisor_vendor = Some("VMwareVMware".into());
                f.virt_firmware = None;
            },
        ] {
            let mut f = supported();
            broken(&mut f);
            assert_eq!(
                decide_with(MEMORY_INTEGRITY, &f, true, clean()),
                no(NOT_SUPPORTED)
            );
        }
        // Windows' own hypervisor is enough when the firmware flag is unknown.
        let mut f = supported();
        f.virt_firmware = None;
        f.hypervisor_vendor = Some(MICROSOFT_HV.into());
        assert_eq!(decide_with(MEMORY_INTEGRITY, &f, false, clean()), Decision::Offer);
        // So is a PC where virtualization-based security already runs.
        let mut f = supported();
        f.virt_firmware = None;
        f.vbs_status = Some(2);
        assert_eq!(decide_with(MEMORY_INTEGRITY, &f, false, clean()), Decision::Offer);
    }

    #[test]
    fn the_driver_scan_runs_last_and_names_the_drivers() {
        let mut ran = false;
        let f = Facts::default();
        let d = decide(MEMORY_INTEGRITY, &f, true, &mut || {
            ran = true;
            Scan::default()
        });
        assert_eq!(d, no(NOT_SUPPORTED));
        assert!(!ran, "the scan must not run when the hardware already rules it out");

        let scan = Scan {
            flagged: vec![
                ("old.sys".into(), vec![PeProblem::SectionAlignment]),
                ("older.sys".into(), vec![PeProblem::ImportTableExecutable]),
                ("old.sys".into(), vec![PeProblem::NotAnImage]),
            ],
            unreadable: vec![],
        };
        assert_eq!(
            decide_with(MEMORY_INTEGRITY, &supported(), true, scan),
            Decision::NotOffered(format!("{DRIVER}: old.sys, older.sys"))
        );
        let many = Scan {
            flagged: (0..8)
                .map(|n| (format!("d{n}.sys"), vec![PeProblem::NotAnImage]))
                .collect(),
            unreadable: vec![],
        };
        assert_eq!(
            decide_with(MEMORY_INTEGRITY, &supported(), true, many),
            Decision::NotOffered(format!("{DRIVER}: d0.sys, d1.sys, d2.sys, d3.sys, d4.sys"))
        );
        let unreadable = Scan {
            flagged: vec![],
            unreadable: vec!["x.sys".into()],
        };
        assert_eq!(
            decide_with(MEMORY_INTEGRITY, &supported(), true, unreadable),
            no(DRIVERS_UNREADABLE)
        );
    }

    #[test]
    fn stack_protection_needs_memory_integrity_running_and_a_capable_processor() {
        let mut f = supported();
        // What cannot change is named first, so the advice can be followed.
        assert_eq!(
            decide_with(STACK_PROTECTION, &Facts::default(), true, clean()),
            no(NOT_SUPPORTED)
        );
        assert_eq!(
            decide_with(STACK_PROTECTION, &f, false, clean()),
            no(NO_SHADOW_STACKS)
        );
        // Memory integrity not on at all.
        assert_eq!(
            decide_with(STACK_PROTECTION, &f, true, clean()),
            no(NEEDS_MEMORY_INTEGRITY)
        );
        // Turned on but waiting for the restart.
        f.configured = vec![2];
        assert_eq!(
            decide_with(STACK_PROTECTION, &f, true, clean()),
            no(NEEDS_RESTART)
        );
        // Running, but the processor has no shadow stacks.
        f.running = vec![2];
        assert_eq!(
            decide_with(STACK_PROTECTION, &f, false, clean()),
            no(NO_SHADOW_STACKS)
        );
        assert_eq!(decide_with(STACK_PROTECTION, &f, true, clean()), Decision::Offer);
        // An older Windows does not have the feature at all.
        let mut old = f.clone();
        old.build = Some(19045);
        assert_eq!(decide_with(STACK_PROTECTION, &old, true, clean()), no(OLD_WINDOWS));
        old.build = Some(STACK_PROTECTION_BUILD);
        assert_eq!(decide_with(STACK_PROTECTION, &old, true, clean()), Decision::Offer);
        // Its own lock or the general one.
        let mut locked = f.clone();
        locked.lock_stack = Some(1);
        assert_eq!(decide_with(STACK_PROTECTION, &locked, true, clean()), no(LOCKED));
        let mut locked = f.clone();
        locked.lock_vbs = Some(1);
        assert_eq!(decide_with(STACK_PROTECTION, &locked, true, clean()), no(LOCKED));
        // A memory integrity lock does not block stack protection by itself.
        let mut other = f.clone();
        other.lock_hvci = Some(1);
        assert_eq!(decide_with(STACK_PROTECTION, &other, true, clean()), Decision::Offer);
        // Already running (5); audit mode (6) is not running.
        let mut on = f.clone();
        on.running = vec![2, 5];
        assert_eq!(decide_with(STACK_PROTECTION, &on, true, clean()), no(ALREADY_ON));
        let mut audit = f;
        audit.running = vec![2, 6];
        assert_eq!(decide_with(STACK_PROTECTION, &audit, true, clean()), Decision::Offer);
        // Never scans drivers for this one.
        let mut ran = false;
        let mut g = supported();
        g.running = vec![2];
        decide(STACK_PROTECTION, &g, true, &mut || {
            ran = true;
            Scan::default()
        });
        assert!(!ran);
        assert_eq!(decide_with("other.id", &g, true, clean()), no(UNREADABLE));
    }

    #[test]
    fn a_restart_is_only_claimed_with_both_times_known() {
        assert!(restarted_since(Some(100), Some(200)));
        assert!(restarted_since(Some(100), Some(100)));
        assert!(!restarted_since(Some(200), Some(100)));
        assert!(!restarted_since(None, Some(100)));
        assert!(!restarted_since(Some(100), None));
    }

    #[test]
    fn a_setting_that_is_not_running_is_reported_with_the_start_time_and_drivers() {
        let mut f = supported();
        f.enabled_hvci = Some(1);
        f.boot_unix = Some(1000);
        f.blocked = vec!["bad.sys".into(), "not ok;rm.sys".into(), "second.sys".into()];
        let out = verification(&f);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].title, MEMORY_INTEGRITY_NOT_RUNNING);
        assert_eq!(out[0].status, "attention");
        assert_eq!(blocked_names(&out[0].detail).unwrap(), "bad.sys, second.sys");
        assert_eq!(boot_from_detail(&out[0].detail), Some(1000));
        assert_eq!(finding_control(&out[0].title), Some(MEMORY_INTEGRITY));
        // Without blocked drivers there is nothing to name.
        f.blocked.clear();
        let out = verification(&f);
        assert_eq!(blocked_names(&out[0].detail), None);
        // Unknown start time: say nothing.
        f.boot_unix = None;
        assert!(verification(&f).is_empty());
        f.boot_unix = Some(1000);
        // Running with nothing blocked: all well.
        f.running = vec![2];
        assert!(verification(&f).is_empty());
        // Not configured: nothing to verify.
        f.running.clear();
        f.enabled_hvci = Some(0);
        assert!(verification(&f).is_empty());
    }

    #[test]
    fn a_driver_blocked_while_memory_integrity_runs_is_reported_too() {
        let mut f = supported();
        f.enabled_hvci = Some(1);
        f.running = vec![2];
        f.boot_unix = Some(1000);
        f.blocked = vec!["pen.sys".into()];
        let out = verification(&f);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].title, DEVICE_BLOCKED);
        assert_eq!(finding_control(DEVICE_BLOCKED), Some(MEMORY_INTEGRITY));
        assert_eq!(blocked_names(&out[0].detail).unwrap(), "pen.sys");
        // Only when memory integrity is on.
        f.enabled_hvci = Some(0);
        assert!(verification(&f).is_empty());
    }

    #[test]
    fn stack_protection_is_only_checked_while_memory_integrity_runs() {
        let mut f = supported();
        f.enabled_stack = Some(1);
        f.running = vec![2];
        f.boot_unix = Some(1000);
        let out = verification(&f);
        assert_eq!(out[0].title, STACK_NOT_RUNNING);
        assert_eq!(finding_control(STACK_NOT_RUNNING), Some(STACK_PROTECTION));
        f.running = vec![2, 5];
        assert!(verification(&f).is_empty());
        // Memory integrity itself not running: its own finding comes first.
        f.running.clear();
        f.enabled_hvci = Some(1);
        let out = verification(&f);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].title, MEMORY_INTEGRITY_NOT_RUNNING);
    }

    #[test]
    fn settings_made_on_purpose_or_that_cannot_work_are_never_offered() {
        for set in [
            |f: &mut Facts| f.mandatory = Some(1),
            |f: &mut Facts| f.enable_vbs = Some(0),
        ] {
            let mut f = supported();
            set(&mut f);
            assert_eq!(decide_with(MEMORY_INTEGRITY, &f, true, clean()), no(SET_BY_HAND));
            f.running = vec![2];
            assert_eq!(decide_with(STACK_PROTECTION, &f, true, clean()), no(SET_BY_HAND));
        }
        // Required platform features the PC does not offer.
        let mut f = supported();
        f.require_platform = Some(3);
        f.available = vec![1, 2];
        assert_eq!(decide_with(MEMORY_INTEGRITY, &f, true, clean()), no(NOT_SUPPORTED));
        f.available = vec![1, 2, 3];
        assert_eq!(decide_with(MEMORY_INTEGRITY, &f, true, clean()), Decision::Offer);
        let mut f = supported();
        f.required = vec![1, 2, 4];
        assert_eq!(decide_with(MEMORY_INTEGRITY, &f, true, clean()), no(NOT_SUPPORTED));
        // Harmless values do not block.
        let mut f = supported();
        f.mandatory = Some(0);
        f.enable_vbs = Some(1);
        f.require_platform = Some(1);
        f.required = vec![1, 2];
        assert_eq!(decide_with(MEMORY_INTEGRITY, &f, true, clean()), Decision::Offer);
    }

    #[test]
    fn one_selection_is_split_so_each_core_protection_is_its_own_batch() {
        let ids: Vec<String> = ["a", STACK_PROTECTION, "b", MEMORY_INTEGRITY]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(
            split_batches(&ids),
            vec![
                vec!["a".to_owned(), "b".to_owned()],
                vec![STACK_PROTECTION.to_owned()],
                vec![MEMORY_INTEGRITY.to_owned()],
            ]
        );
        assert_eq!(split_batches(&ids[..1]), vec![vec!["a".to_owned()]]);
        assert!(split_batches(&[]).is_empty());
    }

    #[test]
    fn names_shown_to_people_are_always_plain() {
        assert_eq!(safe_name("a-b_1.sys").as_deref(), Some("a-b_1.sys"));
        for bad in ["", "a b.sys", "a;b", "x\u{202e}.sys", &"a".repeat(65)] {
            assert_eq!(safe_name(bad), None, "{bad}");
        }
        assert_eq!(
            reason_names(&format!("{DRIVER}: a.sys, b.sys")).as_deref(),
            Some("a.sys, b.sys")
        );
        assert_eq!(reason_names(DRIVER), None);
        assert_eq!(reason_names(&format!("{DRIVER}: <b>x</b>")), None);
        assert_eq!(reason_names("Not offered: something else: a.sys"), None);
        assert_eq!(blocked_names("no names here"), None);
        assert_eq!(blocked_names("x blocked: a;b"), None);
    }

    #[test]
    fn facts_decode_from_the_script_and_tolerate_missing_fields() {
        let f: Facts = serde_json::from_str(
            r#"{"available":[1,2],"running":[],"virtFirmware":true,"lockVbs":null,
                "enabledHvci":1,"bootUnix":1700000000,"blocked":["a.sys"],"extra":1}"#,
        )
        .unwrap();
        assert_eq!(f.available, vec![1, 2]);
        assert_eq!(f.virt_firmware, Some(true));
        assert_eq!(f.lock_vbs, None);
        assert_eq!(f.enabled_hvci, Some(1));
        assert_eq!(f.boot_unix, Some(1_700_000_000));
        let empty: Facts = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, Facts::default());
    }

    #[test]
    fn reasons_are_plain_and_stable() {
        for r in [
            NOT_SUPPORTED,
            LOCKED,
            DRIVER,
            DRIVERS_UNREADABLE,
            ALREADY_ON,
            NEEDS_MEMORY_INTEGRITY,
            NEEDS_RESTART,
            NO_SHADOW_STACKS,
            UNREADABLE,
            SET_BY_HAND,
            OLD_WINDOWS,
        ] {
            assert!(r.starts_with("Not offered: "));
            assert!(!r.contains('—'));
            assert!(r.len() < 90, "{r}");
        }
        assert!(is_vbs(MEMORY_INTEGRITY) && is_vbs(STACK_PROTECTION) && !is_vbs("vbs.running"));
    }
}
