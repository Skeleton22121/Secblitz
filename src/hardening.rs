//! Compiled catalog of the extended hardening controls (schema "items").
//!
//! Every control here observes a small, typed *slice* of machine state as
//! `{"items": {"<key>": <u32 | null>, ...}}` where `null` means "not configured".
//! The same table drives the engine (target, validation, eligibility, undo
//! domain), the platform wire validation and the PowerShell backend (the
//! backend receives the spec as JSON from [`Spec::script_json`], so Rust and
//! PowerShell can never disagree about what is safe).
//!
//! Rules of the model:
//! * A write may only move a key between an unsafe original and its fixed
//!   value ([`fix_of`]). Safe keys are never touched (nothing is "improved"
//!   beyond what we recorded), and a key that drifted to anything else
//!   blocks the write.
//! * Absent values that equal Windows' own safe default count as protected.
//! * Journal data names only keys of this table (or, for the two dynamic
//!   controls, keys that pass a strict name check and exist at write time).
use serde_json::{json, Map, Value};

/// How one key is judged and repaired.
#[derive(Clone, Copy, Debug)]
pub enum Rule {
    /// Safe when the value is in `safe` (or absent and `absent_safe`).
    /// Repair writes `fix`; `None` removes the value.
    Set {
        safe: &'static [u32],
        absent_safe: bool,
        fix: Option<u32>,
    },
    /// Firewall rule exposure: bit 8 = enabled, bit 4 = applies on the Public
    /// profile, bits 1|2 = Domain|Private. Unsafe when enabled on Public.
    Exposure,
}

#[derive(Clone, Copy, Debug)]
pub struct Key {
    pub name: &'static str,
    /// PowerShell registry path (registry-backed keys only).
    pub path: &'static str,
    /// Registry value name when it differs from `name` ("" = same as `name`).
    /// Lets one control hold the same value under several keys.
    pub value: &'static str,
    pub rule: Rule,
    /// Highest value this key can legally hold.
    pub max: u32,
    /// Exact legal values; empty means any value up to `max`.
    pub allowed: &'static [u32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Fixed registry DWORD values.
    Registry,
    /// Fixed Microsoft Defender preference properties.
    DefenderPref,
    /// Fixed Defender attack-surface-reduction rule ids.
    DefenderAsr,
    /// Local account lockout threshold.
    Lockout,
    /// Built-in Administrator (RID 500) enabled state.
    BuiltinAdmin,
    /// Dynamic: built-in file-sharing / discovery firewall rules.
    FirewallExposure,
    /// Dynamic: saved Wi-Fi profiles with a weak or no security type.
    WifiProfiles,
    /// Dynamic: NetBIOS-over-TCP/IP setting of each network adapter.
    NetbiosAdapters,
    /// One named outbound firewall rule owned by Secblitz (1 = present).
    FirewallOutbound,
    // --- OS / credentials / update / privacy controls ---
    /// System-wide exploit protection (DEP, SEHOP, ASLR, CFG): 0 off, 1 on, 2 default.
    ExploitMitigations,
    /// Windows PowerShell 2.0 optional feature: 1 installed, 0 removed or absent.
    PowerShellV2,
    /// Dynamic: leftover remote-access services (start type + running bit).
    LegacyServices,
    /// "Require sign-in when the PC wakes" on the active power plan (AC and DC).
    LockOnWake,
    /// Windows Update pause markers (minutes since 1970, 0 = nothing to resume).
    UpdatePause,
    /// SmartScreen for apps and files (string setting plus the policy value).
    SmartScreen,
    /// Dynamic: risky Microsoft Defender exclusions (1 present, 0 removed).
    DefenderExclusions,
}

/// Management and capability evidence the backend must find clean.
#[derive(Clone, Copy, Debug)]
pub struct Gate {
    /// PolicyManager (MDM) areas whose configured values mean "managed".
    pub areas: &'static [&'static str],
    /// Regex for value names within those areas.
    pub pattern: &'static str,
    /// Defender controls only: tamper protection does not block strengthening
    /// these (non tamper-protected) preferences.
    pub tamper_exempt: bool,
    /// Veto when a local security-settings template exists.
    pub secedit: bool,
    /// Veto when this policy key holds values/subkeys other than ours.
    pub own_policy_key: &'static str,
    /// (path, value) pairs whose presence means somebody else configures it.
    pub policy_values: &'static [(&'static str, &'static str)],
}

const NO_GATE: Gate = Gate {
    areas: &[],
    pattern: ".",
    tamper_exempt: false,
    secedit: false,
    own_policy_key: "",
    policy_values: &[],
};

/// Any policy for virtualization-based security, in the policy store or in
/// Mobile Device Management, means somebody else decides: assessment only.
const VBS_GATE: Gate = Gate {
    areas: &["DeviceGuard", "VirtualizationBasedTechnology"],
    own_policy_key: DEVICE_GUARD_POLICY,
    policy_values: &[
        (DEVICE_GUARD_POLICY, "EnableVirtualizationBasedSecurity"),
        (DEVICE_GUARD_POLICY, "HypervisorEnforcedCodeIntegrity"),
        (DEVICE_GUARD_POLICY, "RequirePlatformSecurityFeatures"),
        (DEVICE_GUARD_POLICY, "ConfigureKernelShadowStacksLaunch"),
    ],
    ..NO_GATE
};

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub source: Source,
    pub reboot: bool,
    /// A choice the person makes; never pre-selected.
    pub ask: bool,
    pub keys: &'static [Key],
    pub gate: Gate,
}

const fn set(
    name: &'static str,
    path: &'static str,
    safe: &'static [u32],
    absent_safe: bool,
    fix: Option<u32>,
    max: u32,
) -> Key {
    Key {
        name,
        path,
        value: "",
        rule: Rule::Set {
            safe,
            absent_safe,
            fix,
        },
        max,
        allowed: &[],
    }
}

const LSA: &str = r"HKLM:\SYSTEM\CurrentControlSet\Control\Lsa";
const PNP: &str = r"HKLM:\SOFTWARE\Policies\Microsoft\Windows NT\Printers\PointAndPrint";
const WU: &str = r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate";
const WU_AU: &str = r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU";
const SYSPOL: &str = r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\System";
const EXPLORER: &str = r"HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Explorer";

const LSA_MSV1: &str = r"HKLM:\SYSTEM\CurrentControlSet\Control\Lsa\MSV1_0";
const CI_CONFIG: &str = r"HKLM:\SYSTEM\CurrentControlSet\Control\CI\Config";
const PRINTERS_POLICY: &str = r"HKLM:\SOFTWARE\Policies\Microsoft\Windows NT\Printers";
const STORE_POLICY: &str = r"HKLM:\SOFTWARE\Policies\Microsoft\WindowsStore";
const WU_UX: &str = r"HKLM:\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings";
const EXPLORER_MACHINE: &str = r"HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer";
const WINDOWS_AI_POLICY: &str = r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsAI";
const DATA_COLLECTION_POLICY: &str = r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\DataCollection";
const DELIVERY_POLICY: &str = r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization";
const POWER_CONSOLELOCK_POLICY: &str =
    r"HKLM:\SOFTWARE\Policies\Microsoft\Power\PowerSettings\0e796bdb-100d-47d6-a2d5-f7d2daa51f51";
const DEVICE_GUARD_POLICY: &str = r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\DeviceGuard";
const HVCI_SCENARIO: &str = r"HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity";
const STACK_SCENARIO: &str =
    r"HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\KernelShadowStacks";
/// Exploit protection states: 0 off, 1 on, 2 not set (Windows default).
const MITIGATION_STATES: &[u32] = &[0, 1, 2];
/// Pause markers are minutes since 1970; the cap keeps them inside a PowerShell int.
const PAUSE_MAX: u32 = 2_000_000_000;

const ASR_ACTIONS: &[u32] = &[0, 1, 2, 6];
const fn asr(guid: &'static str, fix: u32) -> Key {
    Key {
        name: guid,
        path: "",
        value: "",
        rule: Rule::Set {
            safe: &[1, 6],
            absent_safe: false,
            fix: Some(fix),
        },
        max: 6,
        allowed: ASR_ACTIONS,
    }
}

/// A rule that must be Block (1) to count as protected.
const fn asr_block(guid: &'static str) -> Key {
    Key {
        name: guid,
        path: "",
        value: "",
        rule: Rule::Set {
            safe: &[1],
            absent_safe: false,
            fix: Some(1),
        },
        max: 6,
        allowed: ASR_ACTIONS,
    }
}

const SSL3_CLIENT: &str =
    r"HKLM:\SYSTEM\CurrentControlSet\Control\SecurityProviders\SCHANNEL\Protocols\SSL 3.0\Client";
const SSL3_SERVER: &str =
    r"HKLM:\SYSTEM\CurrentControlSet\Control\SecurityProviders\SCHANNEL\Protocols\SSL 3.0\Server";
const TLS10_CLIENT: &str =
    r"HKLM:\SYSTEM\CurrentControlSet\Control\SecurityProviders\SCHANNEL\Protocols\TLS 1.0\Client";
const TLS10_SERVER: &str =
    r"HKLM:\SYSTEM\CurrentControlSet\Control\SecurityProviders\SCHANNEL\Protocols\TLS 1.0\Server";
const TLS11_CLIENT: &str =
    r"HKLM:\SYSTEM\CurrentControlSet\Control\SecurityProviders\SCHANNEL\Protocols\TLS 1.1\Client";
const TLS11_SERVER: &str =
    r"HKLM:\SYSTEM\CurrentControlSet\Control\SecurityProviders\SCHANNEL\Protocols\TLS 1.1\Server";
