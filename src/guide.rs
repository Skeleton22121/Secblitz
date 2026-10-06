//! Plain step-by-step help for the few things only the person can do in
//! Windows itself. Pure data: a guide names one fixed Windows page and a few
//! short numbered steps. It never carries an address, a command or a string
//! that reaches the system; opening a page goes through `Page::request`, the
//! fixed list of broker requests. All text is translation source keys.
use crate::broker::Request;
use secblitz::actions::Action;

/// A Windows page a guide can open. A closed set, never free text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    CoreIsolation,
    Firewall,
    DeviceSecurity,
    WindowsSecurity,
    VirusSettings,
    ProtectionHistory,
    AppBrowser,
    Encryption,
    BitLocker,
    FindMyDevice,
    SignIn,
    OtherUsers,
    WorkAccounts,
    Recovery,
    RemoteDesktop,
    WindowsUpdate,
    OptionalFeatures,
    Wifi,
}

impl Page {
    pub const ALL: [Page; 18] = [
        Page::CoreIsolation,
        Page::Firewall,
        Page::DeviceSecurity,
        Page::WindowsSecurity,
        Page::VirusSettings,
        Page::ProtectionHistory,
        Page::AppBrowser,
        Page::Encryption,
        Page::BitLocker,
        Page::FindMyDevice,
        Page::SignIn,
        Page::OtherUsers,
        Page::WorkAccounts,
        Page::Recovery,
        Page::RemoteDesktop,
        Page::WindowsUpdate,
        Page::OptionalFeatures,
        Page::Wifi,
    ];

    /// The page's name as Windows shows it. Also what to type in Start.
    pub fn name(self) -> &'static str {
        match self {
            Page::CoreIsolation => "Core isolation",
            Page::Firewall => "Firewall and network protection",
            Page::DeviceSecurity => "Device security",
            Page::WindowsSecurity => "Windows Security",
            Page::VirusSettings => "Virus and threat protection settings",
            Page::ProtectionHistory => "Virus and threat protection",
            Page::AppBrowser => "App and browser control",
            Page::Encryption => "Device encryption",
            Page::BitLocker => "BitLocker",
            Page::FindMyDevice => "Find my device",
            Page::SignIn => "Sign-in options",
            Page::OtherUsers => "Other users",
            Page::WorkAccounts => "Access work or school",
            Page::Recovery => "Recovery",
            Page::RemoteDesktop => "Remote Desktop",
            Page::WindowsUpdate => "Windows Update",
            Page::OptionalFeatures => "Optional features",
            Page::Wifi => "Wi-Fi settings",
        }
    }

    /// The visible button that opens it.
    pub fn button(self) -> &'static str {
        match self {
            Page::CoreIsolation => "Open Core isolation",
            Page::Firewall => "Open Firewall protection",
            Page::DeviceSecurity => "Open Device security",
            Page::WindowsSecurity => "Open Windows Security",
            Page::VirusSettings => "Open virus protection settings",
            Page::ProtectionHistory => "Open virus protection",
            Page::AppBrowser => "Open App and browser control",
            Page::Encryption => "Open Device encryption",
            Page::BitLocker => "Open BitLocker",
            Page::FindMyDevice => "Open Find my device",
            Page::SignIn => "Open Sign-in options",
            Page::OtherUsers => "Open Other users",
            Page::WorkAccounts => "Open Access work or school",
            Page::Recovery => "Open Recovery",
            Page::RemoteDesktop => "Open Remote Desktop",
            Page::WindowsUpdate => "Open Windows Update",
            Page::OptionalFeatures => "Open Optional features",
            Page::Wifi => "Open Wi-Fi settings",
        }
    }

    pub fn action(self) -> Action {
        match self {
            Page::CoreIsolation => Action::OpenCoreIsolation,
            Page::Firewall => Action::OpenFirewall,
            Page::DeviceSecurity => Action::OpenDeviceSecurity,
            Page::WindowsSecurity => Action::OpenWindowsSecurity,
            Page::VirusSettings => Action::OpenTamperProtection,
            Page::ProtectionHistory => Action::OpenProtectionHistory,
            Page::AppBrowser => Action::OpenAppBrowserControl,
            Page::Encryption => Action::OpenEncryptionSettings,
            Page::BitLocker => Action::OpenBitLocker,
            Page::FindMyDevice => Action::OpenFindMyDevice,
            Page::SignIn => Action::OpenSignInSettings,
            Page::OtherUsers => Action::OpenAccounts,
            Page::WorkAccounts => Action::OpenWorkAccounts,
            Page::Recovery => Action::OpenRecovery,
            Page::RemoteDesktop => Action::OpenRemoteDesktop,
            Page::WindowsUpdate => Action::OpenWindowsUpdate,
            Page::OptionalFeatures => Action::OpenOptionalFeatures,
            Page::Wifi => Action::OpenWifi,
        }
    }

    /// The broker request the launcher serves for this page.
    pub fn request(self) -> Request {
        match self {
            Page::CoreIsolation => Request::OpenCoreIsolation,
            Page::Firewall => Request::OpenFirewall,
            Page::DeviceSecurity => Request::OpenDeviceSecurity,
            Page::WindowsSecurity => Request::OpenWindowsSecurity,
            Page::VirusSettings => Request::OpenTamperProtection,
            Page::ProtectionHistory => Request::OpenProtectionHistory,
            Page::AppBrowser => Request::OpenAppBrowserControl,
            Page::Encryption => Request::OpenEncryption,
            Page::BitLocker => Request::OpenBitLocker,
            Page::FindMyDevice => Request::OpenFindMyDevice,
            Page::SignIn => Request::OpenSignIn,
            Page::OtherUsers => Request::OpenAccounts,
            Page::WorkAccounts => Request::OpenWorkAccounts,
            Page::Recovery => Request::OpenRecovery,
            Page::RemoteDesktop => Request::OpenRemoteDesktop,
            Page::WindowsUpdate => Request::OpenWindowsUpdate,
            Page::OptionalFeatures => Request::OpenOptionalFeatures,
            Page::Wifi => Request::OpenWifi,
        }
    }

    /// The page for a request, when it is one of these.
    #[cfg(test)]
    pub fn from_request(request: Request) -> Option<Page> {
        Page::ALL.into_iter().find(|p| p.request() == request)
    }

    /// The page for an `actions::Action`.
    pub fn from_action(action: Action) -> Option<Page> {
        Page::ALL.into_iter().find(|p| p.action() == action)
    }

    /// The page the older "next step" kinds point to.
    pub fn for_step(step: crate::advice::NextStep) -> Option<Page> {
        use crate::advice::NextStep as S;
        Some(match step {
            S::OpenWindowsSecurity => Page::WindowsSecurity,
            S::OpenWindowsUpdate => Page::WindowsUpdate,
            S::OpenEncryption => Page::Encryption,
            S::OpenAccounts => Page::SignIn,
            S::OpenRemoteDesktop => Page::RemoteDesktop,
            S::ReviewWindowsFeatures => Page::OptionalFeatures,
            S::ReviewFirmware => Page::Recovery,
            _ => return None,
        })
    }
}

