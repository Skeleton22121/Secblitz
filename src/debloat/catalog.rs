//! The compiled app catalog and the protected list.
//!
//! The package lists are inspired by Raphire/Win11Debloat and
//! ChrisTitusTech/winutil (both MIT licensed); names were re-checked by hand.
//! Order is part of the contract: the index is the stable id stored in the
//! journal and sent over the broker. Only ever append when adding apps in a
//! later release.
use super::{App, Group};

const fn app(
    family: &'static str,
    name: &'static str,
    group: Group,
    store_id: Option<&'static str>,
) -> App {
    App {
        family,
        name,
        group,
        store_id,
    }
}

/// `family` is the exact package name, or a known publisher prefix ending in `*`.
pub static CATALOG: &[App] = &[
    // Recommended
    app(
        "Microsoft.BingNews",
        "News",
        Group::Recommended,
        Some("9WZDNCRFHVFW"),
    ),
    app(
        "Microsoft.BingWeather",
        "Weather",
        Group::Recommended,
        Some("9WZDNCRFJ3Q2"),
    ),
    app(
        "Clipchamp.Clipchamp",
        "Clipchamp video editor",
        Group::Recommended,
        Some("9P1J8S7CCWWT"),
    ),
    app(
        "Microsoft.GetHelp",
        "Get Help",
        Group::Recommended,
        Some("9PKDZBMV1H3T"),
    ),
    app(
        "Microsoft.WindowsFeedbackHub",
        "Feedback Hub",
        Group::Recommended,
        Some("9NBLGGH4R32N"),
    ),
    app(
        "Microsoft.MicrosoftSolitaireCollection",
        "Solitaire games",
        Group::Recommended,
        Some("9WZDNCRFHWD2"),
    ),
    app(
        "Microsoft.MicrosoftOfficeHub",
        "Microsoft 365 offers",
        Group::Recommended,
        None,
    ),
    app(
        "Microsoft.PowerAutomateDesktop",
        "Power Automate",
        Group::Recommended,
        Some("9NFTCH6J7FHV"),
    ),
    app(
        "Microsoft.BingSearch",
        "Bing search helper",
        Group::Recommended,
        None,
    ),
    app(
        "Microsoft.WindowsMaps",
        "Maps",
        Group::Recommended,
        // Retired: Microsoft removed Maps from the Store in July 2025.
        None,
    ),
    app(
        "Microsoft.ZuneVideo",
        "Movies & TV",
        Group::Recommended,
        None,
    ),
    app(
        "Microsoft.3DBuilder",
        "3D Builder",
        Group::Recommended,
        None,
    ),
    app("Microsoft.Print3D", "Print 3D", Group::Recommended, None),
    app(
        "Microsoft.MixedReality.Portal",
        "Mixed Reality Portal",
        Group::Recommended,
        None,
    ),
    app("Microsoft.SkypeApp", "Skype", Group::Recommended, None),
    app("Microsoft.Messaging", "Messaging", Group::Recommended, None),
    app("Microsoft.People", "People", Group::Recommended, None),
    app(
        "Microsoft.Wallet",
        "Microsoft Pay",
        Group::Recommended,
        None,
    ),
    app(
        "Microsoft.MicrosoftJournal",
        "Journal",
        Group::Recommended,
        None,
    ),
    app("Microsoft.Getstarted", "Tips", Group::Recommended, None),
    app(
        "Microsoft.549981C3F5F10",
        "Cortana",
        Group::Recommended,
        None,
    ),
    // Sponsored
    app(
        "king.com.*",
        "Candy Crush and other King games",
        Group::Sponsored,
        None,
    ),
    app("SpotifyAB.SpotifyMusic", "Spotify", Group::Sponsored, None),
    app("Disney.37853FC22B2CE", "Disney+", Group::Sponsored, None),
    app("4DF9E0F8.Netflix", "Netflix", Group::Sponsored, None),
    app("BytedancePte.Ltd.TikTok", "TikTok", Group::Sponsored, None),
    app(
        "AmazonVideo.PrimeVideo",
        "Prime Video",
        Group::Sponsored,
        None,
    ),
    app(
        "Facebook.*",
        "Facebook and Instagram",
        Group::Sponsored,
        None,
    ),
    app("Duolingo-*", "Duolingo", Group::Sponsored, None),
    app("ROBLOXCORPORATION.ROBLOX", "Roblox", Group::Sponsored, None),
    app("9E2F88E3.Twitter", "X (Twitter)", Group::Sponsored, None),
    app(
        "AdobeSystemsIncorporated.AdobeExpress",
        "Adobe Express",
        Group::Sponsored,
        None,
    ),
    // Promotions
    app(
        "Microsoft.Copilot",
        "Copilot",
        Group::Promotions,
        Some("9NHT9RB2F4HD"),
    ),
    app(
        "Microsoft.Windows.Ai.Copilot.Provider",
        "Copilot helper",
        Group::Promotions,
        None,
    ),
    app(
        "MicrosoftWindows.Client.WebExperience",
        "Widgets",
        Group::Promotions,
        None,
    ),
    app(
        "MSTeams",
        "Microsoft Teams",
        Group::Promotions,
        Some("XP8BT8DW290MPQ"),
    ),
    app(
        "MicrosoftTeams",
        "Microsoft Teams (older version)",
        Group::Promotions,
        None,
    ),
    app(
        "Microsoft.OutlookForWindows",
        "New Outlook",
        Group::Promotions,
        Some("9NRX63209R7B"),
    ),
    app(
        "Microsoft.Windows.DevHome",
        "Dev Home",
        Group::Promotions,
        // Retired: its Store id now opens "Windows Advanced Settings".
        None,
    ),
    app(
        "Microsoft.Todos",
        "Microsoft To Do",
        Group::Promotions,
        Some("9NBLGGH5R558"),
    ),
    app(
        "Microsoft.YourPhone",
        "Phone Link",
        Group::Promotions,
        Some("9NMPJ99VJBWV"),
    ),
    app(
        "MicrosoftWindows.CrossDevice",
        "Phone Link helper",
        Group::Promotions,
        None,
    ),
    app(
        "Microsoft.StartExperiencesApp",
        "Start suggestions",
        Group::Promotions,
        None,
    ),
    // Utilities
    app(
        "Microsoft.WindowsAlarms",
        "Clock and alarms",
        Group::Utilities,
        Some("9WZDNCRFJ3PR"),
    ),
    app(
        "Microsoft.MicrosoftStickyNotes",
        "Sticky Notes",
        Group::Utilities,
        Some("9NBLGGH4QGHW"),
    ),
    app(
        "Microsoft.WindowsSoundRecorder",
        "Sound Recorder",
        Group::Utilities,
        Some("9WZDNCRFHWKN"),
    ),
    app(
        "Microsoft.ZuneMusic",
        "Media Player",
        Group::Utilities,
        Some("9WZDNCRFJ3PT"),
    ),
    app(
        "Microsoft.WindowsCamera",
        "Camera",
        Group::Utilities,
        Some("9WZDNCRFJBBG"),
    ),
    app(
        "Microsoft.Windows.Photos",
        "Photos",
        Group::Utilities,
        Some("9WZDNCRFJBH4"),
    ),
    app(
        "MicrosoftCorporationII.QuickAssist",
        "Quick Assist",
        Group::Utilities,
        Some("9P7BP5VNWKX5"),
    ),
    // Gaming
    app(
        "Microsoft.GamingApp",
        "Xbox app",
        Group::Gaming,
        Some("9MV0B5HZVK9Z"),
    ),
    app(
        "Microsoft.XboxGamingOverlay",
        "Game Bar",
        Group::Gaming,
        Some("9NZKPSTSNW4P"),
    ),
    app(
        "Microsoft.XboxGameOverlay",
        "Game Bar overlay",
        Group::Gaming,
        None,
    ),
    app(
        "Microsoft.XboxSpeechToTextOverlay",
        "Game Bar voice typing",
        Group::Gaming,
        None,
    ),
    app(
        "Microsoft.Xbox.TCUI",
        "Xbox sign-in screens",
        Group::Gaming,
        None,
    ),
    app(
        "Microsoft.XboxApp",
        "Xbox app (older version)",
        Group::Gaming,
        None,
    ),
];