const TLS_ENABLED_VALUES: &[u32] = &[0, 1, u32::MAX];
/// SCHANNEL `Enabled` is a DWORD where 0 is off and any other value (often
/// 0xFFFFFFFF) is on. Only 0 is safe; the exact original is journaled.
const fn tls_enabled(name: &'static str, path: &'static str) -> Key {
    Key {
        name,
        path,
        value: "Enabled",
        rule: Rule::Set {
            safe: &[0],
            absent_safe: false,
            fix: Some(0),
        },
        max: u32::MAX,
        allowed: TLS_ENABLED_VALUES,
    }
}
const fn tls_default_off(name: &'static str, path: &'static str) -> Key {
    Key {
        name,
        path,
        value: "DisabledByDefault",
        rule: Rule::Set {
            safe: &[1],
            absent_safe: false,
            fix: Some(1),
        },
        max: 1,
        allowed: &[],
    }
}
/// Cloud extended timeout in seconds (Defender accepts 0 to 50).
const CLOUD_TIMEOUT_SAFE: &[u32] = &[
    20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43,
    44, 45, 46, 47, 48, 49, 50,
];

const TCPIP: &str = r"HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters";
const TCPIP6: &str = r"HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip6\Parameters";

static SPECS: &[Spec] = &[
    Spec {
        id: "defender.cloud_protection",
        title: "Defender cloud-assisted detection",
        description: "Turn on Defender cloud protection and block-at-first-sight. Sends information about suspicious files to Microsoft. Unmanaged devices only; the original values are restored on undo.",
        source: Source::DefenderPref,
        reboot: false,
        ask: true,
        keys: &[
            set("MAPSReporting", "", &[1, 2], false, Some(2), 2),
            set("DisableBlockAtFirstSeen", "", &[0], false, Some(0), 1),
        ],
        gate: NO_GATE,
    },
    Spec {
        id: "defender.pua",
        title: "Defender unwanted-app blocking",
        description: "Turn on blocking of potentially unwanted applications. Audit-only or disabled settings are repaired; the original value is restored on undo.",
        source: Source::DefenderPref,
        reboot: false,
        ask: false,
        keys: &[set("PUAProtection", "", &[1], false, Some(1), 2)],
        gate: Gate { tamper_exempt: true, ..NO_GATE },
    },
    Spec {
        id: "defender.script_nis",
        title: "Defender script scanning and network inspection",
        description: "Repair only explicitly disabled script scanning or network attack inspection. Absent values already mean on.",
        source: Source::DefenderPref,
        reboot: false,
        ask: false,
        keys: &[
            set("DisableScriptScanning", "", &[0], true, Some(0), 1),
            set("DisableIntrusionPreventionSystem", "", &[0], true, Some(0), 1),
        ],
        gate: Gate { tamper_exempt: true, ..NO_GATE },
    },
    Spec {
        id: "defender.asr.standard",
        title: "Defender standard attack surface rules",
        description: "Block vulnerable-driver abuse, credential theft from LSASS and WMI persistence. Only rules that are off are set to Block; Block and Warn settings are preserved. Rules are added, never replaced as a list.",
        source: Source::DefenderAsr,
        reboot: false,
        ask: false,
        keys: &[
            asr("56a863a9-875e-4185-98a7-b882c64b5ce5", 1),
            asr("9e6c4e1f-7d60-472f-ba1a-a39ef669e4b2", 1),
            asr("e6db77e5-3df2-4cf1-b95a-636979351e5b", 1),
        ],
        gate: Gate { tamper_exempt: true, ..NO_GATE },
    },
    Spec {
        id: "defender.asr.web_script_email",
        title: "Defender script and email attachment rules",
        description: "Ask before scripts or email attachments launch programs (Warn mode, the person can allow each time). Rules already on are preserved.",
        source: Source::DefenderAsr,
        reboot: false,
        ask: true,
        keys: &[
            asr("d3e037e1-3eb8-44c8-a917-57927947596d", 6),
            asr("5beb7efe-fd9a-4556-801d-275e5ffc04cc", 6),
            asr("be9ba2d9-53ea-4cdc-84e5-9b1eeee46550", 6),
        ],
        gate: Gate { tamper_exempt: true, ..NO_GATE },
    },
    Spec {
        id: "lsa.run_as_ppl",
        title: "Run the sign-in service as a protected process",
        description: "Set RunAsPPL to 2 (protected, without a firmware lock) only when Secure Boot is on, no add-on would be blocked and no third-party sign-in packages exist. Value 1 (firmware lock) is never written. Restart required.",
        source: Source::Registry,
        reboot: true,
        ask: true,
        keys: &[Key {
            allowed: &[0, 1, 2],
            ..set("RunAsPPL", LSA, &[1, 2], false, Some(2), 2)
        }],
        gate: Gate {
            areas: &["LocalSecurityAuthority"],
            ..NO_GATE
        },
    },
    Spec {
        id: "net.public_sharing_exposure",
        title: "Hide file sharing and discovery on public networks",
        description: "Remove the Public profile from enabled built-in file and printer sharing and network discovery rules (or disable a rule that is Public-only). Rules are never deleted and the network type is never changed.",
        source: Source::FirewallExposure,
        reboot: false,
        ask: false,
        keys: &[Key {
            name: "*",
            path: "",
            value: "",
            rule: Rule::Exposure,
            max: 15,
            allowed: &[],
        }],
        gate: NO_GATE,
    },
    Spec {
        id: "printer.point_and_print",
        title: "Printer driver install without permission",
        description: "Remove only insecure Point and Print policy values so Windows uses its protected default. Unrelated printer policy and the Print Spooler are untouched.",
        source: Source::Registry,
        reboot: false,
        ask: false,
        keys: &[
            set("RestrictDriverInstallationToAdministrators", PNP, &[1], true, None, 1),
            set("NoWarningNoElevationOnInstall", PNP, &[0], true, None, 1),
            set("UpdatePromptSettings", PNP, &[0, 1], true, None, 2),
        ],
        gate: Gate {
            areas: &["Printers", "ADMX_Printing"],
            own_policy_key: PNP,
            ..NO_GATE
        },
    },
    Spec {
        id: "net.llmnr",
        title: "Turn off LLMNR name lookups",
        description: "Set the DNS Client EnableMulticast policy to 0 so a stranger on the network cannot answer name lookups. Undo removes the value again. Restart recommended.",
        source: Source::Registry,
        reboot: true,
        ask: false,
        keys: &[set(
            "EnableMulticast",
            r"HKLM:\SOFTWARE\Policies\Microsoft\Windows NT\DNSClient",
            &[0],
            false,
            Some(0),
            1,
        )],
        gate: Gate {
            areas: &["ADMX_DnsClient"],
            own_policy_key: r"HKLM:\SOFTWARE\Policies\Microsoft\Windows NT\DNSClient",
            ..NO_GATE
        },
    },
    Spec {
        id: "accounts.lockout_policy",
        title: "Lock out password guessers",
        description: "Where local accounts never lock, set the lockout threshold to 10 failed sign-ins. Lockout duration and window are not changed.",
        source: Source::Lockout,
        reboot: false,
        ask: false,
        keys: &[set(
            "LockoutThreshold",
            "",
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            false,
            Some(10),
            999,
        )],
        gate: Gate {
            areas: &["DeviceLock"],
            secedit: true,
            ..NO_GATE
        },
    },
    Spec {
        id: "autorun.disabled",
        title: "Stop USB and disc auto-run",
        description: "Set NoDriveTypeAutoRun to 255 and NoAutorun to 1 so inserted drives never start anything on their own. Takes effect after sign-out or restart.",
        source: Source::Registry,
        reboot: true,
        ask: true,
        keys: &[
            set("NoDriveTypeAutoRun", EXPLORER, &[255], false, Some(255), 255),
            set("NoAutorun", EXPLORER, &[1], false, Some(1), 1),
        ],
        gate: Gate {
            areas: &["ADMX_AutoPlay", "Autoplay"],
            ..NO_GATE
        },
    },
    Spec {
        id: "wifi.risky_profiles",
        title: "Saved Wi-Fi networks that join by themselves",
        description: "Set saved open, WEP or WPA-TKIP Wi-Fi networks to connect manually. Networks are never deleted; passwords are never read.",
        source: Source::WifiProfiles,
        reboot: false,
        ask: true,
        keys: &[set("*", "", &[0], false, Some(0), 1)],
        gate: NO_GATE,
    },
    Spec {
        id: "lsa.restrict_anonymous",
        title: "Block anonymous account and share listing",
        description: "Set RestrictAnonymous to 1 and keep Everyone from including anonymous users and null sessions off. Very old devices may no longer list shared folders. Restart required.",
        source: Source::Registry,
        reboot: true,
        ask: true,
        keys: &[
            set("RestrictAnonymous", LSA, &[1, 2], false, Some(1), 2),
            set("EveryoneIncludesAnonymous", LSA, &[0], true, Some(0), 1),
            set(
                "RestrictNullSessAccess",
                r"HKLM:\SYSTEM\CurrentControlSet\Services\LanmanServer\Parameters",
                &[1],
                true,
                Some(1),
                1,
            ),
        ],
        gate: Gate {
            areas: &["LocalPoliciesSecurityOptions"],
            pattern: "^NetworkAccess_",
            secedit: true,
            ..NO_GATE
        },
    },
    Spec {
        id: "remote_assistance.disabled",
        title: "Block unsolicited Remote Assistance",
        description: "Set fAllowToGetHelp to 0 so nobody can be invited to take over this PC through Remote Assistance. Quick Assist is separate and unaffected.",
        source: Source::Registry,
        reboot: false,
        ask: true,
        keys: &[set(
            "fAllowToGetHelp",
            r"HKLM:\SYSTEM\CurrentControlSet\Control\Remote Assistance",
            &[0],
            false,
            Some(0),
            1,
        )],
        gate: Gate {
            areas: &["RemoteAssistance", "ADMX_RemoteAssistance"],
            policy_values: &[(
                r"HKLM:\SOFTWARE\Policies\Microsoft\Windows NT\Terminal Services",
                "fAllowToGetHelp",
            )],
            ..NO_GATE
        },
    },
    Spec {
        id: "wsh.disabled",
        title: "Turn off Windows Script Host",
        description: "Set Windows Script Host Enabled to 0 so .vbs and .js files cannot be run by double-clicking. Undo removes the value.",
        source: Source::Registry,
        reboot: false,
        ask: true,
        keys: &[set(
            "Enabled",
            r"HKLM:\SOFTWARE\Microsoft\Windows Script Host\Settings",
            &[0],
            false,
            Some(0),
            1,
        )],
        gate: NO_GATE,
    },
    Spec {
        id: "update.auto_policy_disabled",
        title: "Windows automatic updates switched off by a setting",
        description: "Remove locally set update blockers (NoAutoUpdate=1, AUOptions=1, DisableWindowsUpdateAccess=1). Update servers or managed devices are left alone; undo restores the values.",
        source: Source::Registry,
        reboot: false,
        ask: true,
        keys: &[
            set("NoAutoUpdate", WU_AU, &[0], true, None, 1),
            Key {
                allowed: &[0, 1, 2, 3, 4, 5],
                ..set("AUOptions", WU_AU, &[0, 2, 3, 4, 5], true, None, 5)
            },
            set("DisableWindowsUpdateAccess", WU, &[0], true, None, 1),
        ],
        gate: Gate {
            areas: &["Update", "ADMX_WindowsUpdate"],
            policy_values: &[
                (WU, "WUServer"),
                (WU, "WUStatusServer"),
                (WU, "SetPolicyDrivenUpdateSourceForFeatureUpdates"),
                (WU, "SetPolicyDrivenUpdateSourceForQualityUpdates"),
                (WU_AU, "UseWUServer"),
            ],
            ..NO_GATE
        },
    },
    Spec {
        id: "ntlm.lm_compat_level",
        title: "Accept only modern NTLM sign-in",
        description: "Set LmCompatibilityLevel to 5 (NTLMv2 only). Old network drives or scanners may stop signing in. Restart required.",
        source: Source::Registry,
        reboot: true,
        ask: true,
        keys: &[set("LmCompatibilityLevel", LSA, &[5], false, Some(5), 5)],
        gate: Gate {
            areas: &["LocalPoliciesSecurityOptions"],
            pattern: "^NetworkSecurity_LANManagerAuthenticationLevel",
            secedit: true,
            ..NO_GATE
        },
    },
    Spec {
        id: "accounts.builtin_administrator",
        title: "Hidden built-in Administrator account",
        description: "Disable the built-in Administrator (RID 500) account when another enabled administrator exists. Never deletes it; undo enables it again.",
        source: Source::BuiltinAdmin,
        reboot: false,
        ask: true,
        keys: &[set("Enabled", "", &[0], false, Some(0), 1)],
        gate: Gate {
            areas: &["LocalPoliciesSecurityOptions"],
            pattern: "^Accounts_EnableAdministratorAccountStatus",
            secedit: true,
            ..NO_GATE
        },
    },
    Spec {
        id: "privacy.activity_history",
        title: "Windows activity history",
        description: "Stop Windows collecting and syncing an activity timeline (EnableActivityFeed, PublishUserActivities, UploadUserActivities = 0).",
        source: Source::Registry,
        reboot: false,
        ask: true,
        keys: &[
            set("EnableActivityFeed", SYSPOL, &[0], false, Some(0), 1),
            set("PublishUserActivities", SYSPOL, &[0], false, Some(0), 1),
            set("UploadUserActivities", SYSPOL, &[0], false, Some(0), 1),
        ],
        gate: Gate {
            areas: &["Privacy"],
            pattern: "^(EnableActivityFeed|PublishUserActivities|UploadUserActivities)",
            ..NO_GATE
        },
    },
    Spec {
        id: "privacy.advertising_id",
        title: "Advertising ID",
        description: "Set the DisabledByGroupPolicy advertising-ID policy to 1 so apps cannot track you across apps for ads.",
        source: Source::Registry,
        reboot: false,
        ask: true,
        keys: &[set(
            "DisabledByGroupPolicy",
            r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\AdvertisingInfo",
            &[1],
            false,
            Some(1),
            1,
        )],
        gate: Gate {
            areas: &["Privacy"],
            pattern: "^AllowAdvertisingId",
            ..NO_GATE
        },
    },
    Spec {
        id: "defender.asr.office",
        title: "Defender Office attack rules",
        description: "Block Office from starting other programs, writing risky files, injecting code or having its apps launch child programs. Only offered when Office is installed. Rules are added, never replaced as a list.",
        source: Source::DefenderAsr,
        reboot: false,
        ask: true,
        keys: &[
            asr_block("d4f940ab-401b-4efc-aadc-ad5f3c50688a"),
            asr_block("3b576869-a4ec-4529-8536-b80a7769e899"),
            asr_block("75668c1f-73b5-4cf0-bb93-3ecf5cb7cc84"),
            asr_block("26190899-1602-49e8-8b27-eb1d0a1ce869"),
        ],
        gate: Gate { tamper_exempt: true, ..NO_GATE },
    },
    Spec {
        id: "defender.asr.ransomware_usb",
        title: "Defender ransomware and USB rules",
        description: "Ask before unknown programs from USB drives run and use extra ransomware protection (Warn mode, the person can allow each time). Needs cloud protection. Rules are added, never replaced as a list.",
        source: Source::DefenderAsr,
        reboot: false,
        ask: true,
        keys: &[
            asr("c1db55ab-c21a-4637-bb3f-a12568109d35", 6),
            asr("b2b3f03d-6a65-4f7b-a9c7-1c7ef74a9ba4", 6),
        ],
        gate: Gate { tamper_exempt: true, ..NO_GATE },
    },
    Spec {
        id: "defender.network_protection",
        title: "Defender network protection",
        description: "Turn on Defender network protection so programs cannot reach known harmful websites and servers. Windows Pro and Enterprise only; the original value is restored on undo.",
        source: Source::DefenderPref,
        reboot: false,
        ask: true,
        keys: &[set("EnableNetworkProtection", "", &[1], false, Some(1), 2)],
        gate: Gate { tamper_exempt: true, ..NO_GATE },
    },
    Spec {
        id: "defender.cloud_block_level",
        title: "Defender stricter cloud blocking",
        description: "Set the Defender cloud block level to High and allow up to 20 extra seconds for a cloud verdict on unknown files. Zero tolerance is never set. Original values are restored on undo.",
        source: Source::DefenderPref,
        reboot: false,
        ask: true,
        keys: &[
            Key {
                allowed: &[0, 1, 2, 4, 6],
                ..set("CloudBlockLevel", "", &[2, 4, 6], false, Some(2), 6)
            },
            set("CloudExtendedTimeout", "", CLOUD_TIMEOUT_SAFE, false, Some(20), 50),
        ],
        gate: Gate { tamper_exempt: true, ..NO_GATE },
    },
    Spec {
        id: "net.stack_hardening",
        title: "Harden how this PC handles network traffic",
        description: "Ignore ICMP redirects, refuse source-routed packets (IPv4 and IPv6) and stop answering name-release requests. Undo restores the original values. Restart required.",
        source: Source::Registry,
        reboot: true,
        ask: true,
        keys: &[
            set("EnableICMPRedirect", TCPIP, &[0], false, Some(0), 1),
            set("DisableIPSourceRouting", TCPIP, &[2], false, Some(2), 2),
            Key {
                value: "DisableIPSourceRouting",
                ..set("DisableIPSourceRouting6", TCPIP6, &[2], false, Some(2), 2)
            },
            set(
                "NoNameReleaseOnDemand",
                r"HKLM:\SYSTEM\CurrentControlSet\Services\NetBT\Parameters",
                &[1],
                false,
                Some(1),
                1,
            ),
        ],
        gate: NO_GATE,
    },
    Spec {
        id: "net.netbios",
        title: "Turn off the old NetBIOS name service",
        description: "Set NetBIOS over TCP/IP to Disabled on each network adapter. Only offered when the old file-sharing version is off and no mapped drive or shared folder uses a bare computer name. Undo restores every adapter.",
        source: Source::NetbiosAdapters,
        reboot: false,
        ask: true,
        keys: &[Key {
            allowed: &[0, 1, 2],
            ..set("*", "", &[2], false, Some(2), 2)
        }],
        gate: NO_GATE,
    },
    Spec {
        id: "net.mdns",
        title: "Turn off multicast name lookups (mDNS)",
        description: "Set EnableMDNS to 0 so this PC neither asks nor answers local-network name lookups. Casting, AirPrint and some smart-home devices may stop being found. Undo restores the original value. Restart required.",
        source: Source::Registry,
        reboot: true,
        ask: true,
        keys: &[set(
            "EnableMDNS",
            r"HKLM:\SYSTEM\CurrentControlSet\Services\Dnscache\Parameters",
            &[0],
            false,
            Some(0),
            1,
        )],
        gate: NO_GATE,
    },
    Spec {
        id: "net.wpad",
        title: "Stop automatic proxy discovery (WPAD)",
        description: "Set DisableWpad to 1 so Windows no longer searches the network for a proxy script. The proxy service itself is left running. Undo restores the original value. Restart required.",
        source: Source::Registry,
        reboot: true,
        ask: true,
        keys: &[set(
            "DisableWpad",
            r"HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Internet Settings\WinHttp",
            &[1],
            false,
            Some(1),
            1,
        )],
        gate: NO_GATE,
    },
    Spec {
        id: "firewall.outbound_smb_internet",
        title: "Block file sharing to the internet",
        description: "Add one firewall rule, \"Secblitz: block outbound file sharing to the internet\", that blocks TCP ports 445 and 139 to internet addresses. Home-network file sharing is unaffected. Undo removes the rule.",
        source: Source::FirewallOutbound,
        reboot: false,
        ask: true,
        keys: &[Key {
            allowed: &[0, 1],
            ..set("RulePresent", "", &[1], false, Some(1), 1)
        }],
        gate: NO_GATE,
    },
    Spec {
        id: "tls.legacy_protocols",
        title: "Turn off old secure-connection versions",
        description: "Turn off SSL 3.0, TLS 1.0 and TLS 1.1 (client and server) in Windows. Current browsers are unaffected; very old apps or devices may fail to connect. Undo restores the original values. Restart required.",
        source: Source::Registry,
        reboot: true,
        ask: true,
        keys: &[
            tls_enabled("ssl3.client.enabled", SSL3_CLIENT),
            tls_default_off("ssl3.client.default_off", SSL3_CLIENT),
            tls_enabled("ssl3.server.enabled", SSL3_SERVER),
            tls_default_off("ssl3.server.default_off", SSL3_SERVER),
            tls_enabled("tls10.client.enabled", TLS10_CLIENT),
            tls_default_off("tls10.client.default_off", TLS10_CLIENT),
            tls_enabled("tls10.server.enabled", TLS10_SERVER),
            tls_default_off("tls10.server.default_off", TLS10_SERVER),
            tls_enabled("tls11.client.enabled", TLS11_CLIENT),
            tls_default_off("tls11.client.default_off", TLS11_CLIENT),
            tls_enabled("tls11.server.enabled", TLS11_SERVER),
            tls_default_off("tls11.server.default_off", TLS11_SERVER),
        ],
        gate: NO_GATE,
    },
    // ======================================================================
    // OS / credentials / update / privacy controls (system area).
    // Keep this block self-contained: other areas append their own blocks.
    // ======================================================================
    Spec {
        id: "ntlm.extras",
        title: "No stored old password hashes, no anonymous sign-in fallback",
        description: "Keep Windows from storing the old LM password hash (NoLMHash=1) and from letting the system account fall back to an anonymous sign-in (allownullsessionfallback=0). Absent values already mean safe on current Windows. UseMachineId is deliberately not set because it can break network drives. Restart required.",
        source: Source::Registry,
        reboot: true,
        ask: true,
        keys: &[
            set("NoLMHash", LSA, &[1], true, Some(1), 1),
            set("allownullsessionfallback", LSA_MSV1, &[0], true, Some(0), 1),
        ],
        gate: Gate {
            areas: &["LocalPoliciesSecurityOptions"],
            pattern: "^NetworkSecurity_(DoNotStoreLANManager|AllowLocalSystemNULL)",
            secedit: true,
            ..NO_GATE
        },
    },
    Spec {
        id: "driver.vulnerable_blocklist",
        title: "Block known-dangerous drivers",
        description: "Turn Windows' list of known-dangerous drivers back on (VulnerableDriverBlocklistEnable=1). A missing value already means on. Old hardware tools may stop loading a driver. Restart required.",
        source: Source::Registry,
        reboot: true,
        ask: true,
        keys: &[set("VulnerableDriverBlocklistEnable", CI_CONFIG, &[1], true, Some(1), 1)],
        gate: NO_GATE,
    },
    Spec {
        id: "system.exploit_mitigations",
        title: "Windows built-in memory protections",
        description: "Turn back on only the system-wide exploit protections (DEP, SEHOP, bottom-up ASLR, high-entropy ASLR, Control Flow Guard) that were explicitly switched off. Protections that are on or left at the Windows default are never touched; forced image relocation is never changed. Restart required.",
        source: Source::ExploitMitigations,
        reboot: true,
        ask: false,
        keys: &[
            Key { allowed: MITIGATION_STATES, ..set("DEP", "", &[1, 2], false, Some(1), 2) },
            Key { allowed: MITIGATION_STATES, ..set("SEHOP", "", &[1, 2], false, Some(1), 2) },
            Key { allowed: MITIGATION_STATES, ..set("BottomUp", "", &[1, 2], false, Some(1), 2) },
            Key { allowed: MITIGATION_STATES, ..set("HighEntropy", "", &[1, 2], false, Some(1), 2) },
            Key { allowed: MITIGATION_STATES, ..set("CFG", "", &[1, 2], false, Some(1), 2) },
        ],
        gate: NO_GATE,
    },
    Spec {
        id: "ps.v2_engine",
        title: "Remove the old PowerShell 2.0 engine",
        description: "Turn off the Windows PowerShell 2.0 optional feature, which lacks modern malware scanning and logging. A feature that is missing already counts as removed. Removal can take a minute or more; undo turns the feature back on.",
        source: Source::PowerShellV2,
        reboot: false,
        ask: true,
        keys: &[set("Enabled", "", &[0], false, Some(0), 1)],
        gate: NO_GATE,
    },
    Spec {
        id: "printer.spooler_remote",
        title: "Printing service reachable from the network",
        description: "Stop the Print Spooler accepting connections from other computers (RegisterSpoolerRemoteRpcEndPoint=2) when no printer on this PC is shared. The Spooler restarts once; local printing is unaffected.",
        source: Source::Registry,
        reboot: false,
        ask: true,
        keys: &[Key {
            allowed: &[1, 2],
            ..set("RegisterSpoolerRemoteRpcEndPoint", PRINTERS_POLICY, &[2], false, Some(2), 2)
        }],
        gate: Gate {
            areas: &["Printers", "ADMX_Printing"],
            ..NO_GATE
        },
    },
    Spec {
        id: "services.legacy_remote",
        title: "Leftover remote-access services",
        description: "Stop and disable Remote Registry, WinRM, OpenSSH server, Telnet, FTP, IIS web and SNMP services that are running or start automatically. Services that are not installed are ignored; undo restores each start type and running state.",
        source: Source::LegacyServices,
        reboot: false,
        ask: true,
        keys: &[Key {
            name: "*",
            path: "",
            value: "",
            rule: Rule::Set {
                safe: &[3, 4],
                absent_safe: false,
                fix: Some(4),
            },
            max: 13,
            allowed: &[2, 3, 4, 5, 10, 11, 12, 13],
        }],
        gate: Gate {
            areas: &["RemoteManagement", "ADMX_WinRM"],
            ..NO_GATE
        },
    },
    Spec {
        id: "session.lock_on_wake",
        title: "Ask for your password when the PC wakes",
        description: "Require sign-in when the PC wakes from sleep, on the active power plan (plugged in and on battery). Not offered when the signed-in account has no password.",
        source: Source::LockOnWake,
        reboot: false,
        ask: true,
        keys: &[
            set("Ac", "", &[1], false, Some(1), 1),
            set("Dc", "", &[1], false, Some(1), 1),
        ],
        gate: Gate {
            areas: &["Power"],
            pattern: "^RequirePasswordWhenComputerWakes",
            policy_values: &[
                (POWER_CONSOLELOCK_POLICY, "ACSettingIndex"),
                (POWER_CONSOLELOCK_POLICY, "DCSettingIndex"),
            ],
            ..NO_GATE
        },
    },
    Spec {
        id: "update.store_autoupdate_policy",
        title: "Store apps blocked from updating",
        description: "Remove a locally set Microsoft Store policy that stops apps updating by themselves (AutoDownload=2). Managed devices are left alone; undo restores the value.",
        source: Source::Registry,
        reboot: false,
        ask: true,
        keys: &[set("AutoDownload", STORE_POLICY, &[0, 1, 3, 4], true, None, 4)],
        gate: Gate {
            areas: &["ApplicationManagement"],
            pattern: "^AllowAppStoreAutoUpdate",
            ..NO_GATE
        },
    },
    Spec {
        id: "update.paused",
        title: "Windows updates are paused",
        description: "Resume Windows Update by removing the pause markers (PauseUpdatesExpiryTime and the feature and quality pause start and end times) while a pause is still in force. Undo writes the saved times back, to the minute.",
        source: Source::UpdatePause,
        reboot: false,
        ask: true,
        keys: &[
            set("PauseUpdatesExpiryTime", WU_UX, &[0], false, Some(0), PAUSE_MAX),
            set("PauseFeatureUpdatesEndTime", WU_UX, &[0], false, Some(0), PAUSE_MAX),
            set("PauseQualityUpdatesEndTime", WU_UX, &[0], false, Some(0), PAUSE_MAX),
            set("PauseFeatureUpdatesStartTime", WU_UX, &[0], false, Some(0), PAUSE_MAX),
            set("PauseQualityUpdatesStartTime", WU_UX, &[0], false, Some(0), PAUSE_MAX),
        ],
        gate: Gate {
            areas: &["Update", "ADMX_WindowsUpdate"],
            policy_values: &[
                (WU, "WUServer"),
                (WU, "WUStatusServer"),
                (WU, "SetPolicyDrivenUpdateSourceForFeatureUpdates"),
                (WU, "SetPolicyDrivenUpdateSourceForQualityUpdates"),
                (WU_AU, "UseWUServer"),
            ],
            ..NO_GATE
        },
    },
    Spec {
        id: "smartscreen.apps",
        title: "Warn before running unknown downloads",
        description: "Set the SmartScreen app and file check to Warn when it is Off, and remove a locally set EnableSmartScreen=0 policy value. Managed devices and policy-controlled values are left alone; undo restores both.",
        source: Source::SmartScreen,
        reboot: false,
        ask: true,
        keys: &[
            Key {
                allowed: &[0, 1, 2, 3],
                ..set("SmartScreenEnabled", EXPLORER_MACHINE, &[1, 2, 3], true, Some(1), 3)
            },
            set("EnableSmartScreen", SYSPOL, &[1], true, None, 1),
        ],
        gate: Gate {
            areas: &["SmartScreen"],
            ..NO_GATE
        },
    },
    Spec {
        id: "vbs.memory_integrity",
        title: "Memory integrity",
        description: "Turn on Memory integrity (Core isolation in Windows Security) by setting Enabled=1, and WasEnabledBy=2 so Windows Security shows the switch as normal, under the HypervisorEnforcedCodeIntegrity scenario. Locked is never written, so there is no firmware lock. Offered only when the hardware supports it, nothing manages it, nothing is locked and every driver passes the static compatibility scan. Needs a restart; undo restores the exact earlier values.",
        source: Source::Registry,
        reboot: true,
        ask: true,
        keys: &[
            Key {
                allowed: &[0, 1],
                ..set("Enabled", HVCI_SCENARIO, &[1], false, Some(1), 1)
            },
            set("WasEnabledBy", HVCI_SCENARIO, &[2], false, Some(2), 255),
        ],
        gate: VBS_GATE,
    },
    Spec {
        id: "vbs.kernel_stack_protection",
        title: "Kernel-mode hardware-enforced stack protection",
        description: "Turn on Kernel-mode Hardware-enforced Stack Protection by setting Enabled=1 and WasEnabledBy=2 under the KernelShadowStacks scenario. Locked is never written. Offered only when Memory integrity is running and the processor supports shadow stacks. Needs a restart; undo restores the exact earlier values.",
        source: Source::Registry,
        reboot: true,
        ask: true,
        keys: &[
            Key {
                allowed: &[0, 1],
                ..set("Enabled", STACK_SCENARIO, &[1], false, Some(1), 1)
            },
            set("WasEnabledBy", STACK_SCENARIO, &[2], false, Some(2), 255),
        ],
        gate: VBS_GATE,
    },
    Spec {
        id: "privacy.recall",
        title: "Recall screenshots",
        description: "Stop Windows saving snapshots of your screen for Recall (DisableAIDataAnalysis=1) on PCs that have the Recall feature. Existing snapshots are removed by Windows. Not offered where Recall does not exist; undo removes the setting.",
        source: Source::Registry,
        reboot: false,
        ask: true,
        keys: &[set("DisableAIDataAnalysis", WINDOWS_AI_POLICY, &[1], false, Some(1), 1)],
        gate: Gate {
            areas: &["WindowsAI"],
            own_policy_key: WINDOWS_AI_POLICY,
            ..NO_GATE
        },
    },
    Spec {
        id: "privacy.diagnostic_data_level",
        title: "Optional diagnostic data",
        description: "Limit diagnostic data sent to Microsoft to the required level (AllowTelemetry=1). It is never set to 0 and the diagnostics service is never switched off. Undo restores the earlier value.",
        source: Source::Registry,
        reboot: false,
        ask: true,
        keys: &[Key {
            allowed: &[0, 1, 2, 3],
            ..set("AllowTelemetry", DATA_COLLECTION_POLICY, &[0, 1], false, Some(1), 3)
        }],
        gate: Gate {
            areas: &["System"],
            pattern: "^AllowTelemetry",
            ..NO_GATE
        },
    },
    Spec {
        id: "privacy.delivery_optimization",
        title: "Update sharing with other PCs",
        description: "Stop this PC sharing Windows and app downloads with other computers (DODownloadMode=0). Updates still download from Microsoft. Undo restores the earlier value.",
        source: Source::Registry,
        reboot: false,
        ask: true,
        keys: &[Key {
            allowed: &[0, 1, 2, 3, 99, 100],
            ..set("DODownloadMode", DELIVERY_POLICY, &[0, 99, 100], false, Some(0), 100)
        }],
        gate: Gate {
            areas: &["DeliveryOptimization"],
            pattern: "^DODownloadMode",
            ..NO_GATE
        },
    },
    Spec {
        id: "privacy.clipboard_sync",
        title: "Clipboard shared between your devices",
        description: "Stop the clipboard being synced to your other devices (AllowCrossDeviceClipboard=0). Not offered on Windows Home. Undo restores the earlier value.",
        source: Source::Registry,
        reboot: false,
        ask: true,
        keys: &[set("AllowCrossDeviceClipboard", SYSPOL, &[0], false, Some(0), 1)],
        gate: Gate {
            areas: &["Experience"],
            pattern: "^AllowCrossDeviceClipboard",
            ..NO_GATE
        },
    },
    Spec {
        id: "defender.exclusions_risky",
        title: "Risky antivirus exclusions",
        description: "Remove only the Defender exclusions that hide whole drives, Windows, user folders, program types such as exe or dll, or script engines. Every removed entry is recorded and undo adds it back. Other exclusions are never touched.",
        source: Source::DefenderExclusions,
        reboot: false,
        ask: true,
        keys: &[set("*", "", &[0], false, Some(0), 1)],
        gate: NO_GATE,
    },
];

