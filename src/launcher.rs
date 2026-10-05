//! Unelevated entry point (no arguments): create the broker pipe, request UAC
//! for `gui --broker <id>` every time, serve broker requests until the GUI
//! exits. Also provides the GUI single-instance guard.
//! OWNER: platform agent.
use crate::i18n::Lang;

pub fn run(lang: Lang) -> anyhow::Result<i32> {
    let _ = lang;
    anyhow::bail!("launcher not implemented")
}
