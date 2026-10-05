//! Per-user GUI preferences (theme, language override) and the system
//! switches behind the Settings page (background checks, tray autostart).
//!
//! Preferences are a small JSON file in the engine state dir. Loading is
//! tolerant (a damaged file means defaults); saving is atomic.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    #[default]
    Light,
    Dark,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub theme: ThemeChoice,
    /// Two-letter language code, or None for the Windows display language.
    #[serde(default)]
    pub lang: Option<String>,
}

const FILE: &str = "gui-prefs.json";
/// Preferences are tiny; anything bigger is not ours.
const LIMIT: u64 = 8 * 1024;

fn path() -> anyhow::Result<PathBuf> {
    Ok(secblitz::platform::app_dir()?.join(FILE))
}

/// Parse untrusted bytes. Unknown fields and bad values fall back to defaults
/// one field at a time, so one bad value never resets the others.
pub fn parse(bytes: &[u8]) -> Prefs {
    let mut prefs = Prefs::default();
    if bytes.len() as u64 > LIMIT {
        return prefs;
    }
    let Ok(serde_json::Value::Object(map)) = serde_json::from_slice::<serde_json::Value>(bytes)
    else {
        return prefs;
    };
    match map.get("theme").and_then(|v| v.as_str()) {
        Some("dark") => prefs.theme = ThemeChoice::Dark,
        _ => prefs.theme = ThemeChoice::Light,
    }
    prefs.lang = map
        .get("lang")
        .and_then(|v| v.as_str())
        .filter(|code| crate::i18n::Lang::parse(code).is_some())
        .map(str::to_owned);
    prefs
}

fn read_bounded(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

pub fn load() -> Prefs {
    path()
        .ok()
        .and_then(|p| read_bounded(&p))
        .map(|bytes| parse(&bytes))
        .unwrap_or_default()
}

pub fn save(prefs: &Prefs) -> anyhow::Result<()> {
    write_to(&path()?, prefs)
}

fn write_to(path: &Path, prefs: &Prefs) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(prefs)?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })?;
    Ok(())
}

// ----- tray autostart -----

/// Run-key value that starts the tray agent at logon.
#[cfg_attr(not(windows), allow(dead_code))]
pub const TRAY_VALUE: &str = "SecblitzTray";

/// The command stored in the Run key.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn tray_command(exe: &Path) -> String {
    format!("\"{}\" tray", exe.display())
}

/// The running exe, but only when it is the installed copy
/// (`%ProgramFiles%\Secblitz\secblitz.exe`). A copy elsewhere must never
/// become a logon autostart target.
pub fn installed_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    #[cfg(windows)]
    {
        // Resolved with SHGetKnownFolderPath, never from the inherited environment.
        crate::service::trusted_status_dir()?;
        Some(exe)
    }
    #[cfg(not(windows))]
    {
        let base = std::env::var_os("ProgramFiles")?;
        let expected = Path::new(&base).join("Secblitz").join("secblitz.exe");
        same_path(&exe, &expected).then_some(exe)
    }
}

#[cfg_attr(windows, allow(dead_code))]
fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .replace('/', "\\")
        .eq_ignore_ascii_case(&b.to_string_lossy().replace('/', "\\"))
}

/// Is the tray set to start at logon?
pub fn tray_enabled() -> bool {
    #[cfg(windows)]
    {
        run_key::get().is_some()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn set_tray_enabled(on: bool) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if on {
            let exe = installed_exe()
                .ok_or_else(|| anyhow::anyhow!("Only the installed copy can start with Windows"))?;
            run_key::set(&tray_command(&exe))
        } else {
            run_key::remove()
        }
    }
    #[cfg(not(windows))]
    {
        let _ = on;
        anyhow::bail!("The taskbar icon is only available on Windows")
    }
}

#[cfg(windows)]
mod run_key {
    use super::TRAY_VALUE;
    use std::ptr::null_mut;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
        HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE, KEY_WOW64_64KEY, REG_SZ,
    };

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const ERROR_FILE_NOT_FOUND: u32 = 2;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn open(access: u32) -> Result<HKEY, u32> {
        let mut key: HKEY = null_mut();
        let path = wide(RUN);
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                path.as_ptr(),
                0,
                access | KEY_WOW64_64KEY,
                &mut key,
            )
        };
        if status == 0 {
            Ok(key)
        } else {
            Err(status)
        }
    }

    /// The stored command, if the value exists.
    pub fn get() -> Option<String> {
        let key = open(KEY_QUERY_VALUE).ok()?;
        let name = wide(TRAY_VALUE);
        let mut size = 0u32;
        let status = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut size,
            )
        };
        let out = if status == 0 && size > 0 && size <= 4096 {
            let mut buf = vec![0u16; (size as usize).div_ceil(2)];
            let mut size = (buf.len() * 2) as u32;
            let status = unsafe {
                RegQueryValueExW(
                    key,
                    name.as_ptr(),
                    null_mut(),
                    null_mut(),
                    buf.as_mut_ptr().cast(),
                    &mut size,
                )
            };
            (status == 0).then(|| {
                let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                String::from_utf16_lossy(&buf[..end])
            })
        } else {
            None
        };
        unsafe { RegCloseKey(key) };
        out
    }

    pub fn set(command: &str) -> anyhow::Result<()> {
        let key = open(KEY_SET_VALUE)
            .map_err(|s| anyhow::anyhow!("Cannot open the startup list ({s})"))?;
        let name = wide(TRAY_VALUE);
        let data = wide(command);
        let status = unsafe {
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        unsafe { RegCloseKey(key) };
        anyhow::ensure!(status == 0, "Cannot save the startup entry ({status})");
        Ok(())
    }

    pub fn remove() -> anyhow::Result<()> {
        let key = open(KEY_SET_VALUE)
            .map_err(|s| anyhow::anyhow!("Cannot open the startup list ({s})"))?;
        let name = wide(TRAY_VALUE);
        let status = unsafe { RegDeleteValueW(key, name.as_ptr()) };
        unsafe { RegCloseKey(key) };
        anyhow::ensure!(
            status == 0 || status == ERROR_FILE_NOT_FOUND,
            "Cannot remove the startup entry ({status})"
        );
        Ok(())
    }
}

