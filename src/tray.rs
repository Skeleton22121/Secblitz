//! Unelevated tray agent (`secblitz.exe tray`): shield icon coloured by
//! `status.json`, notifications when protection drops, menu Open / Check now /
//! Quit, exits on the updater's quiesce event.
//! OWNER: platform agent.
use crate::i18n::Lang;

pub fn run(lang: Lang) -> anyhow::Result<i32> {
    let _ = lang;
    anyhow::bail!("tray not implemented")
}
