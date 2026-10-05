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

const ASR_ACTIONS: &[u32] = &[0, 1, 2, 6];
const fn asr(guid: &'static str, fix: u32) -> Key {
    Key {
        name: guid,
        path: "",
        rule: Rule::Set {
            safe: &[1, 6],
            absent_safe: false,
            fix: Some(fix),
        },
        max: 6,
        allowed: ASR_ACTIONS,
    }
}

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
        Source::WifiProfiles => {
            !name.is_empty()
                && name.chars().count() <= 64
                && !name.chars().any(|c| c.is_control() || c == '"')
                && name.trim() == name
        }
        _ => false,
    }
}

impl Spec {
    pub fn dynamic(&self) -> bool {
        matches!(
            self.source,
            Source::FirewallExposure | Source::WifiProfiles
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
            items.iter().any(|(n, v)| {
                self.key(n)
                    .is_some_and(|k| !is_safe(k.rule, *v))
            })
        })
    }

    /// The exact state a repair of `before` must produce.
    pub fn derive_target(&self, before: &Value) -> anyhow::Result<Value> {
        let mut items = Map::new();
        for (name, v) in self.parse(before)? {
            let key = self.key(&name).expect("parsed key exists");
            items.insert(
                name,
                fix_of(key.rule, v).map_or(Value::Null, Value::from),
            );
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
                    "name": k.name, "path": k.path, "rule": kind, "safe": safe,
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
                let Rule::Set { safe, absent_safe, fix, .. } = k.rule else { continue };
                let safe_vals: Vec<Option<u32>> = s
                    .keys
                    .iter()
                    .map(|k| match k.rule {
                        Rule::Set { safe, .. } => Some(safe[0]),
                        Rule::Exposure => Some(0),
                    })
                    .collect();
                let mut unsafe_candidates: Vec<Option<u32>> = (0..=k.max)
                    .filter(|n| (k.allowed.is_empty() || k.allowed.contains(n)) && !safe.contains(n))
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
        for id in ["net.llmnr", "lsa.run_as_ppl", "wsh.disabled", "defender.asr.standard"] {
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
        for ok in [json!({"items": {"RunAsPPL": null}}), json!({"items": {"RunAsPPL": 0}})] {
            ppl.validate(&ok).unwrap();
        }
        let asr = spec("defender.asr.standard").unwrap();
        let g = "56a863a9-875e-4185-98a7-b882c64b5ce5";
        let bad_action = json!({"items": {g: 3,
            "9e6c4e1f-7d60-472f-ba1a-a39ef669e4b2": 1,
            "e6db77e5-3df2-4cf1-b95a-636979351e5b": 1}});
        assert!(asr.validate(&bad_action).is_err());
        let lock = spec("accounts.lockout_policy").unwrap();
        assert!(lock.validate(&json!({"items": {"LockoutThreshold": 1000}})).is_err());
        lock.validate(&json!({"items": {"LockoutThreshold": 0}})).unwrap();
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
        let now = json!({"items": {"FPS-A": 11, "FPS-B": 4, "FPS-C": 3, "FPS-D": 7, "FPS-NEW": 15}});
        assert_eq!(
            fw.view(&now, &before),
            json!({"items": {"FPS-A": 11, "FPS-B": 4, "FPS-C": 3, "FPS-D": 7}})
        );
        assert_eq!(fw.catalog_target(), json!("derived-items-v1"));

        let wifi = spec("wifi.risky_profiles").unwrap();
        wifi.validate(&json!({"items": {"Cafe Guest": 1, "John's WiFi": 0}})).unwrap();
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
            wifi.derive_target(&json!({"items": {"Open": 1, "Done": 0}})).unwrap(),
            json!({"items": {"Open": 0, "Done": 0}})
        );
        // Fixed controls are never narrowed.
        let ppl = spec("lsa.run_as_ppl").unwrap();
        let v = json!({"items": {"RunAsPPL": 2}});
        assert_eq!(ppl.view(&v, &json!({"items": {}})), v);
    }

    /// When SECBLITZ_PARITY_OUT names a file, write every spec with the Rust
    /// verdict (safe / fix) for each candidate value. The PowerShell fixture
    /// replays it so both implementations of the rules provably agree.
    #[test]
    fn export_rule_parity_fixture_for_powershell() {
        let Ok(path) = std::env::var("SECBLITZ_PARITY_OUT") else { return };
        let mut out = Vec::new();
        for s in all() {
            let mut cases = Vec::new();
            for k in s.keys {
                let mut values: Vec<Option<u32>> = vec![None];
                values.extend((0..=k.max.min(16)).map(Some));
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
    fn script_json_round_trips_and_is_single_quote_free() {
        for s in all() {
            let v: Value = serde_json::from_str(&s.script_json()).unwrap();
            assert_eq!(v["id"], s.id);
            assert_eq!(v["keys"].as_array().unwrap().len(), s.keys.len());
        }
    }
}
