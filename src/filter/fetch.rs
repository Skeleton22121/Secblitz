//! Downloading, storing and compiling the block lists. Portable and unit-tested.

use anyhow::{bail, ensure, Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use super::lists::{self, Inputs, Role, Source, SOURCES};
use super::matcher::{Category, Filter, HashSet64};

const SECONDS_PER_DAY: u64 = 86_400;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

pub fn due(source: &Source, last: Option<u64>, now: u64) -> bool {
    match last {
        None => true,
        Some(t) => now.saturating_sub(t) >= source.refresh_days * SECONDS_PER_DAY,
    }
}

/// HTTPS only, no redirects, no proxy, gzip unpacked by the client.
pub fn client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .gzip(true)
        .no_proxy()
        .timeout(DOWNLOAD_TIMEOUT)
        .user_agent(concat!("Secblitz/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("Cannot create the download client")
}

/// Only a plain 200 is a list; a redirect or an error page is not.
pub fn check_status(code: u16) -> Result<()> {
    ensure!(code == 200, "Unexpected response {code}");
    Ok(())
}

pub fn read_capped(reader: impl Read, max: u64) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    reader.take(max + 1).read_to_end(&mut out)?;
    ensure!(out.len() as u64 <= max, "The list is larger than allowed");
    Ok(out)
}

pub fn download(client: &reqwest::blocking::Client, source: &Source) -> Result<String> {
    let response = client.get(source.url).send()?;
    check_status(response.status().as_u16())?;
    let bytes = read_capped(response, source.max_bytes)?;
    String::from_utf8(bytes).context("The list is not text")
}

fn file_for(lists_dir: &Path, id: &str) -> std::path::PathBuf {
    lists_dir.join(format!("{id}.txt"))
}

fn known(id: &str) -> Option<&'static Source> {
    SOURCES.iter().find(|s| s.id == id)
}

pub fn store(lists_dir: &Path, id: &str, text: &str) -> Result<()> {
    ensure!(known(id).is_some(), "Unknown list");
    fs::create_dir_all(lists_dir)?;
    let path = file_for(lists_dir, id);
    let tmp = path.with_extension("txt.tmp");
    fs::write(&tmp, text).with_context(|| format!("Cannot write {}", tmp.display()))?;
    fs::rename(&tmp, &path).with_context(|| format!("Cannot replace {}", path.display()))?;
    Ok(())
}

pub fn stored_at(lists_dir: &Path, id: &str) -> Option<u64> {
    let modified = fs::metadata(file_for(lists_dir, id))
        .ok()?
        .modified()
        .ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_secs())
}

pub fn load_all(lists_dir: &Path) -> BTreeMap<&'static str, String> {
    let mut out = BTreeMap::new();
    for source in &SOURCES {
        let Ok(file) = fs::File::open(file_for(lists_dir, source.id)) else {
            continue;
        };
        let Ok(bytes) = read_capped(file, source.max_bytes) else {
            continue;
        };
        if let Ok(text) = String::from_utf8(bytes) {
            out.insert(source.id, text);
        }
    }
    out
}

/// How many usable entries a downloaded list holds. Zero means it is garbage
/// (an error page, an empty file, the wrong format) and must not replace the
/// copy that works.
pub fn usable_entries(source: &Source, text: &str) -> usize {
    match source.role {
        Role::Dns | Role::WindowsTracking => lists::parse_blocklist(text).block.len(),
        Role::Threats => lists::parse_blocklist_hashes(text).0.len(),
        Role::TrackingClassifier | Role::AdClassifier => lists::parse_classifier(text).len(),
    }
}

fn text_of<'a>(lists: &'a BTreeMap<&str, String>, role: Role) -> Vec<&'a str> {
    SOURCES
        .iter()
        .filter(|s| s.role == role)
        .filter_map(|s| lists.get(s.id).map(String::as_str))
        .collect()
}

pub fn rebuild(lists: &BTreeMap<&str, String>) -> Option<Filter> {
    let dns = text_of(lists, Role::Dns).into_iter().next();
    let windows = text_of(lists, Role::WindowsTracking).into_iter().next();
    let threats = text_of(lists, Role::Threats).into_iter().next();
    if dns.is_none() && windows.is_none() && threats.is_none() {
        return None;
    }
    // The threat feed (millions of names) goes in as hashes only.
    let mut filter = lists::build(&Inputs {
        dns,
        windows,
        threats: None,
        tracking_classifiers: text_of(lists, Role::TrackingClassifier),
        ad_classifiers: text_of(lists, Role::AdClassifier),
    });
    if let Some(text) = threats {
        let (block, allow) = lists::parse_blocklist_hashes(text);
        filter.dangerous = Category {
            block: HashSet64::from_hashes(block),
            allow: HashSet64::from_hashes(allow),
        };
    }
    Some(filter)
}

