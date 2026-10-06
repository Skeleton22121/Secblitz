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
pub mod friendly;
pub mod icons;
pub mod journal;
pub mod offline;
pub mod suggested;
pub mod vault;
#[cfg(windows)]
pub(crate) mod wincrypto;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) mod winfs;

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

/// Why an app was left installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kept {
    /// Not enough free space to save a copy first.
    NoSpace,
    /// The copy couldn't be made (technical reason for the details only).
    NoCopy(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemResult {
    Removed,
    /// A copy could not be saved first, so the app was left installed.
    Kept(Kept),
    /// Windows protects this app; nothing changed.
    Protected,
    Failed(String),
}

/// Live progress of `remove`, per catalog app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// Saving a copy of the app before removing it.
    Saving(u16),
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
    /// Catalog indices left installed because no copy could be saved.
    #[serde(default)]
    pub kept: Vec<u16>,
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
    backup: &dyn Fn(&Installed) -> std::result::Result<(), Kept>,
    run: &dyn Fn(&str) -> PackageOutcome,
    emit: &dyn Fn(Progress),
) -> Result<Batch> {
    remove_with_checkpoint(indices, installed, backup, run, emit, &|_| {})
}

/// Same, and `checkpoint` sees the batch after every app, so the record of
/// what is already removed is never lost if the run is cut short.
pub(crate) fn remove_with_checkpoint(
    indices: &[u16],
    installed: &[Installed],
    backup: &dyn Fn(&Installed) -> std::result::Result<(), Kept>,
    run: &dyn Fn(&str) -> PackageOutcome,
    emit: &dyn Fn(Progress),
    checkpoint: &dyn Fn(&Batch),
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
        emit(Progress::Saving(index));
        let mut kept = None;
        for p in &packages {
            if let Err(k) = backup(p) {
                kept = Some(k);
                break;
            }
        }
        if let Some(k) = kept {
            batch.kept.push(index);
            emit(Progress::Finished(index, ItemResult::Kept(k)));
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
        checkpoint(&batch);
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
    #[cfg(windows)]
    let store = backup::Store::open().inspect(|s| s.clean_staging());
    #[cfg(windows)]
    let backup = |p: &Installed| -> std::result::Result<(), Kept> {
        match &store {
            Ok(store) => {
                offline::backup_family_with(&offline::WindowsHost, store, p.index, &p.package)
                    .map(|_| ())
            }
            Err(e) => Err(Kept::NoCopy(format!("{e:#}"))),
        }
    };
    #[cfg(not(windows))]
    let backup = |_: &Installed| -> std::result::Result<(), Kept> {
        Err(Kept::NoCopy("Only available on Windows".into()))
    };
    let recordable =
        |b: &Batch| !b.removed.is_empty() || !b.skipped.is_empty() || !b.failed.is_empty();
    // The record is saved after every app (replacing this run's own line), so
    // an app that is already gone is always on the list, even if the run stops.
    let checkpoint = |b: &Batch| {
        if recordable(b) {
            let _ = journal::upsert(b);
        }
    };
    let batch = remove_with_checkpoint(&indices, &installed, &backup, &run, emit, &checkpoint)?;
    if recordable(&batch) {
        let _ = journal::upsert(&batch);
    }
    Ok(batch)
}

/// Ask Windows (machine-wide policy) not to push suggested apps. Needs an
/// elevated caller. Fully enforced only on Enterprise and Education editions.
/// What was there before is recorded, so Remove Secblitz can put it back.
pub fn set_consumer_features_policy() -> Result<()> {
    #[cfg(windows)]
    {
        suggested::block(&mut suggested::MachinePolicy, &suggested::journal_path()?)
    }
    #[cfg(not(windows))]
    {
        bail!("Only available on Windows")
    }
}

/// Bookkeeping after an app is installed again: mark it restored in the
/// removed-apps list and drop its saved copy (which has done its job).
pub fn finish_restore(index: u16) {
    let _ = journal::mark_restored(index);
    offline::forget(index);
}

/// What `restore_all` did, as catalog indices.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RestoreAll {
    pub restored: Vec<u16>,
    /// Back only through the Microsoft Store.
    pub needs_store: Vec<u16>,
    pub failed: Vec<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bucket {
    Restored,
    NeedsStore,
    Failed,
}

/// Where one restore attempt ends up. `None` means there was no saved copy.
pub(crate) fn classify(outcome: Option<Result<offline::Restored>>, has_store_id: bool) -> Bucket {
    use offline::Restored::*;
    match outcome {
        Some(Ok(Back | BackWithoutSomeData | AlreadyThere)) => Bucket::Restored,
        _ if has_store_id => Bucket::NeedsStore,
        _ => Bucket::Failed,
    }
}

/// Put back every app that is still removed, from saved copies. Newest first.
/// Blocking. `emit(index, ok)` after each app.
pub fn restore_all(emit: &dyn Fn(u16, bool)) -> RestoreAll {
    let mut result = RestoreAll::default();
    for (index, _) in journal::still_removed(&journal::load(), catalog().len()) {
        let outcome = offline::has_copy(index).then(|| offline::restore_index(index));
        let has_store_id = catalog()[index as usize].store_id.is_some();
        match classify(outcome, has_store_id) {
            Bucket::Restored => {
                finish_restore(index);
                result.restored.push(index);
                emit(index, true);
            }
            Bucket::NeedsStore => {
                result.needs_store.push(index);
                emit(index, false);
            }
            Bucket::Failed => {
                result.failed.push(index);
                emit(index, false);
            }
        }
    }
    result
}