pub fn all() -> &'static [Spec] {
    SPECS
}

pub fn spec(id: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.id == id)
}

pub fn is_hardening(id: &str) -> bool {
    spec(id).is_some()
}

/// True for choices the person makes (never pre-selected).
pub fn is_ask(id: &str) -> bool {
    spec(id).is_some_and(|s| s.ask)
}

pub fn is_safe(rule: Rule, v: Option<u32>) -> bool {
    match (rule, v) {
        (Rule::Set { absent_safe, .. }, None) => absent_safe,
        (Rule::Set { safe, .. }, Some(n)) => safe.contains(&n),
        (Rule::Exposure, Some(n)) => !(n & 8 != 0 && n & 4 != 0),
        (Rule::Exposure, None) => false,
    }
}

/// The only value an unsafe original may be moved to. Safe values are kept.
pub fn fix_of(rule: Rule, v: Option<u32>) -> Option<u32> {
    if is_safe(rule, v) {
        return v;
    }
    match (rule, v) {
        (Rule::Set { fix, .. }, _) => fix,
        (Rule::Exposure, Some(n)) => Some(if n & 3 == 0 { n & !8 } else { n & !4 }),
        (Rule::Exposure, None) => None,
    }
}

fn key_name_ok(source: Source, name: &str) -> bool {
    match source {
        Source::FirewallExposure => {
            (name.starts_with("FPS-") || name.starts_with("NETDIS-"))
                && name.len() <= 96
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        }
        Source::NetbiosAdapters => {
            // Adapter SettingID: a braced GUID.
            name.len() == 38
                && name.starts_with('{')
                && name.ends_with('}')
                && name[1..37]
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() || c == '-')
                && name[1..37].matches('-').count() == 4
        }
        Source::WifiProfiles => {
            !name.is_empty()
                && name.chars().count() <= 64
                && !name.chars().any(|c| c.is_control() || c == '"')
                && name.trim() == name
        }
        Source::LegacyServices => LEGACY_SERVICES.contains(&name),
        Source::DefenderExclusions => {
            let rest = ["path:", "ext:", "proc:"]
                .iter()
                .find_map(|p| name.strip_prefix(p));
            rest.is_some_and(|r| !r.is_empty())
                && name.chars().count() <= 300
                && !name.chars().any(|c| c.is_control() || c == '"')
                && name.trim() == name
        }
        _ => false,
    }
}

