//! Fixed-service exact-state DACL repair and bounded advisory auditing.
use crate::model::{Backend, CheckStatus, Control, Finding, Observation, Readiness};
use anyhow::Result;
use serde_json::Value;

mod state;

#[cfg(any(windows, test))]
mod descriptor;
#[cfg(windows)]
mod windows;

/// Catalog marker only; never pass it to observe/write/repair_target.
pub const TARGET_SENTINEL: &str = "service-dacl-repair-v1";

pub fn controls() -> Vec<Control> {
    [("permissions.service.bits", "BITS"), ("permissions.service.wuauserv", "Windows Update")]
        .into_iter().map(|(id, name)| Control {
            id: id.into(), title: format!("Repair dangerous {name} service permissions"),
            description: "Remove dangerous explicit broad-principal service grants; preserve other ACE bytes and require exact-state rollback.".into(),
            target: serde_json::json!(TARGET_SENTINEL), reboot: false,
        }).collect()
}

fn service_name(id: &str) -> Result<&'static str> {
    match id {
        "permissions.service.bits" => Ok("BITS"),
        "permissions.service.wuauserv" => Ok("wuauserv"),
        _ => anyhow::bail!("Unknown service permission control: {id}"),
    }
}

/// Unsupported ACEs are kept as bounded opaque records for observation; repair_target always rejects them.
pub fn validate_value(id: &str, value: &Value) -> Result<()> {
    service_name(id)?;
    state::State::parse(value)?;
    Ok(())
}

/// Engine must derive its target from the durable BEFORE image on every replay.
pub fn repair_target(id: &str, before: &Value) -> Result<Value> {
    service_name(id)?;
    Ok(state::State::parse(before)?.repair()?.value())
}

pub fn observe(id: &str) -> Result<Observation> {
    service_name(id)?;
    #[cfg(windows)]
    {
        windows::observe(id)
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Service permission observation requires Windows")
    }
}

/// Privileged engine boundary, not an untrusted restore API: the engine must authorize rollback against its durable before-image, which the native inverse check cannot prove.
pub fn write(id: &str, value: &Value) -> Result<()> {
    validate_value(id, value)?;
    #[cfg(windows)]
    {
        windows::write(id, value)
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Service permission repair requires Windows")
    }
}

/// Adds fixed repair controls plus advisory findings; the native boundary repeats platform::permission_gate(id) on observe and before every write.
pub fn with_permissions(delegate: Box<dyn Backend>) -> Box<dyn Backend> {
    Box::new(PermissionBackend {
        delegate: with_audit(delegate),
    })
}

struct PermissionBackend {
    delegate: Box<dyn Backend>,
}
impl Backend for PermissionBackend {
    fn machine_id(&mut self) -> Result<String> {
        self.delegate.machine_id()
    }
    fn controls(&self) -> Vec<Control> {
        let mut result = self.delegate.controls();
        result.extend(controls());
        result
    }
    fn observe(&mut self, id: &str) -> Result<Observation> {
        if id.starts_with("permissions.") {
            observe(id)
        } else {
            self.delegate.observe(id)
        }
    }
    fn observe_many(&mut self, ids: &[&str]) -> Vec<Result<Observation>> {
        let ours = |id: &&str| id.starts_with("permissions.");
        let theirs: Vec<&str> = ids.iter().copied().filter(|id| !ours(id)).collect();
        let mut delegated = self.delegate.observe_many(&theirs).into_iter();
        ids.iter()
            .map(|id| {
                if ours(id) {
                    observe(id)
                } else {
                    delegated
                        .next()
                        .unwrap_or_else(|| Err(anyhow::anyhow!("Some details for a check could not be read.")))
                }
            })
            .collect()
    }
    fn write(&mut self, id: &str, value: &Value) -> Result<()> {
        if id.starts_with("permissions.") {
            write(id, value)
        } else {
            self.delegate.write(id, value)
        }
    }
    fn findings(&mut self) -> Result<Vec<Finding>> {
        self.delegate.findings()
    }
    fn readiness(&mut self) -> Readiness {
        self.delegate.readiness()
    }
}

/// Individual failures are findings, not a reason to skip other services. Not an AccessCheck.
pub fn audit() -> Result<Vec<Finding>> {
    #[cfg(windows)]
    {
        Ok(windows::audit())
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Service permission auditing requires Windows")
    }
}

/// Opt-in adapter: retain the delegate's controls, eligibility and write policy.
/// No permission controls are added to the engine's fixed-target allowlist.
pub fn with_audit(delegate: Box<dyn Backend>) -> Box<dyn Backend> {
    Box::new(AuditedBackend { delegate })
}

struct AuditedBackend {
    delegate: Box<dyn Backend>,
}

