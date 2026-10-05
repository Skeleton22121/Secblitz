//! Clean up apps: remove preinstalled Windows apps the user doesn't want.
//!
//! OWNER: debloat agent. Public contract used by the GUI page:
//! - `catalog()` — compiled, ordered list of known apps (index = stable id).
//! - `inventory()` — which catalog apps are installed on this PC (blocking).
//! - `remove(indices, progress)` — remove for all users + deprovision (blocking).
//! - `journal::load()` / restore support via the broker (`ReinstallStoreApp`).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    pub fn selected_by_default(self) -> bool {
        matches!(self, Group::Recommended | Group::Sponsored)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct App {
    /// Package family name prefix, e.g. "Microsoft.BingNews_" or "king.com.CandyCrush".
    pub family: &'static str,
    /// Friendly English name (translation source key).
    pub name: &'static str,
    pub group: Group,
    /// Microsoft Store product id for automatic restore, if verified.
    pub store_id: Option<&'static str>,
}

/// One installed catalog app.
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

pub fn catalog() -> &'static [App] {
    // TODO(debloat)
    &[]
}