/// Two to four short steps and the page they start from.
#[derive(Debug, PartialEq, Eq)]
pub struct Guide {
    pub page: Page,
    /// A second page some PCs need instead (for example BitLocker).
    pub alt: Option<Page>,
    pub steps: &'static [&'static str],
}

const fn g(page: Page, steps: &'static [&'static str]) -> Guide {
    Guide {
        page,
        alt: None,
        steps,
    }
}

static MEMORY_INTEGRITY: Guide = g(
    Page::CoreIsolation,
    &[
        "Find Memory integrity under Core isolation and turn it on.",
        "If Windows names a driver that blocks it, update that driver or its app, then try again.",
        "Restart your PC when Windows asks.",
    ],
);
static KERNEL_STACK: Guide = g(
    Page::CoreIsolation,
    &[
        "Turn on Memory integrity first. The next option appears below it.",
        "Turn on Kernel-mode Hardware-enforced Stack Protection.",
        "If you don't see it, your PC doesn't support it and nothing is wrong.",
        "Restart your PC when Windows asks.",
    ],
);
static ENCRYPTION: Guide = Guide {
    page: Page::Encryption,
    alt: Some(Page::BitLocker),
    steps: &[
        "Save your recovery key somewhere safe first. Microsoft accounts keep it at account.microsoft.com/devices/recoverykey.",
        "Turn on Device encryption.",
        "No Device encryption page? Use Open BitLocker instead, and choose Turn on BitLocker.",
    ],
};
static SECURE_BOOT: Guide = g(
    Page::Recovery,
    &[
        "Save your work, then select Restart now next to Advanced startup.",
        "After the restart choose Troubleshoot, Advanced options, UEFI Firmware Settings, then Restart.",
        "Find Secure Boot, often under Boot or Security, and turn it on. Menus differ by PC maker.",
        "Save and exit. If you're unsure, check your PC maker's guide first.",
    ],
);
static ACCOUNTS: Guide = Guide {
    page: Page::OtherUsers,
    alt: Some(Page::SignIn),
    steps: &[
        "Look at each account listed and remove the ones nobody uses.",
        "Open Sign-in options, choose Password, then Change, and set a strong password.",
        "Give every account its own password.",
    ],
};
static AUTOLOGON: Guide = g(
    Page::SignIn,
    &[
        "Press the Windows key and R together, type netplwiz and press Enter.",
        "Tick Users must enter a user name and password to use this computer, then select OK.",
        "Enter your password when asked.",
        "No such box? Turn off the Windows Hello only sign-in option in Sign-in options, then try again.",
    ],
);
static REMOTE_DESKTOP: Guide = g(
    Page::RemoteDesktop,
    &[
        "Turn off Remote Desktop.",
        "Select Confirm if Windows asks.",
        "Skip this if you connect to this PC from elsewhere on purpose.",
    ],
);
static SMB1: Guide = g(
    Page::OptionalFeatures,
    &[
        "Scroll down and select More Windows features.",
        "Untick SMB 1.0/CIFS File Sharing Support, then select OK.",
        "Restart your PC when Windows asks.",
    ],
);
static LIFECYCLE: Guide = g(
    Page::WindowsUpdate,
    &[
        "Select Check for updates.",
        "If a newer version of Windows is offered, choose Download and install.",
        "If none is offered, ask your PC maker how long it will get safety updates.",
    ],
);
static SECURE_BOOT_CERTS: Guide = g(
    Page::WindowsUpdate,
    &[
        "Select Check for updates and install everything offered.",
        "Restart when Windows asks. You may need to do this more than once.",
        "Then look for a startup (BIOS) update on your PC maker's website.",
    ],
);
static UPDATES: Guide = g(
    Page::WindowsUpdate,
    &[
        "Select Check for updates.",
        "Choose Download and install for anything offered.",
        "Restart when Windows asks.",
    ],
);
static REBOOT: Guide = g(
    Page::WindowsUpdate,
    &[
        "Save your work.",
        "Select Restart now when Windows shows that a restart is needed.",
    ],
);
static WORK: Guide = g(
    Page::WorkAccounts,
    &[
        "Look at the accounts listed. A work or school account means someone else may manage this PC.",
        "If one is yours to remove, select it and choose Disconnect. If you're not sure, leave it.",
    ],
);
static HELLO: Guide = g(
    Page::SignIn,
    &[
        "Select PIN (Windows Hello).",
        "Choose Set up and follow the steps.",
        "Face or fingerprint appear here too if your PC has the hardware.",
    ],
);
static FIND_MY_DEVICE: Guide = g(
    Page::FindMyDevice,
    &[
        "Turn on Find my device.",
        "It needs a Microsoft account. If the switch is greyed out, sign in with one first.",
    ],
);
static DAILY_ADMIN: Guide = Guide {
    page: Page::OtherUsers,
    alt: None,
    steps: &[
        "Choose Add account and make one for admin tasks only, with a strong password.",
        "Select it, choose Change account type, then Administrator.",
        "Sign in to it and set your everyday account to Standard user the same way.",
        "Use your everyday account from now on. Windows asks for the admin password when needed.",
    ],
};
static TAMPER: Guide = g(
    Page::VirusSettings,
    &[
        "Scroll to Tamper Protection and turn it on.",
        "Select Yes if Windows asks. Windows only lets a person turn this on.",
    ],
);
static WIFI: Guide = g(
    Page::Wifi,
    &[
        "Select your network, then its properties, to see its security type.",
        "Open your router's settings page. The address and password are often on a sticker on the router.",
        "Set the Wi-Fi security to WPA3, or WPA2 if WPA3 isn't listed, and save.",
    ],
);
static DNS: Guide = g(
    Page::Wifi,
    &[
        "Select your connection, then open its properties.",
        "Next to DNS server assignment select Edit, then choose Manual.",
        "Enter a provider that supports encrypted lookups, such as 1.1.1.1 or 8.8.8.8, and set DNS encryption to Encrypted preferred.",
        "If Secblitz Web protection is on, skip this. It already manages your lookups.",
    ],
);
static THREATS: Guide = g(
    Page::ProtectionHistory,
    &[
        "Select Protection history.",
        "Open each item Windows found and choose Actions.",
        "Choose Remove or Quarantine, then run a quick scan.",
    ],
);

