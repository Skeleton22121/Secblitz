//! Application-upgrade API: deliberately unsupported in production.
//!
//! The catalog is empty on purpose: no reviewed installer has yet passed
//! payload-identity and process-supervision checks, so nothing may be offered.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fmt, marker::PhantomData, time::Duration};
use uuid::Uuid;

#[path = "app_updates/catalog.rs"]
mod catalog;
pub use catalog::{CatalogReview, MAX_CATALOG_AGE_SECONDS, MAX_STABLE_OBSERVATION_AGE_SECONDS};

#[cfg(test)]
#[path = "app_updates/tests.rs"]
mod tests;

pub const UNSUPPORTED_REASON: &str = "Application upgrades are unsupported: no current stable installer workflow has fully validated payload identity, process supervision, and separately approved disruptive side effects. The production catalog is empty. Historical targets, expired reviews, and persisted plans cannot enable download or execution. No security-currency claim is available.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unsupported;

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(UNSUPPORTED_REASON)
    }
}

impl std::error::Error for Unsupported {}

fn unavailable<T>() -> Result<T> {
    Err(Unsupported.into())
}

#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    pub supported: bool,
    pub native_backend: bool,
    pub reviewed_packages: Vec<Release>,
    pub limitations: &'static str,
    pub assessment: Assessment,
}

#[derive(Debug, Clone, Serialize)]
pub struct Assessment {
    pub observed_on_utc: &'static str,
    pub publisher_stable_observed: &'static str,
    pub winget_release_observed: &'static str,
    pub publisher_source: &'static str,
    pub manifest_source: &'static str,
    pub license_source: &'static str,
    pub winget_source: &'static str,
}

pub fn capabilities() -> Capabilities {
    Capabilities {
        supported: false,
        native_backend: false,
        reviewed_packages: Vec::new(),
        limitations: UNSUPPORTED_REASON,
        assessment: Assessment {
            observed_on_utc: "2026-10-03",
            publisher_stable_observed: "1.140.0",
            winget_release_observed: "1.29.380",
            publisher_source: "https://update.code.visualstudio.com/api/update/win32-x64-user/stable/latest",
            manifest_source: "https://github.com/microsoft/winget-pkgs/blob/master/manifests/m/Microsoft/VisualStudioCode/1.140.0/Microsoft.VisualStudioCode.installer.yaml",
            license_source: "https://code.visualstudio.com/license",
            winget_source: "https://github.com/microsoft/winget-cli/releases/tag/v1.29.380",
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub original_sid: String,
    pub machine: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub package_id: String,
    pub version: String,
    pub source: String,
    pub source_identifier: String,
    pub source_url: String,
    pub architecture: String,
    pub scope: String,
    pub installer_sha256: String,
    pub license_name: String,
    pub license_url: String,
    pub license_text: String,
    pub source_terms: String,
    pub execution_policy: String,
    #[serde(default)]
    pub review: Option<CatalogReview>,
}

impl Release {
    pub fn validate_freshness_at(&self, utc_seconds: u64) -> Result<()> {
        catalog::validate(self, utc_seconds)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Installed {
    pub package_id: String,
    pub version: String,
    pub architecture: String,
    pub scope: String,
    pub executable_sha256: String,
    pub product_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Availability {
    Available {
        installed: Installed,
        release: Box<Release>,
    },
    NotApplicable {
        reason: String,
    },
    Unknown {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discovery {
    pub binding: Binding,
    pub checked_at: u64,
    pub application: Availability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: u32,
    pub id: Uuid,
    pub binding: Binding,
    pub created_at: u64,
    pub expires_at: u64,
    pub installed: Installed,
    pub release: Release,
    pub digest: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DisruptiveConsent {
    pub allow_tunnel_process_stop: bool,
    pub allow_tunnel_service_reconfiguration: bool,
    pub allow_context_menu_process_stop: bool,
    pub allow_shell_integration_replacement: bool,
    pub allow_previous_version_cleanup: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Consent {
    pub upgrade_selected_package: bool,
    pub accept_package_license: bool,
    pub accept_microsoft_source: bool,
    pub acknowledge_no_automatic_rollback: bool,
    #[serde(default)]
    pub disruptions: DisruptiveConsent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    pub digest: String,
    pub at: u64,
    pub consent: Consent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Planned,
    Consumed,
    /// Legacy exact-version observation only; NEVER a current-security claim.
    /// No operation in this implementation emits this status.
    Verified,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub plan: Plan,
    pub approval: Option<Approval>,
    pub status: Status,
    pub observed: Option<Installed>,
    pub detail: String,
}

pub struct Task<T> {
    _result: PhantomData<T>,
}

impl<T> Task<T> {
    pub fn wait(&self, _timeout: Duration) -> Result<Option<T>> {
        unavailable()
    }
}

pub fn discover() -> Result<Task<Discovery>> {
    unavailable()
}

pub fn plan(
    _package_id: &str,
    _exact_version: &str,
    _valid_for_seconds: u64,
) -> Result<Task<Plan>> {
    unavailable()
}

pub fn approve(_id: Uuid, _displayed_digest: &str, _consent: Consent) -> Result<Record> {
    unavailable()
}

pub fn start(_id: Uuid, _displayed_digest: &str) -> Result<Task<Record>> {
    unavailable()
}

pub fn verify(_id: Uuid) -> Result<Task<Record>> {
    unavailable()
}

pub fn list() -> Result<Vec<Record>> {
    unavailable()
}

pub fn get(_id: Uuid) -> Result<Record> {
    unavailable()
}
