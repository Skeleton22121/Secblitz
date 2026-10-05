//! Windows platform boundary. No caller-provided PowerShell or registry paths are accepted.
use crate::model::{Backend, Control};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::path::PathBuf;

#[cfg(windows)]
#[path = "platform/windows.rs"]
mod windows;

/// Explicitly selected support operation, never a reversible hardening control.
/// No scripts, paths, sources, scan arguments, or arbitrary IDs are accepted.
pub fn support_action(id: &str) -> Result<()> {
    validate_support_id(id)?;
    #[cfg(windows)]
    {
        windows::support_action(id)
    }
    #[cfg(not(windows))]
    {
        bail!("Defender support actions require Windows")
    }
}

fn validate_support_id(id: &str) -> Result<()> {
    if !matches!(id, "defender_update" | "defender_quickscan") {
        bail!("Unknown support action id");
    }
    Ok(())
}

// Reuse the compiled backend's module bootstrap and policy helper definitions,
// but never its control dispatcher. Exact delimiter changes fail closed.
#[cfg(any(windows, test))]
fn support_script(id: &str) -> Result<String> {
    validate_support_id(id)?;
    let source = include_str!("platform/backend.ps1");
    let delimiter = "\ntry {\n    switch -CaseSensitive ($action) {";
    let (definitions, _) = source
        .split_once(delimiter)
        .ok_or_else(|| anyhow::anyhow!("Embedded backend dispatcher boundary changed"))?;
    if source.matches(delimiter).count() != 1 {
        bail!("Ambiguous embedded backend dispatcher boundary");
    }
    Ok(format!(
        "$inputJson=$null\n{definitions}\n$supportId='{id}'\n{}",
        include_str!("actions/defender.ps1")
    ))
}

/// Recheck management/policy authority for one of the two fixed service-DACL
/// controls. The native wrapper must call this for observation eligibility and
/// immediately before every write (including restore); success is not cached.
/// Owner/ACL validation, elevation and mutation verification belong to that wrapper.
pub fn permission_gate(id: &str) -> Result<()> {
    validate_permission_id(id)?;
    #[cfg(windows)]
    {
        windows::permission_gate(id)
    }
    #[cfg(not(windows))]
    {
        bail!("Service permission eligibility requires Windows")
    }
}

fn validate_permission_id(id: &str) -> Result<()> {
    if !matches!(
        id,
        "permissions.service.bits" | "permissions.service.wuauserv"
    ) {
        bail!("Unknown service permission control id");
    }
    Ok(())
}

#[cfg_attr(not(windows), allow(dead_code))]
fn validate_request(action: &str, id: Option<&str>, value: Option<&Value>) -> Result<()> {
    match action {
        "permission_gate" if value.is_none() => {
            validate_permission_id(id.ok_or_else(|| anyhow::anyhow!("Missing control id"))?)
        }
        "machine" | "findings" if id.is_none() && value.is_none() => Ok(()),
        "observe" | "write" => {
            let id = id.ok_or_else(|| anyhow::anyhow!("Missing control id"))?;
            if !controls().iter().any(|c| c.id == id) {
                bail!("Unknown control id");
            }
            match (action, value) {
                ("observe", None) => Ok(()),
                ("write", Some(value)) => validate_value(id, value),
                _ => bail!("Invalid platform action arguments"),
            }
        }
        _ => bail!("Invalid platform action arguments"),
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn validate_permission_reply(reply: &Value) -> Result<()> {
    if reply != &json!({"ok":true}) {
        bail!("Service permission gate was not acknowledged");
    }
    Ok(())
}

pub fn backend() -> Result<Box<dyn Backend>> {
    #[cfg(windows)]
    {
        windows::backend()
    }
    #[cfg(not(windows))]
    {
        bail!("Secblitz requires Windows 10/11 x64; this platform cannot assess or change Windows")
    }
}

pub fn is_elevated() -> Result<bool> {
    #[cfg(windows)]
    {
        windows::is_elevated()
    }
    #[cfg(not(windows))]
    {
        bail!("Elevation is only supported on Windows")
    }
}

pub fn elevate(args: &[String]) -> Result<()> {
    #[cfg(windows)]
    {
        windows::elevate(args)
    }
    #[cfg(not(windows))]
    {
        let _ = args;
        bail!("Elevation is only supported on Windows")
    }
}

/// GUI-owned data (score log, app prefs, app clean-up journal, dev status file)
/// lives in the `App` namespace inside the protected state directory. The engine
/// treats it as opaque, like `operations` and `Patching`; journal entries must
/// never be written next to the WAL files themselves.
pub fn app_dir() -> Result<PathBuf> {
    let dir = state_dir()?.join("App");
    match std::fs::symlink_metadata(&dir) {
        Ok(m) => anyhow::ensure!(
            m.is_dir() && !m.file_type().is_symlink(),
            "The app data folder is not a plain directory"
        ),
        // Created inside the protected state directory, so it inherits its ACL.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir(&dir)?,
        Err(e) => return Err(e.into()),
    }
    Ok(dir)
}

pub fn state_dir() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        windows::state_dir()
    }
    #[cfg(not(windows))]
    {
        bail!("The protected journal directory is only available on Windows")
    }
}