impl Backend for AuditedBackend {
    fn readiness(&mut self) -> Readiness {
        self.delegate.readiness()
    }
    fn machine_id(&mut self) -> Result<String> {
        self.delegate.machine_id()
    }
    fn controls(&self) -> Vec<Control> {
        self.delegate.controls()
    }
    fn observe(&mut self, id: &str) -> Result<Observation> {
        self.delegate.observe(id)
    }
    fn observe_many(&mut self, ids: &[&str]) -> Vec<Result<Observation>> {
        self.delegate.observe_many(ids)
    }
    fn write(&mut self, id: &str, value: &Value) -> Result<()> {
        self.delegate.write(id, value)
    }
    fn findings(&mut self) -> Result<Vec<Finding>> {
        let mut findings = self.delegate.findings()?;
        match audit() {
            Ok(extra) => findings.extend(extra),
            Err(error) => findings.push(Finding {
                title: "Service permission audit".into(),
                status: CheckStatus::Unknown,
                detail: error.to_string(),
            }),
        }
        Ok(findings)
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    #[test]
    fn both_adapters_forward_readiness_once_without_other_probes() {
        use crate::model::{Probe, VolumeReadiness};
        use std::{cell::Cell, rc::Rc};
        struct ReadinessOnly(Rc<Cell<usize>>);
        impl Backend for ReadinessOnly {
            fn machine_id(&mut self) -> Result<String> {
                panic!("unexpected machine probe")
            }
            fn controls(&self) -> Vec<Control> {
                panic!("unexpected catalog probe")
            }
            fn observe(&mut self, _: &str) -> Result<Observation> {
                panic!("unexpected observation")
            }
            fn write(&mut self, _: &str, _: &Value) -> Result<()> {
                panic!("unexpected write")
            }
            fn findings(&mut self) -> Result<Vec<Finding>> {
                panic!("unexpected findings")
            }
            fn readiness(&mut self) -> Readiness {
                self.0.set(self.0.get() + 1);
                Readiness {
                    journal_volume: Probe::Known(VolumeReadiness {
                        available_bytes: 1u64 << 40,
                        read_only: true,
                    }),
                    ..Default::default()
                }
            }
        }
        let count = Rc::new(Cell::new(0));
        let mut wrapped = with_permissions(Box::new(ReadinessOnly(count.clone())));
        assert!(
            matches!(wrapped.readiness().journal_volume, Probe::Known(v) if v.read_only && v.available_bytes == 1u64 << 40)
        );
        assert_eq!(count.get(), 1);
    }

    struct Delegate;
    impl Backend for Delegate {
        fn machine_id(&mut self) -> Result<String> {
            Ok("delegate-machine".into())
        }
        fn controls(&self) -> Vec<Control> {
            vec![]
        }
        fn observe(&mut self, id: &str) -> Result<Observation> {
            anyhow::bail!("delegate observation gate: {id}")
        }
        fn write(&mut self, id: &str, _: &Value) -> Result<()> {
            anyhow::bail!("delegate write gate: {id}")
        }
        fn findings(&mut self) -> Result<Vec<Finding>> {
            Ok(vec![Finding {
                title: "delegate evidence".into(),
                status: CheckStatus::Info,
                detail: "retained".into(),
            }])
        }
    }

    #[test]
    fn adapter_retains_delegate_policy_and_surfaces_unavailable_audit() {
        assert!(audit().is_err());
        let mut backend = with_audit(Box::new(Delegate));
        assert_eq!(backend.machine_id().unwrap(), "delegate-machine");
        assert!(backend.controls().is_empty());
        assert_eq!(
            backend
                .observe("permissions.service.bits")
                .unwrap_err()
                .to_string(),
            "delegate observation gate: permissions.service.bits"
        );
        assert_eq!(
            backend
                .write("permissions.service.bits", &Value::Null)
                .unwrap_err()
                .to_string(),
            "delegate write gate: permissions.service.bits"
        );
        let findings = backend.findings().unwrap();
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].title, "delegate evidence");
        assert_eq!(findings[1].status, CheckStatus::Unknown);
    }

    #[test]
    fn repair_adapter_adds_only_fixed_controls_and_rejects_foreign_permission_ids() {
        let mut backend = with_permissions(Box::new(Delegate));
        assert_eq!(backend.controls(), controls());
        assert_eq!(backend.machine_id().unwrap(), "delegate-machine");
        assert!(backend
            .observe("permissions.service.other")
            .unwrap_err()
            .to_string()
            .contains("Unknown service permission control"));
        assert!(backend
            .write(
                "permissions.service.bits",
                &serde_json::json!(TARGET_SENTINEL)
            )
            .is_err());
        assert!(backend
            .observe("defender.realtime")
            .unwrap_err()
            .to_string()
            .starts_with("delegate observation gate"));
        assert!(backend
            .write("defender.realtime", &Value::Bool(false))
            .unwrap_err()
            .to_string()
            .starts_with("delegate write gate"));
        assert_eq!(backend.findings().unwrap().len(), 2);
    }
}