// ----- background protection (monitor service) -----

/// Is the background check installed and running (or about to be)?
pub fn background_on() -> anyhow::Result<bool> {
    use secblitz::service::MonitorState;
    Ok(!matches!(
        secblitz::service::query_status()?.state,
        MonitorState::NotInstalled | MonitorState::Stopped
    ))
}

/// Install (if needed) and start the monitor. Blocking.
pub fn enable_background() -> anyhow::Result<()> {
    use secblitz::service::{self, MonitorState};
    if service::query_status()?.state == MonitorState::NotInstalled {
        service::install()?;
    }
    if !matches!(
        service::query_status()?.state,
        MonitorState::Running | MonitorState::StartPending
    ) {
        service::start()?;
    }
    Ok(())
}

/// Stop and remove the monitor registration. Blocking (up to ~30 s).
pub fn disable_background() -> anyhow::Result<()> {
    #[cfg(windows)]
    stop_monitor()?;
    secblitz::service::uninstall()
}

#[cfg(windows)]
fn stop_monitor() -> anyhow::Result<()> {
    use std::time::{Duration, Instant};
    use windows_service::{
        service::{ServiceAccess, ServiceState},
        service_manager::{ServiceManager, ServiceManagerAccess},
    };
    let scm = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = match scm.open_service(
        "SecblitzMonitor",
        ServiceAccess::QUERY_STATUS | ServiceAccess::STOP,
    ) {
        Ok(service) => service,
        // Already gone: nothing to stop.
        Err(windows_service::Error::Winapi(e)) if e.raw_os_error() == Some(1060) => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if service.query_status()?.current_state != ServiceState::Stopped {
        let _ = service.stop();
        let deadline = Instant::now() + Duration::from_secs(30);
        while service.query_status()?.current_state != ServiceState::Stopped {
            anyhow::ensure!(
                Instant::now() < deadline,
                "The background check did not stop in time"
            );
            std::thread::sleep(Duration::from_millis(300));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_is_tolerant_and_validates_fields() {
        assert_eq!(parse(b""), Prefs::default());
        assert_eq!(parse(b"not json"), Prefs::default());
        assert_eq!(parse(b"[1,2]"), Prefs::default());
        let p = parse(br#"{"theme":"dark","lang":"es","extra":1}"#);
        assert_eq!(p.theme, ThemeChoice::Dark);
        assert_eq!(p.lang.as_deref(), Some("es"));
        // One bad field never discards the other.
        let p = parse(br#"{"theme":"neon","lang":"fr"}"#);
        assert_eq!(p.theme, ThemeChoice::Light);
        assert_eq!(p.lang.as_deref(), Some("fr"));
        let p = parse(br#"{"theme":"dark","lang":"../../etc"}"#);
        assert_eq!(p.theme, ThemeChoice::Dark);
        assert_eq!(p.lang, None);
        // Oversized input is not ours.
        let big = format!(r#"{{"theme":"dark","pad":"{}"}}"#, "x".repeat(9000));
        assert_eq!(parse(big.as_bytes()), Prefs::default());
    }

    #[test]
    fn save_and_load_round_trip_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE);
        let prefs = Prefs {
            theme: ThemeChoice::Dark,
            lang: Some("de".into()),
        };
        write_to(&file, &prefs).unwrap();
        assert_eq!(parse(&read_bounded(&file).unwrap()), prefs);
        assert!(!dir.path().join("gui-prefs.json.tmp").exists());
        // Damaged files fall back to defaults instead of failing.
        std::fs::write(&file, b"{broken").unwrap();
        assert_eq!(parse(&read_bounded(&file).unwrap()), Prefs::default());
    }

    #[test]
    fn tray_command_quotes_the_exe_and_uses_only_the_tray_word() {
        let c = tray_command(Path::new(r"C:\Program Files\Secblitz\secblitz.exe"));
        assert_eq!(c, r#""C:\Program Files\Secblitz\secblitz.exe" tray"#);
    }

    #[test]
    fn path_comparison_ignores_case_and_slashes() {
        assert!(same_path(
            Path::new(r"C:\Program Files\Secblitz\secblitz.exe"),
            Path::new("c:/program files/secblitz/SECBLITZ.EXE")
        ));
        assert!(!same_path(
            Path::new(r"C:\Users\x\Downloads\secblitz.exe"),
            Path::new(r"C:\Program Files\Secblitz\secblitz.exe")
        ));
    }
}
