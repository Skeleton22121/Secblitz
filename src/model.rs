use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Control {
    pub id: String,
    pub title: String,
    pub description: String,
    pub target: serde_json::Value,
    pub reboot: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum EffectiveFirewall {
    Enabled(bool),
    Inbound(InboundAction),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InboundAction {
    Block,
    Allow,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Authority {
    Local,
    Managed,
    Unknown,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Observation {
    pub value: serde_json::Value,
    pub eligible: bool,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective: Option<EffectiveFirewall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority: Option<Authority>,
    /// Names of the accounts a fix would switch off, so the person can see them
    /// before approving. Only the old-accounts control supplies them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
}

/// Validate typed evidence at both native decoding and the engine boundary.
/// Missing evidence is representable, but never proves effective protection.
pub fn validate_observation(id: &str, obs: &Observation) -> anyhow::Result<()> {
    use anyhow::ensure;
    let enabled = matches!(
        id,
        "firewall.domain.enabled" | "firewall.private.enabled" | "firewall.public.enabled"
    );
    let inbound = matches!(
        id,
        "firewall.domain.inbound" | "firewall.private.inbound" | "firewall.public.inbound"
    );
    ensure!(
        (enabled || inbound) || (obs.effective.is_none() && obs.authority.is_none()),
        "Firewall evidence is invalid for this control"
    );
    ensure!(
        match obs.effective {
            None => true,
            Some(EffectiveFirewall::Enabled(_)) => enabled,
            Some(EffectiveFirewall::Inbound(_)) => inbound,
        },
        "Firewall evidence does not match the control"
    );
    ensure!(
        !obs.eligible || !matches!(obs.authority, Some(Authority::Managed | Authority::Unknown)),
        "Nonlocal firewall authority cannot be eligible"
    );
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum Probe<T> {
    Known(T),
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VolumeReadiness {
    pub available_bytes: u64,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PowerReadiness {
    pub ac_connected: Option<bool>,
    pub battery_percent: Option<u8>,
    pub battery_present: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Readiness {
    pub system_volume: Probe<VolumeReadiness>,
    pub journal_volume: Probe<VolumeReadiness>,
    pub power: Probe<PowerReadiness>,
    pub windows_update_reboot: Probe<bool>,
}

impl Readiness {
    /// Confirmed storage conditions that block new repairs. Unknown probes and
    /// informational power/reboot signals do not block; undo keeps its own gates.
    pub fn blocks_repairs(&self) -> bool {
        matches!(&self.system_volume, Probe::Known(v) if v.read_only)
            || matches!(&self.journal_volume, Probe::Known(v) if v.read_only || v.available_bytes == 0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub title: String,
    pub status: String,
    pub detail: String,
}

pub trait Backend {
    fn machine_id(&mut self) -> anyhow::Result<String>;
    fn controls(&self) -> Vec<Control>;
    fn observe(&mut self, id: &str) -> anyhow::Result<Observation>;
    /// Observe several controls, results in the order of `ids`. Reads one at
    /// a time unless a backend knows its reads are independent.
    fn observe_many(&mut self, ids: &[&str]) -> Vec<anyhow::Result<Observation>> {
        ids.iter().map(|id| self.observe(id)).collect()
    }
    fn write(&mut self, id: &str, value: &serde_json::Value) -> anyhow::Result<()>;
    fn findings(&mut self) -> anyhow::Result<Vec<Finding>>;
    fn readiness(&mut self) -> Readiness {
        Readiness::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn firewall_evidence_is_typed_control_scoped_and_never_grants_eligibility() {
        for profile in ["domain", "private", "public"] {
            for (suffix, raw, evidence, wrong) in [
                (
                    "enabled",
                    json!(true),
                    EffectiveFirewall::Enabled(true),
                    EffectiveFirewall::Inbound(InboundAction::Block),
                ),
                (
                    "inbound",
                    json!("NotConfigured"),
                    EffectiveFirewall::Inbound(InboundAction::Block),
                    EffectiveFirewall::Enabled(true),
                ),
            ] {
                let id = format!("firewall.{profile}.{suffix}");
                let mut obs = Observation {
                    value: raw,
                    eligible: true,
                    effective: Some(evidence),
                    authority: Some(Authority::Local),
                    ..Default::default()
                };
                validate_observation(&id, &obs).unwrap();
                for foreign in [
                    "defender.realtime",
                    "firewall.other.inbound",
                    "permissions.service.bits",
                ] {
                    assert!(validate_observation(foreign, &obs).is_err());
                }
                obs.effective = Some(wrong);
                assert!(validate_observation(&id, &obs).is_err());
                obs.effective = Some(evidence);
                for authority in [Authority::Managed, Authority::Unknown] {
                    obs.authority = Some(authority);
                    assert!(validate_observation(&id, &obs).is_err());
                    obs.eligible = false;
                    validate_observation(&id, &obs).unwrap();
                    obs.eligible = true;
                }
                obs.effective = None;
                obs.authority = None;
                validate_observation(&id, &obs).unwrap();
            }
        }
    }

    #[test]
    fn evidence_serialization_rejects_untyped_payloads_and_defaults_to_unknown() {
        let old: Observation = serde_json::from_value(
            json!({"value":"NotConfigured","eligible":true,"reason":"local"}),
        )
        .unwrap();
        assert!(old.effective.is_none() && old.authority.is_none());
        assert!(serde_json::to_value(&old)
            .unwrap()
            .get("effective")
            .is_none());
        assert_eq!(
            serde_json::to_value(EffectiveFirewall::Inbound(InboundAction::Block)).unwrap(),
            json!({"kind":"inbound","value":"block"})
        );
        for bad in [
            json!({"kind":"inbound","value":"Block"}),
            json!({"kind":"enabled","value":"true"}),
            json!({"kind":"inbound","value":"NotConfigured"}),
            json!({"kind":"inbound","value":"block","command":"execute"}),
        ] {
            assert!(serde_json::from_value::<EffectiveFirewall>(bad).is_err());
        }
        assert_eq!(
            serde_json::to_value(Readiness::default()).unwrap(),
            json!({
                "system_volume":{"status":"unknown"}, "journal_volume":{"status":"unknown"},
                "power":{"status":"unknown"}, "windows_update_reboot":{"status":"unknown"}
            })
        );
    }
}
