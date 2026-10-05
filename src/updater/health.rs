//! CLI contract: hidden `update health --json` calls health() directly and prints
//! only the serialized result. No Engine, UAC retry, locks, UI or network. The
//! worker retains update.lock + engine.lock while running this read-only probe.
use super::*;

pub(super) const LIMIT: usize = 4096;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskHealth {
    Absent,
    Ready,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorHealth {
    Absent,
    Stopped,
    Running,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UpdateHealth {
    pub schema: u32,
    pub version: String,
    pub task: TaskHealth,
    pub monitor: MonitorHealth,
}

pub fn health() -> Result<UpdateHealth> {
    #[cfg(windows)]
    {
        super::windows::health()
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Update health requires Windows")
    }
}

pub(super) fn validate(
    bytes: &[u8],
    expected: &str,
    before: Option<&UpdateHealth>,
) -> Result<UpdateHealth> {
    ensure!(bytes.len() <= LIMIT, "Health output too large");
    let report: UpdateHealth = serde_json::from_slice(bytes)?;
    ensure!(
        report.schema == 1 && stable(&report.version)? == stable(expected)?,
        "Health schema/version mismatch"
    );
    if let Some(before) = before {
        ensure!(
            report.task == before.task && report.monitor == before.monitor,
            "Update did not preserve task/monitor state"
        );
    }
    Ok(report)
}
