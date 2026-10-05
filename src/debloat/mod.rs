//! Clean up apps: remove preinstalled Windows apps the user doesn't want.
//!
//! Public contract used by the GUI page and the broker:
//! - `catalog()` — compiled, ordered list of known apps (index = stable id).
//! - `inventory()` — which catalog apps are installed on this PC (blocking).
//! - `remove(indices, emit)` — remove for all users + deprovision (blocking).
//! - `journal` — what was removed, for the Removed apps list and History.
//!
//! Safety: only packages that match a catalog entry and are not on the
//! protected list are ever passed to Windows, and names are validated before
//! any PowerShell is started.
pub mod backup;
pub mod catalog;
pub mod vault;
#[cfg(windows)]
pub(crate) mod wincrypto;
pub mod journal;
#[cfg(windows)]
mod windows;

#[cfg(test)]
mod tests;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub use catalog::{is_protected, matches as pattern_matches};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    /// Pre-selected: clearly unnecessary Microsoft apps.
    Recommended,
    /// Pre-selected: sponsored third-party apps and games.
    Sponsored,
    /// Not pre-selected: Copilot, Widgets, Teams, Outlook (new)…
    Promotions,
    /// Not pre-selected: small utilities some people use.
    Utilities,
    /// Not pre-selected, with a warning: Xbox / Game Bar.
    Gaming,
}

impl Group {
    pub const ALL: [Group; 5] = [
        Group::Recommended,
        Group::Sponsored,
        Group::Promotions,
        Group::Utilities,
        Group::Gaming,
    ];

    pub fn selected_by_default(self) -> bool {
        matches!(self, Group::Recommended | Group::Sponsored)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct App {
    /// Package name, or a publisher prefix ending in `*` (e.g. "king.com.*").
    pub family: &'static str,
    /// Friendly English name (translation source key).
    pub name: &'static str,
    pub group: Group,
    /// Microsoft Store product id for automatic restore, if verified.
    pub store_id: Option<&'static str>,
}

/// One installed catalog package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    pub index: u16,
    pub package: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemResult {
    Removed,
    /// Windows protects this app; nothing changed.
    Protected,
    Failed(String),
}

/// Live progress of `remove`, per catalog app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    Started(u16),
    Finished(u16, ItemResult),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Removed {
    pub index: u16,
    pub package: String,
    pub version: String,
    /// Set after a successful restore.
    #[serde(default)]
    pub restored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub index: u16,
    /// Technical reason, for the Technical details expander only.
    pub reason: String,
}

/// Outcome of one removal run; also one line of `debloat.jsonl`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Batch {
    /// Unix seconds.
    pub t: u64,
    pub removed: Vec<Removed>,
    /// Catalog indices Windows protects.
    pub skipped: Vec<u16>,
    pub failed: Vec<Failure>,
}

impl Batch {
    /// Number of distinct catalog apps removed.
    pub fn removed_apps(&self) -> usize {
        let mut seen: Vec<u16> = self.removed.iter().map(|r| r.index).collect();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    }
}

pub fn catalog() -> &'static [App] {
    catalog::CATALOG
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---- inventory -----------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(windows), allow(dead_code))]
struct RawPackage {
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    non_removable: bool,
    #[serde(default)]
    framework: bool,
}

/// Turn the inventory script's JSON into catalog packages. Anything outside
/// the catalog, protected, malformed, non-removable or a framework is dropped.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn parse_inventory(json: &str) -> Result<Vec<Installed>> {
    let raw: Vec<RawPackage> = if json.trim_start().starts_with('{') {
        vec![serde_json::from_str(json).context("Read the app list")?]
    } else {
        serde_json::from_str(json).context("Read the app list")?
    };
    let mut found: Vec<Installed> = raw
        .into_iter()
        .filter(|p| !p.non_removable && !p.framework)
        .filter_map(|p| {
            catalog::owner(&p.name).map(|index| Installed {
                index,
                package: p.name,
                version: p.version.chars().take(64).collect(),
            })
        })
        .collect();
    found.sort_by(|a, b| (a.index, &a.package).cmp(&(b.index, &b.package)));
    found.dedup_by(|a, b| a.package.eq_ignore_ascii_case(&b.package));
    Ok(found)
}

/// Which catalog apps are present on this PC. Blocking (a few seconds).
pub fn inventory() -> Result<Vec<Installed>> {
    #[cfg(windows)]
    {
        let json = windows::run(windows::INVENTORY, &[], std::time::Duration::from_secs(180))?;
        parse_inventory(&json)
    }
    #[cfg(not(windows))]
    {
        bail!("Removing apps is only available on Windows")
    }
}

// ---- removal -------------------------------------------------------------

/// What the removal script reports for one package.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum PackageOutcome {
    Removed,
    Protected,
    Failed(String),
}