/// The only services the legacy-remote-access control may stop and disable.
pub const LEGACY_SERVICES: &[&str] = &[
    "RemoteRegistry",
    "WinRM",
    "sshd",
    "TlntSvr",
    "FTPSVC",
    "W3SVC",
    "SNMP",
];

impl Spec {
    pub fn dynamic(&self) -> bool {
        matches!(
            self.source,
            Source::FirewallExposure
                | Source::WifiProfiles
                | Source::NetbiosAdapters
                | Source::LegacyServices
                | Source::DefenderExclusions
        )
    }

    fn key(&self, name: &str) -> Option<&Key> {
        if self.dynamic() {
            key_name_ok(self.source, name).then(|| &self.keys[0])
        } else {
            self.keys.iter().find(|k| k.name == name)
        }
    }

    /// The target recorded in the catalog. Dynamic controls derive their target
    /// from each before-image, so the catalog holds a fixed sentinel.
    pub fn catalog_target(&self) -> Value {
        if self.dynamic() {
            return json!("derived-items-v1");
        }
        let mut items = Map::new();
        for k in self.keys {
            let fixed = match k.rule {
                Rule::Set { fix, .. } => fix,
                Rule::Exposure => None,
            };
            items.insert(k.name.into(), fixed.map_or(Value::Null, Value::from));
        }
        json!({ "items": items })
    }

