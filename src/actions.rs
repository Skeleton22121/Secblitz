//! Explicit user-selected advisory actions, separate from reversible controls.
//! Calling `run` is the authorization boundary: callers obtain consent first.
use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    UpdateDefender,
    QuickScan,
    StartMonitoring,
    OpenWindowsUpdate,
    OpenWindowsSecurity,
    OpenSignInSettings,
    OpenEncryptionSettings,
    OpenTamperProtection,
    OpenProtectionHistory,
    OpenAppBrowserControl,
    OpenOptionalFeatures,
    OpenAccounts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionResult {
    pub status: String,
    pub detail: String,
}

#[cfg(windows)]
#[path = "actions/windows.rs"]
mod windows;

/// SHUTDOWN_RESTART: restart, do not power off. No force flags: programs with
/// unsaved work can still ask to stay open.
#[cfg(any(windows, test))]
const RESTART_FLAGS: u32 = 0x0000_0004;
/// SHTDN_REASON_MAJOR_OPERATINGSYSTEM | SHTDN_REASON_MINOR_SECURITYFIX |
/// SHTDN_REASON_FLAG_PLANNED: a planned restart for a security update.
#[cfg(any(windows, test))]
const RESTART_REASON: u32 = 0x0002_0000 | 0x0000_0012 | 0x8000_0000;

/// Restart the PC to finish installing updates. Only ever called after the
/// person confirmed, with a reminder to save their work first.
pub fn restart_for_updates() -> Result<()> {
    #[cfg(windows)]
    {
        windows::restart_for_updates()
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Restarting needs Windows")
    }
}

// No user-supplied URI, arguments, executable, or PowerShell is accepted.
fn settings_uri(action: Action) -> Option<&'static str> {
    match action {
        Action::OpenWindowsUpdate => Some("ms-settings:windowsupdate"),
        Action::OpenWindowsSecurity => Some("ms-settings:windowsdefender"),
        Action::OpenSignInSettings => Some("ms-settings:signinoptions"),
        Action::OpenEncryptionSettings => Some("ms-settings:deviceencryption"),
        Action::OpenTamperProtection => Some("windowsdefender://threatsettings"),
        Action::OpenProtectionHistory => Some("windowsdefender://threat"),
        Action::OpenAppBrowserControl => Some("windowsdefender://appbrowser"),
        Action::OpenOptionalFeatures => Some("ms-settings:optionalfeatures"),
        Action::OpenAccounts => Some("ms-settings:otherusers"),
        _ => None,
    }
}

#[cfg(any(windows, test))]
fn validate_settings_request(uri: &str, elevated: bool) -> Result<()> {
    anyhow::ensure!(
        matches!(
            uri,
            "ms-settings:windowsupdate"
                | "ms-settings:windowsdefender"
                | "ms-settings:signinoptions"
                | "ms-settings:deviceencryption"
                | "windowsdefender://threatsettings"
                | "windowsdefender://threat"
                | "windowsdefender://appbrowser"
                | "ms-settings:optionalfeatures"
                | "ms-settings:otherusers"
        ),
        "Unknown settings URI"
    );
    // An allowlisted URI still resolves through user-writable protocol handlers.
    // Never dispatch it with the elevated worker's token.
    anyhow::ensure!(
        !elevated,
        "Open Settings from the non-elevated interactive application"
    );
    Ok(())
}