#[derive(Debug, Deserialize)]
#[cfg_attr(not(windows), allow(dead_code))]
struct RawOutcome {
    #[serde(default)]
    removed: bool,
    #[serde(default)]
    protected: bool,
    #[serde(default)]
    error: Option<String>,
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn parse_outcome(json: &str) -> PackageOutcome {
    match serde_json::from_str::<RawOutcome>(json) {
        Ok(o) if o.protected => PackageOutcome::Protected,
        Ok(o) if o.removed => PackageOutcome::Removed,
        Ok(o) => PackageOutcome::Failed(
            o.error
                .map(|e| crate::text::excerpt(&e, 300))
                .filter(|e| !e.is_empty())
                .unwrap_or_else(|| "Unknown result".into()),
        ),
        Err(_) => PackageOutcome::Failed("Unreadable answer from Windows".into()),
    }
}

/// Validate a request against the catalog and the protected list.
pub(crate) fn validate_indices(indices: &[u16]) -> Result<Vec<u16>> {
    let mut clean = Vec::new();
    for &index in indices {
        let Some(app) = catalog().get(index as usize) else {
            bail!("Unknown app");
        };
        // A catalog entry that could ever match a protected package is a bug.
        if is_protected(app.family.trim_end_matches('*')) {
            bail!("That app can't be removed");
        }
        if !clean.contains(&index) {
            clean.push(index);
        }
    }
    Ok(clean)
}

/// Core of `remove`, with the PowerShell call injected so it is testable.
/// `installed` is a fresh inventory; every package is re-validated here.
pub(crate) fn remove_with(
    indices: &[u16],
    installed: &[Installed],
    run: &dyn Fn(&str) -> PackageOutcome,
    emit: &dyn Fn(Progress),
) -> Result<Batch> {
    let indices = validate_indices(indices)?;
    let mut batch = Batch {
        t: now(),
        ..Batch::default()
    };
    for index in indices {
        let packages: Vec<&Installed> = installed
            .iter()
            .filter(|p| p.index == index)
            // Never trust the caller's list: re-derive ownership.
            .filter(|p| catalog::owner(&p.package) == Some(index))
            .collect();
        if packages.is_empty() {
            continue;
        }
        emit(Progress::Started(index));
        let mut removed = Vec::new();
        let mut protected = false;
        let mut failure: Option<String> = None;
        for p in packages {
            match run(&p.package) {
                PackageOutcome::Removed => removed.push(Removed {
                    index,
                    package: p.package.clone(),
                    version: p.version.clone(),
                    restored: false,
                }),
                PackageOutcome::Protected => protected = true,
                PackageOutcome::Failed(e) => failure = Some(e),
            }
        }
        let result = if let Some(reason) = failure {
            batch.failed.push(Failure {
                index,
                reason: reason.clone(),
            });
            ItemResult::Failed(reason)
        } else if removed.is_empty() && protected {
            batch.skipped.push(index);
            ItemResult::Protected
        } else {
            ItemResult::Removed
        };
        batch.removed.append(&mut removed);
        emit(Progress::Finished(index, result));
    }
    Ok(batch)
}

/// Remove catalog apps `indices` for all users and from the Windows image.
/// Blocking; reports per-app progress. Appends the batch to the journal.
pub fn remove(indices: &[u16], emit: &dyn Fn(Progress)) -> Result<Batch> {
    let indices = validate_indices(indices)?;
    let installed = inventory()?;
    #[cfg(windows)]
    let run = |package: &str| -> PackageOutcome {
        if !catalog::is_valid_package_name(package) || is_protected(package) {
            return PackageOutcome::Failed("Refused".into());
        }
        match windows::run(
            windows::REMOVE,
            &[("SECBLITZ_APP", package)],
            std::time::Duration::from_secs(300),
        ) {
            Ok(json) => parse_outcome(&json),
            Err(e) => PackageOutcome::Failed(format!("{e:#}")),
        }
    };
    #[cfg(not(windows))]
    let run =
        |_: &str| -> PackageOutcome { PackageOutcome::Failed("Only available on Windows".into()) };
    let batch = remove_with(&indices, &installed, &run, emit)?;
    if !batch.removed.is_empty() || !batch.skipped.is_empty() || !batch.failed.is_empty() {
        let _ = journal::append(&batch);
    }
    Ok(batch)
}

/// Ask Windows (machine-wide policy) not to push suggested apps. Needs an
/// elevated caller. Fully enforced only on Enterprise and Education editions.
pub fn set_consumer_features_policy() -> Result<()> {
    #[cfg(windows)]
    {
        let json = windows::run(windows::POLICY, &[], std::time::Duration::from_secs(60))?;
        let value: serde_json::Value = serde_json::from_str(&json).context("Read the answer")?;
        if value.get("ok") == Some(&serde_json::Value::Bool(true)) {
            Ok(())
        } else {
            bail!("Windows did not confirm the setting")
        }
    }
    #[cfg(not(windows))]
    {
        bail!("Only available on Windows")
    }
}