/// A freshly built set replaces the one in use unless a switch that had
/// domains would end up with none (a broken list); then the old set stays.
pub fn accept(candidate: Filter, previous: &Filter) -> Result<Filter> {
    let new = lists::counts(&candidate);
    let old = lists::counts(previous);
    if new.iter().zip(old).any(|(&n, o)| n == 0 && o > 0) {
        bail!("The new lists are missing entries");
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str) -> &'static Source {
        known(id).unwrap()
    }

    #[test]
    fn due_daily_and_weekly() {
        let day = SECONDS_PER_DAY;
        let daily = source("adguard-dns");
        let weekly = source("easylist");
        assert!(due(daily, None, 1_000_000));
        assert!(!due(daily, Some(1_000_000), 1_000_000 + day - 1));
        assert!(due(daily, Some(1_000_000), 1_000_000 + day));
        assert!(!due(weekly, Some(1_000_000), 1_000_000 + 6 * day));
        assert!(due(weekly, Some(1_000_000), 1_000_000 + 7 * day));
        // A clock that moved back does not make everything due.
        assert!(!due(daily, Some(2_000_000), 1_000_000));
    }

    #[test]
    fn download_rejects_oversize() {
        let data = [b'a'; 101];
        assert!(read_capped(&data[..], 100).is_err());
        assert_eq!(read_capped(&data[..100], 100).unwrap().len(), 100);
        assert!(read_capped(&b""[..], 100).unwrap().is_empty());
    }

    #[test]
    fn download_rejects_redirect() {
        assert!(check_status(200).is_ok());
        for code in [204, 301, 302, 307, 404, 500] {
            assert!(check_status(code).is_err(), "{code}");
        }
    }

    #[test]
    fn store_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        store(dir.path(), "adguard-dns", "||ads.example^\n").unwrap();
        assert!(store(dir.path(), "../evil", "x").is_err());
        fs::write(dir.path().join("stray.txt"), "||x.example^").unwrap();
        let all = load_all(dir.path());
        assert_eq!(all.len(), 1);
        assert_eq!(all["adguard-dns"], "||ads.example^\n");
        assert!(stored_at(dir.path(), "adguard-dns").is_some());
        assert!(stored_at(dir.path(), "easylist").is_none());
    }

    #[test]
    fn rebuild_needs_a_blocking_list() {
        let mut lists = BTreeMap::new();
        assert!(rebuild(&lists).is_none());
        lists.insert("easylist", "||ads.example^\n".to_string());
        assert!(rebuild(&lists).is_none());
        lists.insert("hagezi-tif", "||evil.example^\n".to_string());
        let filter = rebuild(&lists).unwrap();
        assert_eq!(lists::counts(&filter), [0, 0, 1]);
    }

    #[test]
    fn rebuild_splits_the_lists_into_switches() {
        let mut lists = BTreeMap::new();
        lists.insert(
            "adguard-dns",
            "||ads.example^\n||spy.example^\n".to_string(),
        );
        lists.insert("easyprivacy", "||spy.example^\n".to_string());
        lists.insert("hagezi-windows", "||telemetry.example^\n".to_string());
        lists.insert(
            "hagezi-tif",
            "||evil.example^\n@@||fine.evil.example^\n".to_string(),
        );
        let filter = rebuild(&lists).unwrap();
        assert!(filter.ads.blocks("ads.example"));
        assert!(!filter.ads.blocks("spy.example"));
        assert!(filter.tracking.blocks("spy.example"));
        assert!(filter.tracking.blocks("telemetry.example"));
        assert!(filter.dangerous.blocks("evil.example"));
        assert!(!filter.dangerous.blocks("fine.evil.example"));
    }

    #[test]
    fn bad_list_keeps_previous_set() {
        let mut good = BTreeMap::new();
        good.insert("adguard-dns", "||ads.example^\n".to_string());
        good.insert("hagezi-tif", "||evil.example^\n".to_string());
        let previous = rebuild(&good).unwrap();

        let mut broken = good.clone();
        broken.insert(
            "adguard-dns",
            "<html>Service unavailable</html>".to_string(),
        );
        let candidate = rebuild(&broken).unwrap();
        assert!(accept(candidate, &previous).is_err());

        let mut updated = good.clone();
        updated.insert(
            "adguard-dns",
            "||ads.example^\n||more.example^\n".to_string(),
        );
        let candidate = rebuild(&updated).unwrap();
        assert_eq!(
            lists::counts(&accept(candidate, &previous).unwrap()),
            [2, 2, 1]
        );
    }

    #[test]
    fn garbage_downloads_have_no_entries() {
        for id in ["adguard-dns", "hagezi-tif", "easylist"] {
            assert_eq!(
                usable_entries(source(id), "<html>Not found</html>"),
                0,
                "{id}"
            );
        }
        assert_eq!(
            usable_entries(source("hagezi-tif"), "||a.example^\n||b.example^\n"),
            2
        );
        assert_eq!(
            usable_entries(source("easylist"), "||a.example^$third-party\n"),
            1
        );
    }
}