/// The guide for a control id, a finding title or a diagnostics rule id.
pub fn guide(key: &str) -> Option<&'static Guide> {
    Some(match key {
        "vbs.memory_integrity" | "Memory integrity" => &MEMORY_INTEGRITY,
        "vbs.kernel_stack_protection" => &KERNEL_STACK,
        "Device encryption" => &ENCRYPTION,
        "Secure Boot" => &SECURE_BOOT,
        "Local accounts" => &ACCOUNTS,
        "accounts.autologon" | "Automatic logon" => &AUTOLOGON,
        "remote_desktop.disabled" | "Remote Desktop" => &REMOTE_DESKTOP,
        "smb1.disabled" | "SMB1" => &SMB1,
        "os.feature_release_support" | "Windows lifecycle" => &LIFECYCLE,
        "boot.secure_boot_certs" => &SECURE_BOOT_CERTS,
        "Windows updates" => &UPDATES,
        "update.reboot_overdue" => &REBOOT,
        "Management and mutation eligibility" => &WORK,
        "accounts.hello_configured" => &HELLO,
        "accounts.find_my_device" => &FIND_MY_DEVICE,
        "accounts.daily_admin" => &DAILY_ADMIN,
        "defender.tamper_protection" => &TAMPER,
        "net.wifi_security" => &WIFI,
        "net.dns_encryption" => &DNS,
        "defender.threats" => &THREATS,
        _ => return None,
    })
}