/// Case-insensitive match of a package name against a catalog pattern.
pub fn matches(pattern: &str, package: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => starts_with_ci(package, prefix),
        None => pattern.eq_ignore_ascii_case(package),
    }
}

/// Exact names (case-insensitive) that must never be removed.
const PROTECTED_EXACT: &[&str] = &[
    "Microsoft.WindowsStore",
    "Microsoft.DesktopAppInstaller",
    "Microsoft.SecHealthUI",
    "Microsoft.WindowsTerminal",
    "Microsoft.WindowsNotepad",
    "Microsoft.WindowsCalculator",
    "Microsoft.ScreenSketch",
    "Microsoft.XboxIdentityProvider",
    "Microsoft.Windows.ShellExperienceHost",
    "Microsoft.Windows.StartMenuExperienceHost",
    "Microsoft.Windows.CloudExperienceHost",
    "Microsoft.StorePurchaseApp",
    "Microsoft.Services.Store.Engagement",
    "Microsoft.LockApp",
    "Microsoft.AAD.BrokerPlugin",
    "Microsoft.AccountsControl",
    "Microsoft.CredDialogHost",
    "Microsoft.ECApp",
    "Microsoft.Win32WebViewHost",
    "Microsoft.AsyncTextService",
    "Microsoft.BioEnrollment",
    "Microsoft.WebpImageExtension",
    "Microsoft.HEIFImageExtension",
    "Microsoft.VP9VideoExtensions",
    "Microsoft.WebMediaExtensions",
    "Microsoft.RawImageExtension",
    "Microsoft.HEVCVideoExtension",
    "Microsoft.AV1VideoExtension",
    "Microsoft.MPEG2VideoExtension",
];