/// Blocking, explicitly selected action. Errors never imply that in-flight
/// Defender work stopped. Settings dispatch does not verify remediation.
pub fn run(action: Action) -> Result<ActionResult> {
    if let Some(uri) = settings_uri(action) {
        #[cfg(windows)]
        windows::open_settings(uri)?;
        #[cfg(not(windows))]
        {
            let _ = uri;
            anyhow::bail!("Settings actions require Windows");
        }
        #[cfg(windows)]
        return Ok(ActionResult {
            status: "opened".into(),
            detail: "Windows accepted the settings-page request. Page availability and security settings are not verified; no fix is claimed.".into(),
        });
    }
    let (status, detail) = match action {
        Action::UpdateDefender => {
            crate::platform::support_action("defender_update")?;
            ("returned", "Defender's signature-update command returned successfully using its configured sources. This does not establish that signatures are the latest available.")
        }
        Action::QuickScan => {
            crate::platform::support_action("defender_quickscan")?;
            ("returned", "Defender's quick-scan command returned successfully. Completion and threat status are not independently verified; review Windows Security for results.")
        }
        Action::StartMonitoring => {
            if crate::service::query_status()?.state == crate::service::MonitorState::NotInstalled {
                crate::service::install()?;
            }
            crate::service::start().context("Monitor startup failed; the installed service was retained. Check service status before retrying")?;
            ("running", "SCM reports SecblitzMonitor Running. Monitoring is read-only; this does not verify report freshness or machine health.")
        }
        _ => anyhow::bail!("Settings actions require Windows"),
    };
    Ok(ActionResult {
        status: status.into(),
        detail: detail.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_is_a_planned_security_fix_restart_that_forces_nothing() {
        // SHUTDOWN_RESTART only: no SHUTDOWN_FORCE_OTHERS (0x1), FORCE_SELF (0x2)
        // or power-off flags, so unsaved work can still stop the restart.
        assert_eq!(RESTART_FLAGS, 0x4);
        assert_eq!(RESTART_REASON & 0x8000_0000, 0x8000_0000, "planned");
        assert_eq!(RESTART_REASON & 0x00FF_0000, 0x0002_0000, "operating system");
        assert_eq!(RESTART_REASON & 0xFFFF, 0x12, "security fix");
        #[cfg(not(windows))]
        assert!(restart_for_updates().is_err());
    }

    #[test]
    fn settings_allowlist_is_closed() {
        for (action, uri) in [
            (Action::OpenWindowsUpdate, "ms-settings:windowsupdate"),
            (Action::OpenWindowsSecurity, "ms-settings:windowsdefender"),
            (Action::OpenSignInSettings, "ms-settings:signinoptions"),
            (
                Action::OpenEncryptionSettings,
                "ms-settings:deviceencryption",
            ),
            (
                Action::OpenTamperProtection,
                "windowsdefender://threatsettings",
            ),
            (Action::OpenProtectionHistory, "windowsdefender://threat"),
            (
                Action::OpenAppBrowserControl,
                "windowsdefender://appbrowser",
            ),
            (Action::OpenOptionalFeatures, "ms-settings:optionalfeatures"),
            (Action::OpenAccounts, "ms-settings:otherusers"),
        ] {
            assert_eq!(settings_uri(action), Some(uri));
            validate_settings_request(uri, false).unwrap();
            assert!(validate_settings_request(uri, true).is_err());
        }
        for action in [
            Action::UpdateDefender,
            Action::QuickScan,
            Action::StartMonitoring,
        ] {
            assert_eq!(settings_uri(action), None);
        }
    }

    #[test]
    fn settings_boundary_rejects_executables_and_unlisted_uris() {
        for uri in [
            "",
            "cmd.exe",
            "https://example.com",
            "ms-settings:",
            "MS-SETTINGS:windowsupdate",
            "ms-settings:windowsupdate ",
            "ms-settings:windowsupdate\0",
            "ms-settings:windowsupdate & calc.exe",
            "windowsdefender://",
            "windowsdefender://threat/",
            "WINDOWSDEFENDER://threat",
            "windowsdefender://threat&calc.exe",
            "ms-settings:otherusers ",
        ] {
            for elevated in [false, true] {
                assert!(validate_settings_request(uri, elevated).is_err());
            }
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_actions_fail_without_claiming_success() {
        for action in [
            Action::UpdateDefender,
            Action::QuickScan,
            Action::StartMonitoring,
            Action::OpenWindowsUpdate,
            Action::OpenWindowsSecurity,
            Action::OpenSignInSettings,
            Action::OpenEncryptionSettings,
            Action::OpenTamperProtection,
            Action::OpenProtectionHistory,
            Action::OpenAppBrowserControl,
            Action::OpenOptionalFeatures,
            Action::OpenAccounts,
        ] {
            assert!(run(action).is_err());
        }
    }
}
