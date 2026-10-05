//! Per-user GUI preferences (theme, language override, onboarding seen).
//! OWNER: platform agent. Stored as small JSON in the engine state dir.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    #[default]
    Light,
    Dark,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub theme: ThemeChoice,
    /// Two-letter language code, or None for the Windows display language.
    #[serde(default)]
    pub lang: Option<String>,
}

pub fn load() -> Prefs {
    // TODO(platform)
    Prefs::default()
}

pub fn save(prefs: &Prefs) -> anyhow::Result<()> {
    // TODO(platform)
    let _ = prefs;
    Ok(())
}