/// Prefixes (case-insensitive) that must never be removed.
const PROTECTED_PREFIX: &[&str] = &[
    "Microsoft.VCLibs",
    "Microsoft.UI.Xaml",
    "Microsoft.NET",
    "Microsoft.WindowsAppRuntime",
    "Microsoft.MicrosoftEdge",
    "Microsoft.OneDrive",
    "Microsoft.WindowsStore",
    "Microsoft.Windows.Search",
    "MicrosoftWindows.Client.CBS",
    "MicrosoftWindows.Client.Core",
    "MicrosoftWindows.Client.FileExp",
    "MicrosoftWindows.Client.OOBE",
    "Windows.",
    "NcsiUwpApp",
];

fn starts_with_ci(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

fn ends_with_ci(s: &str, suffix: &str) -> bool {
    s.len() >= suffix.len()
        && s.as_bytes()[s.len() - suffix.len()..].eq_ignore_ascii_case(suffix.as_bytes())
}

/// True when this package must never be touched, whatever was requested.
pub fn is_protected(name: &str) -> bool {
    PROTECTED_EXACT.iter().any(|p| p.eq_ignore_ascii_case(name))
        || PROTECTED_PREFIX.iter().any(|p| starts_with_ci(name, p))
        // Codec packages: HEIF, VP9, WebMedia, Webp, AV1, HEVC, RawImage…
        || ends_with_ci(name, "Extension")
        || ends_with_ci(name, "Extensions")
        || ends_with_ci(name, ".Framework")
}

/// Package names are plain ASCII identifiers; refuse anything else early.
pub fn is_valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}

/// Which catalog entry (if any) owns this installed package. Protected and
/// malformed names never match.
pub fn owner(package: &str) -> Option<u16> {
    if !is_valid_package_name(package) || is_protected(package) {
        return None;
    }
    CATALOG
        .iter()
        .position(|app| matches(app.family, package))
        .map(|i| i as u16)
}