    fn parse(&self, value: &Value) -> anyhow::Result<Vec<(String, Option<u32>)>> {
        use anyhow::{ensure, Context};
        let obj = value
            .as_object()
            .context("Invalid hardening state: expected an object")?;
        ensure!(
            obj.len() == 1,
            "Invalid hardening state: expected exactly one items field"
        );
        let items = obj
            .get("items")
            .and_then(Value::as_object)
            .context("Invalid hardening state: items must be an object")?;
        ensure!(items.len() <= 256, "Too many hardening items");
        let mut out = Vec::new();
        for (name, v) in items {
            let key = self
                .key(name)
                .with_context(|| format!("Unknown hardening item for {}", self.id))?;
            let parsed = match v {
                Value::Null => None,
                Value::Number(n) => {
                    let n = n.as_u64().and_then(|n| u32::try_from(n).ok());
                    Some(n.context("Invalid hardening DWORD")?)
                }
                _ => anyhow::bail!("Invalid hardening item value"),
            };
            if let Some(n) = parsed {
                ensure!(n <= key.max, "Hardening value out of range");
                ensure!(
                    key.allowed.is_empty() || key.allowed.contains(&n),
                    "Hardening value is not a legal setting"
                );
            } else {
                ensure!(
                    !matches!(key.rule, Rule::Exposure),
                    "Exposure state cannot be absent"
                );
            }
            out.push((name.clone(), parsed));
        }
        if !self.dynamic() {
            ensure!(
                out.len() == self.keys.len(),
                "Hardening state must contain every item"
            );
        }
        Ok(out)
    }