// Targets and restoration values describe raw preferences. Independently verified
// firewall effective status lives only in Observation metadata, never before-images.
// Registry restoration includes absence rather than inventing a previous value.
#[cfg_attr(not(windows), allow(dead_code))]
fn controls() -> Vec<Control> {
    let mut out = Vec::new();
    for (id, title) in [
        ("defender.realtime", "Defender real-time protection"),
        ("defender.behavior", "Defender behavior monitoring"),
        ("defender.ioav", "Defender downloaded-file scanning"),
        ("defender.archive", "Defender archive scanning"),
    ] {
        out.push(Control { id: id.into(), title: title.into(), description: "Enable the core Defender preference only on an unmanaged device with no competing antivirus. Preserve exclusions and other preferences.".into(), target: json!(false), reboot: false });
    }
    for profile in ["domain", "private", "public"] {
        for (suffix, title, target) in [
            ("enabled", "Enable firewall", json!(true)),
            (
                "inbound",
                "Block unsolicited inbound traffic",
                json!("Block"),
            ),
        ] {
            out.push(Control {
                id: format!("firewall.{profile}.{suffix}"),
                title: format!("{title} ({profile})"),
                description:
                    "Preserve all firewall rules and outbound policy. Unmanaged devices only."
                        .into(),
                target,
                reboot: false,
            });
        }
    }
    out.push(Control { id: "uac.enabled".into(), title: "Enable UAC".into(), description: "Repair an explicitly disabled EnableLUA value; preserve absent or already enabled settings.".into(), target: json!({"present":true,"value":1}), reboot: true });
    out.push(Control {
        id: "uac.consent".into(),
        title: "Require administrator consent".into(),
        description: "Repair consent mode 0 to Windows default 5. Preserve every nonzero mode."
            .into(),
        target: json!({"present":true,"value":5}),
        reboot: false,
    });
    for (id, title, description, target, reboot) in [
        ("installer.always_install_elevated", "Disable always-elevated MSI installation", "Repair only machine AlwaysInstallElevated=1. The machine setting breaks the vulnerable machine/user conjunction; preserve HKCU, absent values and normal administrator-authorized installs.", 0, false),
        ("lsa.restrict_anonymous_sam", "Restrict anonymous SAM enumeration", "Repair only RestrictAnonymousSAM=0. Require authentication for account enumeration; legacy anonymous enumeration workflows may be affected. Preserve absent values and other LSA settings.", 1, false),
        ("lsa.limit_blank_password_use", "Limit blank-password accounts to console logon", "Repair only LimitBlankPasswordUse=0. Block remote logons using blank local passwords while preserving physical console logon. Preserve absent values; no passwords are inspected or changed.", 1, false),
        ("wdigest.use_logon_credential", "Disable WDigest plaintext credential caching", "Repair only UseLogonCredential=1. Preserve absent values (safe on supported Windows). Readback verifies stored configuration, not running LSASS; restart/sign-out may be needed for existing sessions. Legacy Digest SSO may require credentials.", 0, true),
    ] {
        out.push(Control {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            target: json!({"present":true,"value":target}),
            reboot,
        });
    }
    out
}

