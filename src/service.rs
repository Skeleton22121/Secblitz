//! Optional, explicitly installed `SecblitzMonitor` Windows service (elevated caller only).
//! LocalService may write only `Monitor/latest.json` and `Status/status.json`; it never
//! opens the journal or remediates. Reports are unauthenticated diagnostic snapshots.

use anyhow::Result;

#[cfg(windows)]
#[path = "service/windows.rs"]
mod windows;

/// Trusted `Program Files/Secblitz/Status` directory, only when the running
/// executable is the installed copy.
#[cfg(windows)]
pub fn trusted_status_dir() -> Option<std::path::PathBuf> {
    windows::trusted_status_dir()
}

#[cfg(windows)]
pub fn ensure_status_dir() -> Result<std::path::PathBuf> {
    windows::ensure_status_dir()
}

pub fn install() -> Result<()> {
    #[cfg(windows)]
    {
        windows::install()
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Windows services are only supported on Windows")
    }
}

pub fn uninstall() -> Result<()> {
    #[cfg(windows)]
    {
        windows::uninstall()
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Windows services are only supported on Windows")
    }
}

pub fn start() -> Result<()> {
    #[cfg(windows)]
    {
        windows::start()
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Windows services are only supported on Windows")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorState {
    NotInstalled,
    Stopped,
    StartPending,
    StopPending,
    Running,
    ContinuePending,
    PausePending,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StatusDetails {
    pub state: MonitorState,
    pub win32_exit_code: Option<u32>,
    pub service_exit_code: Option<u32>,
    pub checkpoint: u32,
    pub wait_hint_ms: u64,
}

pub fn query_status() -> Result<StatusDetails> {
    #[cfg(windows)]
    {
        windows::status_details()
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Windows services are only supported on Windows")
    }
}

pub fn status_details() -> Result<String> {
    Ok(serde_json::to_string(&query_status()?)?)
}

pub fn status() -> Result<()> {
    query_status().map(|_| ())
}

pub fn run() -> Result<()> {
    #[cfg(windows)]
    {
        windows::run()
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Windows services are only supported on Windows")
    }
}

// Validate the lexical path before passing it to Win32. Kept platform-neutral
// so rejection of Win32 aliases, device paths and ADS can be tested on Linux.
#[cfg(any(windows, test))]
fn validate_path(path: &str) -> Result<()> {
    use anyhow::ensure;
    let bytes = path.as_bytes();
    ensure!(
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\',
        "Expected an absolute local drive path"
    );
    ensure!(
        !path.contains(['\0', '"', '/', '*', '?', '<', '>', '|'])
            && !path.chars().any(char::is_control),
        "Invalid Windows path characters"
    );
    if path.len() == 3 {
        return Ok(());
    }
    for part in path[3..].split('\\') {
        ensure!(
            !part.is_empty() && !part.ends_with(['.', ' ']) && !part.contains(':'),
            "Ambiguous Windows path component"
        );
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        ensure!(
            !matches!(
                stem.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            ) && !(stem.starts_with("COM") || stem.starts_with("LPT"))
                .then(|| &stem[3..])
                .is_some_and(|n| matches!(
                    n,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )),
            "Reserved Windows device name"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_is_language_neutral_serializable_data() {
        let status = StatusDetails {
            state: MonitorState::NotInstalled,
            win32_exit_code: None,
            service_exit_code: None,
            checkpoint: 0,
            wait_hint_ms: 0,
        };
        let value = serde_json::to_value(status).unwrap();
        assert_eq!(value["state"], "not_installed");
        assert!(value["win32_exit_code"].is_null());
        assert!(value["service_exit_code"].is_null());
        assert_eq!(
            serde_json::to_value(MonitorState::StopPending).unwrap(),
            "stop_pending"
        );
    }
    #[test]
    fn local_paths_only_without_win32_aliases() {
        for path in [
            r"C:\",
            r"C:\Program Files\Secblitz\secblitz.exe",
            r"D:\Apps\Secblitz",
        ] {
            validate_path(path).unwrap();
        }
        for path in [
            "",
            r"C:relative",
            r"\rooted",
            r"\\server\share",
            r"\\?\C:\Apps",
            r"C:\Apps\..\bin",
            r"C:\Apps\.\bin",
            r"C:\Apps\\bin",
            r"C:\Apps\",
            r"C:\Apps\file:stream",
            r"C:\Apps\name.",
            r"C:\Apps\name ",
            r"C:\NUL.exe",
            r"C:\COM1",
            r"C:\LPT².txt",
            "C:\\bad\0name",
            "C:\\bad\"name",
            "C:/Apps",
        ] {
            assert!(validate_path(path).is_err(), "accepted {path:?}");
        }
    }
    #[cfg(not(windows))]
    #[test]
    fn unsupported_platform_fails_explicitly() {
        assert!(install().is_err());
        assert!(start().is_err());
        assert!(uninstall().is_err());
        assert!(status().is_err());
        assert!(status_details().is_err());
        assert!(query_status().is_err());
        assert!(run().is_err());
    }
}