    pub fn validate(&self, value: &Value) -> anyhow::Result<()> {
        self.parse(value).map(|_| ())
    }

    /// Any key that is not safe, i.e. there is something to repair.
    pub fn any_unsafe(&self, value: &Value) -> bool {
        self.parse(value).is_ok_and(|items| {
            items
                .iter()
                .any(|(n, v)| self.key(n).is_some_and(|k| !is_safe(k.rule, *v)))
        })
    }

    /// The exact state a repair of `before` must produce.
    pub fn derive_target(&self, before: &Value) -> anyhow::Result<Value> {
        let mut items = Map::new();
        for (name, v) in self.parse(before)? {
            let key = self.key(&name).expect("parsed key exists");
            items.insert(name, fix_of(key.rule, v).map_or(Value::Null, Value::from));
        }
        Ok(json!({ "items": items }))
    }

    /// Restrict an observation to the keys of a journaled template. Fixed
    /// controls observe exactly their keys, so only dynamic ones are narrowed:
    /// a Wi-Fi network or firewall rule that appeared after the fix must not
    /// make undo of the recorded ones look like drift.
    pub fn view(&self, observed: &Value, template: &Value) -> Value {
        if !self.dynamic() {
            return observed.clone();
        }
        let (Some(o), Some(t)) = (
            observed.get("items").and_then(Value::as_object),
            template.get("items").and_then(Value::as_object),
        ) else {
            return observed.clone();
        };
        if self.source == Source::DefenderExclusions {
            // A removed exclusion is simply no longer listed: that is "0".
            // Nothing is filtered out (the engine passes either side as the
            // template), so a new risky entry makes undo stop as a conflict.
            let mut items = o.clone();
            for k in t.keys() {
                items.entry(k.clone()).or_insert_with(|| json!(0));
            }
            return json!({ "items": items });
        }
        let items: Map<String, Value> = o
            .iter()
            .filter(|(k, _)| t.contains_key(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        json!({ "items": items })
    }

    /// JSON handed to the PowerShell backend (compiled data, never user data).
    pub fn script_json(&self) -> String {
        let keys: Vec<Value> = self
            .keys
            .iter()
            .map(|k| {
                let (kind, safe, absent_safe, fix) = match k.rule {
                    Rule::Set {
                        safe,
                        absent_safe,
                        fix,
                    } => ("set", safe.to_vec(), absent_safe, fix),
                    Rule::Exposure => ("exposure", vec![], false, None),
                };
                json!({
                    "name": k.name, "path": k.path,
                    "valueName": if k.value.is_empty() { k.name } else { k.value },
                    "rule": kind, "safe": safe,
                    "absentSafe": absent_safe, "fix": fix, "max": k.max,
                })
            })
            .collect();
        let g = &self.gate;
        json!({
            "id": self.id,
            "source": format!("{:?}", self.source),
            "dynamic": self.dynamic(),
            "reboot": self.reboot,
            "keys": keys,
            "gate": {
                "areas": g.areas, "pattern": g.pattern, "tamperExempt": g.tamper_exempt,
                "secedit": g.secedit, "ownPolicyKey": g.own_policy_key,
                "policyValues": g.policy_values.iter()
                    .map(|(p, n)| json!({"path": p, "name": n})).collect::<Vec<_>>(),
            },
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(spec: &Spec, vals: &[Option<u32>]) -> Value {
        let mut m = Map::new();
        for (k, v) in spec.keys.iter().zip(vals) {
            m.insert(k.name.into(), v.map_or(Value::Null, Value::from));
        }
        json!({ "items": m })
    }

    /// Every legal value of a key (bounded: huge DWORD keys list `allowed`).
    fn candidate_values(k: &Key) -> Vec<u32> {
        if k.allowed.is_empty() {
            // Wide ranges (timestamps, minutes) are sampled at the low end.
            (0..=k.max.min(300)).collect()
        } else {
            k.allowed.to_vec()
        }
    }

    #[test]
    fn catalog_is_well_formed() {
        assert!(all().len() >= 20);
        let mut ids = std::collections::HashSet::new();
        for s in all() {
            assert!(ids.insert(s.id), "duplicate id {}", s.id);
            assert!(!s.keys.is_empty());
            for k in s.keys {
                if let Rule::Set {
                    safe,
                    absent_safe,
                    fix,
                } = k.rule
                {
                    // A repair must always converge on a safe state.
                    match fix {
                        Some(f) => {
                            assert!(safe.contains(&f), "{} fix {f} is not safe", s.id);
                            assert!(f <= k.max);
                        }
                        None => assert!(absent_safe, "{} removal needs absent_safe", s.id),
                    }
                    assert!(safe.iter().all(|n| *n <= k.max));
                    assert!(k.allowed.is_empty() || safe.iter().all(|n| k.allowed.contains(n)));
                }
                if s.source == Source::Registry {
                    assert!(k.path.starts_with("HKLM:\\"), "{}", s.id);
                    assert!(!k.path.contains('\''));
                }
            }
            assert_eq!(s.dynamic(), s.keys[0].name == "*");
            assert!(!s.script_json().contains("\\u0027"));
            assert!(!s.script_json().contains('\''));
            // Catalog target validates (fixed controls) and is itself safe.
            if !s.dynamic() {
                s.validate(&s.catalog_target()).unwrap();
                assert!(!s.any_unsafe(&s.catalog_target()), "{}", s.id);
            }
        }
        assert!(all().iter().filter(|s| !s.ask).count() >= 6);
        assert!(all().iter().filter(|s| s.ask).count() >= 10);
    }

    #[test]
    fn fixed_controls_repair_only_unsafe_keys_and_converge() {
        for s in all().iter().filter(|s| !s.dynamic()) {
            // Every key unsafe in turn, with the others at a safe value.
            for (i, k) in s.keys.iter().enumerate() {
                let Rule::Set {
                    safe,
                    absent_safe,
                    fix,
                    ..
                } = k.rule
                else {
                    continue;
                };
                let safe_vals: Vec<Option<u32>> = s
                    .keys
                    .iter()
                    .map(|k| match k.rule {
                        Rule::Set { safe, .. } => Some(safe[0]),
                        Rule::Exposure => Some(0),
                    })
                    .collect();
                let mut unsafe_candidates: Vec<Option<u32>> = candidate_values(k)
                    .into_iter()
                    .filter(|n| !safe.contains(n))
                    .map(Some)
                    .collect();
                if !absent_safe {
                    unsafe_candidates.push(None);
                }
                for bad in unsafe_candidates {
                    let mut vals = safe_vals.clone();
                    vals[i] = bad;
                    let before = items(s, &vals);
                    s.validate(&before).unwrap();
                    assert!(s.any_unsafe(&before), "{} {bad:?}", s.id);
                    let target = s.derive_target(&before).unwrap();
                    s.validate(&target).unwrap();
                    assert!(!s.any_unsafe(&target), "{} target unsafe", s.id);
                    assert_ne!(before, target);
                    // Other keys are preserved exactly.
                    for (j, v) in vals.iter().enumerate() {
                        if j != i {
                            assert_eq!(
                                target["items"][s.keys[j].name],
                                v.map_or(Value::Null, Value::from)
                            );
                        }
                    }
                    assert_eq!(
                        target["items"][k.name],
                        fix.map_or(Value::Null, Value::from)
                    );
                }
            }
            // All-safe states have nothing to repair and equal their own target.
            let all_safe: Vec<Option<u32>> = s
                .keys
                .iter()
                .map(|k| match k.rule {
                    Rule::Set { safe, .. } => Some(safe[0]),
                    Rule::Exposure => Some(0),
                })
                .collect();
            let st = items(s, &all_safe);
            assert!(!s.any_unsafe(&st));
            assert_eq!(s.derive_target(&st).unwrap(), st);
        }
    }

    #[test]
    fn safe_absent_defaults_count_as_protected() {
        let pnp = spec("printer.point_and_print").unwrap();
        let absent = items(pnp, &[None, None, None]);
        assert!(!pnp.any_unsafe(&absent));
        let bad = items(pnp, &[Some(0), Some(1), Some(2)]);
        assert!(pnp.any_unsafe(&bad));
        assert_eq!(
            pnp.derive_target(&bad).unwrap(),
            items(pnp, &[None, None, None])
        );
        // Partially unsafe: only the unsafe keys move.
        let part = items(pnp, &[Some(1), Some(1), Some(1)]);
        assert_eq!(
            pnp.derive_target(&part).unwrap(),
            items(pnp, &[Some(1), None, Some(1)])
        );
        // Absent is unsafe where Windows' default is unprotected.
        for id in [
            "net.llmnr",
            "lsa.run_as_ppl",
            "wsh.disabled",
            "defender.asr.standard",
        ] {
            let s = spec(id).unwrap();
            let vals = vec![None; s.keys.len()];
            assert!(s.any_unsafe(&items(s, &vals)), "{id}");
        }
        // Stronger settings are preserved.
        let ppl = spec("lsa.run_as_ppl").unwrap();
        assert!(!ppl.any_unsafe(&items(ppl, &[Some(1)])));
        let asr = spec("defender.asr.standard").unwrap();
        assert!(!asr.any_unsafe(&items(asr, &[Some(1), Some(6), Some(1)])));
        assert!(asr.any_unsafe(&items(asr, &[Some(1), Some(2), Some(1)])));
    }

    #[test]
    fn validation_rejects_malformed_states() {
        let ppl = spec("lsa.run_as_ppl").unwrap();
        for bad in [
            json!(null),
            json!({}),
            json!({"items": {}}),
            json!({"items": {"RunAsPPL": 3}}),
            json!({"items": {"RunAsPPL": -1}}),
            json!({"items": {"RunAsPPL": 1.5}}),
            json!({"items": {"RunAsPPL": "1"}}),
            json!({"items": {"RunAsPPL": true}}),
            json!({"items": {"Other": 1}}),
            json!({"items": {"RunAsPPL": 1, "Other": 1}}),
            json!({"items": {"RunAsPPL": 1}, "path": "x"}),
            json!({"present": true, "value": 1}),
        ] {
            assert!(ppl.validate(&bad).is_err(), "accepted {bad}");
        }
        for ok in [
            json!({"items": {"RunAsPPL": null}}),
            json!({"items": {"RunAsPPL": 0}}),
        ] {
            ppl.validate(&ok).unwrap();
        }
        let asr = spec("defender.asr.standard").unwrap();
        let g = "56a863a9-875e-4185-98a7-b882c64b5ce5";
        let bad_action = json!({"items": {g: 3,
            "9e6c4e1f-7d60-472f-ba1a-a39ef669e4b2": 1,
            "e6db77e5-3df2-4cf1-b95a-636979351e5b": 1}});
        assert!(asr.validate(&bad_action).is_err());
        let lock = spec("accounts.lockout_policy").unwrap();
        assert!(lock
            .validate(&json!({"items": {"LockoutThreshold": 1000}}))
            .is_err());
        lock.validate(&json!({"items": {"LockoutThreshold": 0}}))
            .unwrap();
    }

    #[test]
    fn dynamic_controls_validate_names_and_narrow_views() {
        let fw = spec("net.public_sharing_exposure").unwrap();
        fw.validate(&json!({"items": {}})).unwrap();
        fw.validate(&json!({"items": {"FPS-SMB-In-TCP": 15, "NETDIS-LLMNR-In-UDP": 6}}))
            .unwrap();
        for bad in [
            json!({"items": {"RemoteDesktop-UserMode-In-TCP": 15}}),
            json!({"items": {"FPS-x'; calc": 15}}),
            json!({"items": {"FPS-ok": 16}}),
            json!({"items": {"FPS-ok": null}}),
            json!({"items": {"fps-ok": 1}}),
        ] {
            assert!(fw.validate(&bad).is_err(), "accepted {bad}");
        }
        // Enabled on Public -> repaired by dropping Public; Public-only -> disabled.
        let before = json!({"items": {"FPS-A": 15, "FPS-B": 12, "FPS-C": 3, "FPS-D": 7}});
        assert!(fw.any_unsafe(&before));
        assert_eq!(
            fw.derive_target(&before).unwrap(),
            json!({"items": {"FPS-A": 11, "FPS-B": 4, "FPS-C": 3, "FPS-D": 7}})
        );
        // Undo of recorded rules ignores rules that appeared later.
        let now =
            json!({"items": {"FPS-A": 11, "FPS-B": 4, "FPS-C": 3, "FPS-D": 7, "FPS-NEW": 15}});
        assert_eq!(
            fw.view(&now, &before),
            json!({"items": {"FPS-A": 11, "FPS-B": 4, "FPS-C": 3, "FPS-D": 7}})
        );
        assert_eq!(fw.catalog_target(), json!("derived-items-v1"));

        let wifi = spec("wifi.risky_profiles").unwrap();
        wifi.validate(&json!({"items": {"Cafe Guest": 1, "John's WiFi": 0}}))
            .unwrap();
        for bad in [
            json!({"items": {"": 1}}),
            json!({"items": {"a\"b": 1}}),
            json!({"items": {"a\nb": 1}}),
            json!({"items": {"x": 2}}),
            json!({"items": {" padded": 1}}),
        ] {
            assert!(wifi.validate(&bad).is_err(), "accepted {bad}");
        }
        assert_eq!(
            wifi.derive_target(&json!({"items": {"Open": 1, "Done": 0}}))
                .unwrap(),
            json!({"items": {"Open": 0, "Done": 0}})
        );
        // Fixed controls are never narrowed.
        let ppl = spec("lsa.run_as_ppl").unwrap();
        let v = json!({"items": {"RunAsPPL": 2}});
        assert_eq!(ppl.view(&v, &json!({"items": {}})), v);
    }

    #[test]
    fn network_and_defender_extensions_follow_the_research_specs() {
        // Every new control is a choice, never pre-selected.
        for id in [
            "defender.asr.office",
            "defender.asr.ransomware_usb",
            "defender.network_protection",
            "defender.cloud_block_level",
            "net.stack_hardening",
            "net.netbios",
            "net.mdns",
            "net.wpad",
            "firewall.outbound_smb_internet",
            "tls.legacy_protocols",
        ] {
            assert!(spec(id).unwrap().ask, "{id} must be an ASK item");
        }
        // Defender extensions use the add-only ASR source and tamper-exempt strengthening.
        for id in ["defender.asr.office", "defender.asr.ransomware_usb"] {
            assert_eq!(spec(id).unwrap().source, Source::DefenderAsr);
            assert!(spec(id).unwrap().gate.tamper_exempt);
        }
        // Office rules must be Block; Warn is not enough for them.
        let office = spec("defender.asr.office").unwrap();
        let mut vals = vec![Some(1); 4];
        assert!(!office.any_unsafe(&items(office, &vals)));
        vals[2] = Some(6);
        assert!(office.any_unsafe(&items(office, &vals)));
        // Ransomware and USB rules are offered in Warn mode.
        let usb = spec("defender.asr.ransomware_usb").unwrap();
        let before = items(usb, &[Some(0), None]);
        assert_eq!(
            usb.derive_target(&before).unwrap(),
            items(usb, &[Some(6), Some(6)])
        );
        assert!(!usb.any_unsafe(&items(usb, &[Some(1), Some(6)])));
        // Network protection: Enabled only; audit mode is not protection.
        let np = spec("defender.network_protection").unwrap();
        assert!(np.any_unsafe(&items(np, &[Some(2)])));
        assert_eq!(
            np.derive_target(&items(np, &[Some(0)])).unwrap(),
            items(np, &[Some(1)])
        );
        // Cloud block level: High and 20 seconds, never zero tolerance; stronger is kept.
        let cbl = spec("defender.cloud_block_level").unwrap();
        assert_eq!(
            cbl.derive_target(&items(cbl, &[Some(0), Some(0)])).unwrap(),
            items(cbl, &[Some(2), Some(20)])
        );
        assert!(!cbl.any_unsafe(&items(cbl, &[Some(6), Some(35)])));
        assert!(!cbl.any_unsafe(&items(cbl, &[Some(4), Some(20)])));
        assert!(cbl.any_unsafe(&items(cbl, &[Some(2), Some(10)])));
        for k in cbl.keys {
            if let Rule::Set { fix, .. } = k.rule {
                assert_ne!(fix, Some(6));
            }
        }
        // Stack hardening: both IP versions, one registry value name.
        let stack = spec("net.stack_hardening").unwrap();
        let ver: Vec<Value> = serde_json::from_str::<Value>(&stack.script_json()).unwrap()["keys"]
            .as_array()
            .unwrap()
            .clone();
        let by_name = |n: &str| ver.iter().find(|k| k["name"] == n).unwrap().clone();
        assert_eq!(
            by_name("DisableIPSourceRouting")["valueName"],
            "DisableIPSourceRouting"
        );
        assert_eq!(
            by_name("DisableIPSourceRouting6")["valueName"],
            "DisableIPSourceRouting"
        );
        assert!(by_name("DisableIPSourceRouting6")["path"]
            .as_str()
            .unwrap()
            .contains("Tcpip6"));
        assert_eq!(by_name("EnableICMPRedirect")["fix"], 0);
        assert_eq!(by_name("NoNameReleaseOnDemand")["fix"], 1);
        assert!(stack.reboot);
        // TLS: the six protocol sides, Enabled 0 and DisabledByDefault 1; 0xFFFFFFFF is a legal original.
        let tls = spec("tls.legacy_protocols").unwrap();
        assert_eq!(tls.keys.len(), 12);
        let names: std::collections::HashSet<_> = tls.keys.iter().map(|k| k.name).collect();
        assert_eq!(names.len(), 12);
        for proto in ["SSL 3.0", "TLS 1.0", "TLS 1.1"] {
            for side in ["Client", "Server"] {
                let path_end = format!("{proto}\\{side}");
                assert_eq!(
                    tls.keys
                        .iter()
                        .filter(|k| k.path.ends_with(&path_end))
                        .count(),
                    2,
                    "{path_end}"
                );
            }
        }
        let mut m = Map::new();
        for k in tls.keys {
            m.insert(
                k.name.into(),
                if k.value == "Enabled" {
                    json!(u32::MAX)
                } else {
                    Value::Null
                },
            );
        }
        let before = json!({ "items": m });
        tls.validate(&before).unwrap();
        let target = tls.derive_target(&before).unwrap();
        for k in tls.keys {
            let want = if k.value == "Enabled" { 0 } else { 1 };
            assert_eq!(target["items"][k.name], want);
        }
        assert!(tls
            .validate(&json!({"items": {"ssl3.client.enabled": 2}}))
            .is_err());
        // NetBIOS: per adapter, id must be a braced GUID, Disabled (2) is the fix.
        let nb = spec("net.netbios").unwrap();
        assert!(nb.dynamic());
        let a = "{11111111-1111-1111-1111-111111111111}";
        let b = "{abcdefAB-2222-2222-2222-222222222222}";
        nb.validate(&json!({"items": {a: 0, b: 1}})).unwrap();
        for bad in [
            json!({"items": {"Ethernet": 1}}),
            json!({"items": {"{1111}": 1}}),
            json!({"items": {"{1111111g-1111-1111-1111-111111111111}": 1}}),
            json!({"items": {"{11111111-1111-1111-1111-111111111111}'; x": 1}}),
            json!({"items": {a: 3}}),
        ] {
            assert!(nb.validate(&bad).is_err(), "accepted {bad}");
        }
        assert_eq!(
            nb.derive_target(&json!({"items": {a: 0, b: 2}})).unwrap(),
            json!({"items": {a: 2, b: 2}})
        );
        // Outbound SMB rule: present is safe; its absence is the repairable state.
        let fw = spec("firewall.outbound_smb_internet").unwrap();
        assert!(fw.any_unsafe(&json!({"items": {"RulePresent": 0}})));
        assert!(!fw.any_unsafe(&json!({"items": {"RulePresent": 1}})));
        // The research [K] values: ASK, readable in the descriptions.
        assert!(spec("net.mdns").unwrap().keys[0].name == "EnableMDNS");
        assert!(spec("net.wpad").unwrap().keys[0].name == "DisableWpad");
    }

    #[test]
    fn core_protections_write_only_the_documented_values_and_never_a_lock() {
        for (id, scenario) in [
            ("vbs.memory_integrity", "HypervisorEnforcedCodeIntegrity"),
            ("vbs.kernel_stack_protection", "KernelShadowStacks"),
        ] {
            let s = spec(id).unwrap();
            assert!(s.reboot && s.ask && !s.dynamic(), "{id}");
            let names: Vec<&str> = s.keys.iter().map(|k| k.name).collect();
            assert_eq!(names, ["Enabled", "WasEnabledBy"], "{id}");
            for k in s.keys {
                assert!(k.path.ends_with(&format!("Scenarios\\{scenario}")), "{id}");
                assert!(!k.name.contains("Lock") && !k.value.contains("Lock"));
            }
            // Absent is unsafe and becomes Enabled=1, WasEnabledBy=2.
            let absent = items(s, &[None, None]);
            assert!(s.any_unsafe(&absent));
            assert_eq!(
                s.derive_target(&absent).unwrap(),
                items(s, &[Some(1), Some(2)])
            );
            // Explicitly off keeps whatever marker Windows left.
            let off = items(s, &[Some(0), Some(2)]);
            assert_eq!(
                s.derive_target(&off).unwrap(),
                items(s, &[Some(1), Some(2)])
            );
            let marker = items(s, &[Some(0), Some(1)]);
            assert_eq!(
                s.derive_target(&marker).unwrap(),
                items(s, &[Some(1), Some(2)])
            );
            // Already on: nothing to fix and nothing rewritten.
            let on = items(s, &[Some(1), Some(2)]);
            assert!(!s.any_unsafe(&on));
            assert_eq!(s.derive_target(&on).unwrap(), on);
            // Enabled can only ever be 0 or 1.
            assert!(s
                .validate(&json!({"items": {"Enabled": 2, "WasEnabledBy": 2}}))
                .is_err());
            assert!(s.validate(&json!({"items": {"Enabled": 1}})).is_err());
            // Anyone else's policy for this area means assessment only.
            let gate = serde_json::from_str::<Value>(&s.script_json()).unwrap()["gate"].clone();
            assert!(gate["ownPolicyKey"]
                .as_str()
                .unwrap()
                .ends_with("Windows\\DeviceGuard"));
            assert!(gate["areas"].as_array().unwrap().len() >= 2);
        }
    }

    /// When SECBLITZ_PARITY_OUT names a file, write every spec with the Rust
    /// verdict (safe / fix) for each candidate value. The PowerShell fixture
    /// replays it so both implementations of the rules provably agree.
    #[test]
    fn export_rule_parity_fixture_for_powershell() {
        let Ok(path) = std::env::var("SECBLITZ_PARITY_OUT") else {
            return;
        };
        let mut out = Vec::new();
        for s in all() {
            let mut cases = Vec::new();
            for k in s.keys {
                let mut values: Vec<Option<u32>> = vec![None];
                values.extend((0..=k.max.min(16)).map(Some));
                values.push(Some(k.max));
                values.extend(k.allowed.iter().map(|n| Some(*n)));
                values.extend(match k.rule {
                    Rule::Set { safe, .. } => safe.iter().map(|n| Some(*n)).collect::<Vec<_>>(),
                    Rule::Exposure => vec![],
                });
                values.sort_unstable();
                values.dedup();
                for v in values {
                    cases.push(json!({
                        "key": k.name, "value": v,
                        "safe": is_safe(k.rule, v), "fix": fix_of(k.rule, v),
                    }));
                }
            }
            out.push(json!({
                "id": s.id,
                "spec": serde_json::from_str::<Value>(&s.script_json()).unwrap(),
                "cases": cases,
            }));
        }
        std::fs::write(path, serde_json::to_string(&out).unwrap()).unwrap();
    }

    #[test]
    fn system_dynamic_controls_accept_only_their_own_names() {
        let svc = spec("services.legacy_remote").unwrap();
        svc.validate(&json!({"items": {"RemoteRegistry": 10, "sshd": 4, "WinRM": 13}}))
            .unwrap();
        for bad in [
            json!({"items": {"Spooler": 4}}),
            json!({"items": {"winrm": 4}}),
            json!({"items": {"WinRM": 1}}),
            json!({"items": {"WinRM": 14}}),
            json!({"items": {"WinRM'; calc": 4}}),
        ] {
            assert!(svc.validate(&bad).is_err(), "accepted {bad}");
        }
        // Running or automatic is unsafe; the fix is "disabled and stopped".
        let before = json!({"items": {"WinRM": 10, "sshd": 3, "SNMP": 12, "FTPSVC": 5}});
        assert_eq!(
            svc.derive_target(&before).unwrap(),
            json!({"items": {"WinRM": 4, "sshd": 3, "SNMP": 4, "FTPSVC": 4}})
        );

        let ex = spec("defender.exclusions_risky").unwrap();
        ex.validate(&json!({"items": {
            "path:C:\\Users\\Bob\\Downloads": 1, "ext:exe": 0, "proc:powershell.exe": 1,
            "path:D:\\Gäme's": 1,
        }}))
        .unwrap();
        for bad in [
            json!({"items": {"": 1}}),
            json!({"items": {"path:": 1}}),
            json!({"items": {"file:C:\\x": 1}}),
            json!({"items": {"PATH:C:\\x": 1}}),
            json!({"items": {"path:C:\\\"x": 1}}),
            json!({"items": {"ext:exe\n": 1}}),
            json!({"items": {" path:C:\\x": 1}}),
            json!({"items": {"ext:exe": 2}}),
        ] {
            assert!(ex.validate(&bad).is_err(), "accepted {bad}");
        }
        // A removed exclusion disappears from the listing; the view reads it as 0.
        let template = json!({"items": {"ext:exe": 1, "path:C:\\": 1}});
        let now = json!({"items": {"ext:dll": 1}});
        assert_eq!(
            ex.view(&now, &template),
            json!({"items": {"ext:dll": 1, "ext:exe": 0, "path:C:\\": 0}})
        );
        // Other dynamic controls keep ignoring recorded names that vanished.
        let fw = spec("net.public_sharing_exposure").unwrap();
        assert_eq!(
            fw.view(&json!({"items": {}}), &json!({"items": {"FPS-A": 15}})),
            json!({"items": {}})
        );
    }

    #[test]
    fn system_controls_follow_the_research_exclusions() {
        // Never ForceRelocateImages, never telemetry 0, never RunAsPPL 1-style locks.
        let mit = spec("system.exploit_mitigations").unwrap();
        let names: Vec<_> = mit.keys.iter().map(|k| k.name).collect();
        assert_eq!(names, ["DEP", "SEHOP", "BottomUp", "HighEntropy", "CFG"]);
        let diag = spec("privacy.diagnostic_data_level").unwrap();
        let Rule::Set { fix, .. } = diag.keys[0].rule else {
            unreachable!()
        };
        assert_eq!(fix, Some(1));
        // UseMachineId is deliberately not part of ntlm.extras.
        assert!(spec("ntlm.extras")
            .unwrap()
            .keys
            .iter()
            .all(|k| k.name != "UseMachineId"));
        // Every choice with an uncertain value is an ASK, only mitigations drift-repair automatically.
        for id in [
            "ntlm.extras",
            "driver.vulnerable_blocklist",
            "ps.v2_engine",
            "printer.spooler_remote",
            "services.legacy_remote",
            "session.lock_on_wake",
            "update.store_autoupdate_policy",
            "update.paused",
            "smartscreen.apps",
            "privacy.recall",
            "privacy.diagnostic_data_level",
            "privacy.delivery_optimization",
            "privacy.clipboard_sync",
            "defender.exclusions_risky",
        ] {
            assert!(is_ask(id), "{id} must be a choice");
        }
        assert!(!is_ask("system.exploit_mitigations"));
        // Pause markers cannot overflow a PowerShell int.
        assert!(spec("update.paused")
            .unwrap()
            .keys
            .iter()
            .all(|k| k.max <= i32::MAX as u32));
    }

    #[test]
    fn script_json_round_trips_and_is_single_quote_free() {
        for s in all() {
            let v: Value = serde_json::from_str(&s.script_json()).unwrap();
            assert_eq!(v["id"], s.id);
            assert_eq!(v["keys"].as_array().unwrap().len(), s.keys.len());
        }
    }
}