/// Finding titles that now have their own fix. When the check result for the
/// control exists, the fix row replaces the older manual row.
pub fn finding_control(title: &str) -> Option<&'static str> {
    Some(match title {
        "Memory integrity" => "vbs.memory_integrity",
        "Automatic logon" => "accounts.autologon",
        "Remote Desktop" => "remote_desktop.disabled",
        "SMB1" => "smb1.disabled",
        _ => return None,
    })
}

/// One short sentence for a page that would not open: what it is called and
/// how to reach it by hand.
pub fn failure_text(lang: crate::i18n::Lang, page: Page) -> String {
    lang.t("We couldn't open {page}. Press the Windows key, type {page} and press Enter.")
        .replace("{page}", &lang.t(page.name()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;

    const KEYS: &[&str] = &[
        "vbs.memory_integrity",
        "vbs.kernel_stack_protection",
        "Memory integrity",
        "Device encryption",
        "Secure Boot",
        "Local accounts",
        "accounts.autologon",
        "Automatic logon",
        "remote_desktop.disabled",
        "Remote Desktop",
        "smb1.disabled",
        "SMB1",
        "os.feature_release_support",
        "Windows lifecycle",
        "boot.secure_boot_certs",
        "Windows updates",
        "update.reboot_overdue",
        "Management and mutation eligibility",
        "accounts.hello_configured",
        "accounts.find_my_device",
        "accounts.daily_admin",
        "defender.tamper_protection",
        "net.wifi_security",
        "net.dns_encryption",
        "defender.threats",
    ];

    #[test]
    fn every_required_key_has_a_short_plain_guide() {
        for key in KEYS {
            let g = guide(key).unwrap_or_else(|| panic!("no guide for {key}"));
            assert!((2..=4).contains(&g.steps.len()), "{key}: 2 to 4 steps");
            for step in g.steps {
                assert!(step.len() <= 130, "{key}: keep steps short: {step}");
                assert!(!step.contains('\u{2014}'), "{key}: no em dash");
                assert!(step.ends_with('.'), "{key}: {step}");
            }
        }
        assert!(guide("nothing.here").is_none());
        assert!(guide("").is_none());
    }

    #[test]
    fn every_page_maps_to_one_request_and_one_action() {
        for (i, page) in Page::ALL.into_iter().enumerate() {
            assert_eq!(Page::from_request(page.request()), Some(page));
            assert_eq!(Page::from_action(page.action()), Some(page));
            assert!(page.button().starts_with("Open "));
            for other in &Page::ALL[i + 1..] {
                assert_ne!(page.request(), other.request());
                assert_ne!(page.action(), other.action());
                assert_ne!(page.button(), other.button());
            }
        }
    }

    #[test]
    fn guides_and_pages_are_translated_in_every_language() {
        let mut keys: Vec<&str> = Vec::new();
        for key in KEYS {
            let g = guide(key).unwrap();
            keys.extend(g.steps);
        }
        for page in Page::ALL {
            keys.push(page.name());
            keys.push(page.button());
        }
        keys.push("We couldn't open {page}. Press the Windows key, type {page} and press Enter.");
        // Product names stay as Microsoft spells them in every language.
        keys.retain(|k| !matches!(*k, "Windows Security" | "Windows Update" | "BitLocker"));
        for lang in [Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            for key in &keys {
                assert_ne!(lang.t(key), *key, "{}: {key}", lang.code());
            }
        }
    }

    #[test]
    fn failure_text_names_the_page_and_the_way_in() {
        let text = failure_text(Lang::En, Page::CoreIsolation);
        assert_eq!(
            text,
            "We couldn't open Core isolation. Press the Windows key, type Core isolation and press Enter."
        );
    }

    #[test]
    fn replaced_findings_point_to_their_control() {
        assert_eq!(finding_control("SMB1"), Some("smb1.disabled"));
        assert_eq!(finding_control("Secure Boot"), None);
    }
}
