//! Unelevated tray agent (`secblitz.exe tray`): shield icon coloured by
//! `status.json`, notifications when protection drops, menu Open / Check now /
//! Quit, exits on the updater's quiesce event.
//! OWNER: platform agent.
//!
//! The window is a hidden *top-level* window (not message-only): only top-level
//! windows receive the `TaskbarCreated` broadcast and the session-end messages
//! the installer's Restart Manager uses to close the tray.
use crate::i18n::Lang;

#[cfg_attr(not(windows), allow(dead_code))]
mod logic;
#[cfg(windows)]
#[path = "tray/windows.rs"]
mod win;

#[cfg(windows)]
pub fn run(lang: Lang) -> anyhow::Result<i32> {
    win::run(lang)
}

/// Host builds have no tray; exiting cleanly keeps scripts and tests simple.
#[cfg(not(windows))]
pub fn run(lang: Lang) -> anyhow::Result<i32> {
    let _ = lang;
    Ok(0)
}