#[cfg_attr(not(windows), allow(dead_code))]
fn validate_value(id: &str, value: &Value) -> Result<()> {
    let control = controls()
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| anyhow::anyhow!("Unknown control id"))?;
    if id.starts_with("defender.") || id.ends_with(".enabled") && id.starts_with("firewall.") {
        if !value.is_boolean() {
            bail!("{} requires a JSON boolean", control.id);
        }
    } else if id.starts_with("firewall.") {
        if !matches!(value.as_str(), Some("Block" | "Allow" | "NotConfigured")) {
            bail!("Invalid inbound action");
        }
    } else {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RegistryValue {
            present: bool,
            value: Option<u32>,
        }
        let v: RegistryValue = serde_json::from_value(value.clone())?;
        // Require both keys, including an explicit null for an absent value.
        if value.as_object().map(|o| o.len()) != Some(2) || value.get("value").is_none() {
            bail!("Registry state must contain exactly present and value");
        }
        if v.present {
            let max = if id == "uac.consent" { 5 } else { 1 };
            if !matches!(v.value, Some(n) if n <= max) {
                bail!("Invalid registry DWORD");
            }
        } else if v.value.is_some() {
            bail!("Absent registry value must be null");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn firewall_observation_wire_contract_preserves_raw_values() {
        use crate::model::{
            validate_observation, Authority, EffectiveFirewall, InboundAction, Observation,
        };
        for profile in ["domain", "private", "public"] {
            for (raw, effective) in [("NotConfigured", "block"), ("Allow", "allow")] {
                let id = format!("firewall.{profile}.inbound");
                let observation: Observation = serde_json::from_value(json!({
                    "value": raw, "eligible": true, "reason": "Eligible unmanaged local preference",
                    "authority": "local", "effective": {"kind": "inbound", "value": effective}
                }))
                .unwrap();
                validate_value(&id, &observation.value).unwrap();
                validate_observation(&id, &observation).unwrap();
                assert_eq!(observation.value, json!(raw));
                assert_eq!(observation.authority, Some(Authority::Local));
                assert_eq!(
                    observation.effective,
                    Some(EffectiveFirewall::Inbound(if effective == "block" {
                        InboundAction::Block
                    } else {
                        InboundAction::Allow
                    }))
                );
                assert!(
                    validate_observation(&format!("firewall.{profile}.enabled"), &observation)
                        .is_err()
                );
                assert!(validate_observation("uac.enabled", &observation).is_err());
            }
        }
        for authority in ["managed", "unknown"] {
            let mut observation: Observation = serde_json::from_value(json!({
                "value": "Allow", "eligible": false, "reason": "assessment only",
                "authority": authority, "effective": {"kind": "inbound", "value": "block"}
            }))
            .unwrap();
            validate_observation("firewall.public.inbound", &observation).unwrap();
            observation.eligible = true;
            assert!(validate_observation("firewall.public.inbound", &observation).is_err());
        }
        for evidence in [
            json!({"kind":"enabled","value":"true"}),
            json!({"kind":"enabled","value":1}),
            json!({"kind":"inbound","value":"NotConfigured"}),
            json!({"kind":"inbound","value":"Block"}),
            json!({"kind":"unknown","value":false}),
        ] {
            assert!(serde_json::from_value::<Observation>(json!({
                "value": "NotConfigured", "eligible": false, "reason": "fixture",
                "authority": "unknown", "effective": evidence
            }))
            .is_err());
        }
        let legacy: Observation = serde_json::from_value(json!({
            "value": {"present":false,"value":null}, "eligible":false,
            "reason":"Preserving absent or nonzero UAC preference"
        }))
        .unwrap();
        assert!(legacy.effective.is_none() && legacy.authority.is_none());
        validate_observation("uac.enabled", &legacy).unwrap();
    }
    #[test]
    fn support_operations_are_isolated_and_exact() {
        for id in ["defender_update", "defender_quickscan"] {
            let script = support_script(id).unwrap();
            assert!(!script.contains("switch -CaseSensitive ($action)"));
            assert!(script.contains("function CheckScopedPolicy"));
            assert!(script.contains("function MdmRegistered"));
            assert!(validate_request(id, None, None).is_err());
            assert!(validate_request("support", Some(id), None).is_err());
            assert!(validate_request("write", Some(id), Some(&json!(false))).is_err());
        }
        for id in [
            "",
            "defender.realtime",
            "Defender_update",
            "defender_update ",
            "defender_quickscan\0",
            "defender_update'; exit",
        ] {
            assert!(support_script(id).is_err());
            assert!(support_action(id).is_err());
        }
    }
    #[test]
    fn permission_gate_is_an_exact_isolated_action() {
        assert_eq!(controls().len(), 16);
        for id in ["permissions.service.bits", "permissions.service.wuauserv"] {
            validate_request("permission_gate", Some(id), None).unwrap();
            assert!(validate_request("permission_gate", Some(id), Some(&Value::Null)).is_err());
            for action in ["observe", "write", "machine", "findings"] {
                assert!(validate_request(action, Some(id), None).is_err());
                assert!(validate_request(action, Some(id), Some(&json!({"ok":true}))).is_err());
            }
            assert!(validate_value(id, &json!({"present":true,"value":1})).is_err());
        }
        for id in [
            "",
            "bits",
            "permissions.service.BITS",
            "permissions.service.spooler",
            "permissions.service.bits ",
            "permissions.service.bits' ; exit",
            "permissions.service.bits\0",
        ] {
            assert!(validate_request("permission_gate", Some(id), None).is_err());
            assert!(permission_gate(id).is_err());
        }
        for control in controls() {
            assert!(validate_request("permission_gate", Some(&control.id), None).is_err());
            validate_request("observe", Some(&control.id), None).unwrap();
            validate_request("write", Some(&control.id), Some(&control.target)).unwrap();
        }
        assert!(validate_request("permission_gate", None, None).is_err());
        assert!(
            validate_request("Permission_Gate", Some("permissions.service.bits"), None).is_err()
        );
        validate_permission_reply(&json!({"ok":true})).unwrap();
        for bad in [
            Value::Null,
            json!(true),
            json!({}),
            json!({"ok":false}),
            json!({"ok":"true"}),
            json!({"ok":1}),
            json!({"ok":true,"extra":0}),
        ] {
            assert!(validate_permission_reply(&bad).is_err());
        }
    }
    #[test]
    fn privilege_controls_have_exact_binary_journal_contracts() {
        for (id, target, reboot) in [
            ("installer.always_install_elevated", 0, false),
            ("lsa.restrict_anonymous_sam", 1, false),
            ("lsa.limit_blank_password_use", 1, false),
            ("wdigest.use_logon_credential", 0, true),
        ] {
            let control = controls().into_iter().find(|c| c.id == id).unwrap();
            assert_eq!(control.target, json!({"present":true,"value":target}));
            assert_eq!(control.reboot, reboot);
            for original in [
                json!({"present":false,"value":null}),
                json!({"present":true,"value":0}),
                json!({"present":true,"value":1}),
            ] {
                validate_value(id, &original).unwrap();
            }
            for bad in [
                json!({"present":true,"value":2}),
                json!({"present":true,"value":5}),
                json!({"present":true,"value":-1}),
                json!({"present":true,"value":1.0}),
                json!({"present":true,"value":true}),
                json!({"present":true,"value":"1"}),
                json!({"present":true,"value":null}),
                json!({"present":false,"value":0}),
                json!({"present":false}),
                json!({"present":0,"value":null}),
                json!({"present":true,"value":1,"path":"HKCU"}),
            ] {
                assert!(validate_value(id, &bad).is_err(), "{id} accepted {bad}");
            }
        }
    }
    #[test]
    fn fixed_targets_validate() {
        for c in controls() {
            validate_value(&c.id, &c.target).unwrap();
        }
    }
    #[test]
    fn restoration_values_are_strict() {
        for bad in [json!(0), json!("false"), Value::Null, json!({"value":true})] {
            assert!(validate_value("defender.realtime", &bad).is_err());
        }
        for bad in [
            json!({"present":true,"value":6}),
            json!({"present":true,"value":-1}),
            json!({"present":true,"value":1.0}),
            json!({"present":false,"value":0}),
            json!({"present":false}),
            json!({"present":true,"value":1,"extra":0}),
        ] {
            assert!(validate_value("uac.consent", &bad).is_err());
        }
        assert!(validate_value("uac.enabled", &json!({"present":true,"value":2})).is_err());
        assert!(validate_value("uac.enabled", &json!({"present":false,"value":null})).is_ok());
        for n in 0..=5 {
            assert!(validate_value("uac.consent", &json!({"present":true,"value":n})).is_ok());
        }
        assert!(validate_value("firewall.public.inbound", &json!("Allow")).is_ok());
        assert!(validate_value("firewall.public.inbound", &json!("block")).is_err());
        assert!(validate_value("defender.realtime; exit", &json!(false)).is_err());
    }

    #[test]
    fn all_control_ids_reject_untyped_or_executable_input() {
        for control in controls() {
            for value in [
                Value::Null,
                json!([]),
                json!("'; Start-Process cmd; '"),
                json!({"present":true,"value":"0"}),
            ] {
                assert!(
                    validate_value(&control.id, &value).is_err(),
                    "{} accepted {value}",
                    control.id
                );
            }
            if control.id.starts_with("defender.")
                || control.id.starts_with("firewall.") && control.id.ends_with(".enabled")
            {
                for value in [json!(true), json!(false)] {
                    validate_value(&control.id, &value).unwrap();
                }
            }
            if control.id.ends_with(".inbound") {
                for value in [json!("Block"), json!("Allow"), json!("NotConfigured")] {
                    validate_value(&control.id, &value).unwrap();
                }
            }
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_platform_never_advertises_a_backend_or_state_directory() {
        assert!(backend().is_err());
        assert!(is_elevated().is_err());
        assert!(elevate(&[]).is_err());
        assert!(state_dir().is_err());
        assert!(permission_gate("permissions.service.bits").is_err());
        assert!(permission_gate("permissions.service.wuauserv").is_err());
    }
}
