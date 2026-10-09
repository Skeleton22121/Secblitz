use crate::i18n::Lang;

#[cfg(windows)]
#[path = "tray/browser.rs"]
pub mod browser;
#[cfg_attr(not(windows), allow(dead_code))]
mod logic;
#[cfg_attr(not(windows), allow(dead_code))]
pub mod warn_logic;
#[cfg(windows)]
#[path = "tray/windows.rs"]
mod win;

#[cfg(windows)]
pub fn run(lang: Lang) -> anyhow::Result<i32> {
    win::run(lang)
}

#[cfg(not(windows))]
pub fn run(lang: Lang) -> anyhow::Result<i32> {
    let _ = lang;
    Ok(0)
}
