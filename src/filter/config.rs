//! The two small files shared by the app and the filter service: the switches
//! the app writes (`config.json`) and the status the service writes.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::matcher::Switches;

const MAX_CONFIG: u64 = 4 * 1024;
const MAX_STATUS: u64 = 16 * 1024;
const FRESH_SECONDS: u64 = 120;

#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
pub struct Config {
    pub ads: bool,
    pub tracking: bool,
    pub dangerous: bool,
    /// Unix seconds; everything is off until then.
    #[serde(default)]
    pub paused_until: Option<u64>,
}

impl Config {
    pub fn any_on(&self) -> bool {
        self.ads || self.tracking || self.dangerous
    }

    pub fn paused(&self, now: u64) -> bool {
        self.paused_until.is_some_and(|t| t > now)
    }

    /// The switches that count right now (all off while paused).
    pub fn active(&self, now: u64) -> Switches {
        if self.paused(now) {
            return Switches::default();
        }
        Switches {
            ads: self.ads,
            tracking: self.tracking,
            dangerous: self.dangerous,
        }
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    #[default]
    Starting,
    Ready,
    NoLists,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    PortInUse,
    DownloadFailed,
    ListInvalid,
    NoUpstream,
}

#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
pub struct Status {
    pub listening: bool,
    pub state: State,
    pub lists_updated: Option<u64>,
    pub day: u64,
    /// Blocked lookups today: ads, tracking, dangerous.
    pub blocked: [u64; 3],
    /// Domains in each list: ads, tracking, dangerous.
    pub domains: [u64; 3],
    pub last_error: Option<ErrorCode>,
    pub written_at: u64,
}

/// Reads a small file, refusing anything over `max` bytes.
fn read_capped(path: &Path, max: u64) -> Option<Vec<u8>> {
    let file = fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    file.take(max + 1).read_to_end(&mut buf).ok()?;
    (buf.len() as u64 <= max).then_some(buf)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().context("File has no folder")?;
    let name = path.file_name().context("File has no name")?;
    let mut tmp_name = name.to_os_string();
    tmp_name.push(".tmp");
    let tmp = dir.join(tmp_name);
    fs::write(&tmp, bytes).with_context(|| format!("Cannot write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("Cannot replace {}", path.display()))?;
    Ok(())
}

/// Missing, oversize or unreadable files mean everything off.
pub fn load_config(path: &Path) -> Config {
    read_capped(path, MAX_CONFIG)
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_config(path: &Path, c: &Config) -> Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(c)?)
}

pub fn load_status(path: &Path) -> Option<Status> {
    serde_json::from_slice(&read_capped(path, MAX_STATUS)?).ok()
}

pub fn save_status(path: &Path, s: &Status) -> Result<()> {
    write_atomic(path, &serde_json::to_vec(s)?)
}

/// A status written in the last two minutes: the service is alive.
pub fn fresh(status: &Status, now: u64) -> bool {
    status.written_at <= now && now - status.written_at <= FRESH_SECONDS
}

/// `<ProgramData>\Secblitz\Filter`. On Windows the folder comes from the
/// known-folder API, never from the inherited environment; the `ProgramData`
/// variable is only the fallback for the portable (test) build.
pub fn dir() -> Result<PathBuf> {
    Ok(program_data()?.join("Secblitz").join("Filter"))
}

#[cfg(windows)]
fn program_data() -> Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use std::ptr::null_mut;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramData, SHGetKnownFolderPath};

    let mut value = null_mut();
    // SAFETY: valid GUID and output pointer; the allocation is freed below
    // even when the call fails.
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramData, 0, null_mut(), &mut value) };
    let path = if hr >= 0 && !value.is_null() {
        // SAFETY: a successful call returns a NUL-terminated UTF-16 string.
        let slice = unsafe {
            let mut n = 0;
            while *value.add(n) != 0 {
                n += 1;
            }
            std::slice::from_raw_parts(value, n)
        };
        Some(PathBuf::from(std::ffi::OsString::from_wide(slice)))
    } else {
        None
    };
    // SAFETY: null or the allocation returned by SHGetKnownFolderPath.
    unsafe { CoTaskMemFree(value.cast()) };
    path.filter(|p| p.is_absolute())
        .context("ProgramData is not available")
}

#[cfg(not(windows))]
fn program_data() -> Result<PathBuf> {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .context("ProgramData is not available")
}

pub fn config_path() -> Result<PathBuf> {
    Ok(dir()?.join("config.json"))
}

pub fn data_dir() -> Result<PathBuf> {
    Ok(dir()?.join("Data"))
}

pub fn status_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("status.json"))
}

pub fn lists_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("lists"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_turns_everything_off_until_time() {
        let c = Config {
            ads: true,
            tracking: true,
            dangerous: false,
            paused_until: Some(1000),
        };
        assert!(c.any_on());
        assert!(c.paused(999));
        assert_eq!(c.active(999), Switches::default());
        assert!(!c.paused(1000));
        assert_eq!(
            c.active(1000),
            Switches {
                ads: true,
                tracking: true,
                dangerous: false
            }
        );
        assert!(!Config::default().any_on());
    }

    #[test]
    fn config_round_trips_and_old_files_load() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.json");
        let c = Config {
            ads: true,
            paused_until: Some(5),
            ..Config::default()
        };
        save_config(&p, &c).unwrap();
        assert_eq!(load_config(&p), c);
        fs::write(&p, r#"{"ads":true,"tracking":false,"dangerous":true}"#).unwrap();
        let old = load_config(&p);
        assert!(old.ads && old.dangerous && old.paused_until.is_none());
    }

    #[test]
    fn oversize_config_is_default() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.json");
        let pad = " ".repeat(5000);
        fs::write(
            &p,
            format!(r#"{{"ads":true,"tracking":true,"dangerous":true}}{pad}"#),
        )
        .unwrap();
        assert_eq!(load_config(&p), Config::default());
        fs::write(&p, "not json").unwrap();
        assert_eq!(load_config(&p), Config::default());
        assert_eq!(
            load_config(&d.path().join("missing.json")),
            Config::default()
        );
    }

    #[test]
    fn status_round_trips() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("status.json");
        let s = Status {
            listening: true,
            state: State::NoLists,
            lists_updated: Some(7),
            day: 20000,
            blocked: [1, 2, 3],
            domains: [10, 20, 30],
            last_error: Some(ErrorCode::PortInUse),
            written_at: 99,
        };
        save_status(&p, &s).unwrap();
        assert_eq!(load_status(&p), Some(s));
        let text = fs::read_to_string(&p).unwrap();
        assert!(text.contains("\"no-lists\"") && text.contains("\"port-in-use\""));
        assert_eq!(load_status(&d.path().join("missing.json")), None);
        fs::write(&p, vec![b' '; 20_000]).unwrap();
        assert_eq!(load_status(&p), None);
    }

    #[test]
    fn freshness_window() {
        let s = Status {
            written_at: 1000,
            ..Status::default()
        };
        assert!(fresh(&s, 1000));
        assert!(fresh(&s, 1120));
        assert!(!fresh(&s, 1121));
        // A timestamp from the future is not fresh.
        assert!(!fresh(&s, 999));
    }
}
