//! `debloat.jsonl`: one line per removal batch, kept in the protected state
//! directory. Used by the Removed apps list and the History timeline.
use super::Batch;
use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const FILE: &str = "debloat.jsonl";
/// Old batches beyond this are dropped so the file stays small.
pub const MAX_BATCHES: usize = 200;

fn default_path() -> Result<PathBuf> {
    Ok(crate::platform::app_dir()?.join(FILE))
}

/// All batches, oldest first. Missing file, unreadable state directory and
/// malformed lines all yield fewer (or no) batches rather than an error.
pub fn load() -> Vec<Batch> {
    default_path().map(|p| load_from(&p)).unwrap_or_default()
}

pub fn append(batch: &Batch) -> Result<()> {
    append_to(&default_path()?, batch)
}

/// Save a run's batch: replaces this run's earlier checkpoint (same start
/// time, newest line) or appends a new line.
pub fn upsert(batch: &Batch) -> Result<()> {
    upsert_to(&default_path()?, batch)
}

/// Mark every removed copy of catalog app `index` as restored.
pub fn mark_restored(index: u16) -> Result<()> {
    mark_restored_in(&default_path()?, index)
}

/// Apps that are removed right now: (catalog index, time of the newest
/// removal), newest batch first. An app removed again after a restore counts
/// once; restored apps and indices outside the catalog are left out.
pub fn still_removed(batches: &[Batch], catalog_len: usize) -> Vec<(u16, u64)> {
    let mut out: Vec<(u16, u64)> = Vec::new();
    for batch in batches.iter().rev() {
        for r in batch.removed.iter().filter(|r| !r.restored) {
            if (r.index as usize) < catalog_len && !out.iter().any(|(i, _)| *i == r.index) {
                out.push((r.index, batch.t));
            }
        }
    }
    out
}

pub fn load_from(path: &Path) -> Vec<Batch> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Batch>(line).ok())
        .collect()
}

pub fn append_to(path: &Path, batch: &Batch) -> Result<()> {
    let mut batches = load_from(path);
    batches.push(batch.clone());
    if batches.len() > MAX_BATCHES {
        let extra = batches.len() - MAX_BATCHES;
        batches.drain(..extra);
    }
    write_all(path, &batches)
}

pub fn upsert_to(path: &Path, batch: &Batch) -> Result<()> {
    let mut batches = load_from(path);
    match batches.last_mut() {
        Some(last) if last.t == batch.t && last.removed.iter().all(|r| !r.restored) => {
            *last = batch.clone();
        }
        _ => batches.push(batch.clone()),
    }
    if batches.len() > MAX_BATCHES {
        let extra = batches.len() - MAX_BATCHES;
        batches.drain(..extra);
    }
    write_all(path, &batches)
}

pub fn mark_restored_in(path: &Path, index: u16) -> Result<()> {
    let mut batches = load_from(path);
    let mut changed = false;
    for item in batches.iter_mut().flat_map(|b| b.removed.iter_mut()) {
        if item.index == index && !item.restored {
            item.restored = true;
            changed = true;
        }
    }
    if changed {
        write_all(path, &batches)?;
    }
    Ok(())
}

/// Write via a temporary file and rename so a crash never truncates history.
fn write_all(path: &Path, batches: &[Batch]) -> Result<()> {
    let tmp = path.with_extension("jsonl.tmp");
    {
        let mut file = std::fs::File::create(&tmp).context("Write the removed-apps list")?;
        for batch in batches {
            serde_json::to_writer(&mut file, batch)?;
            file.write_all(b"\n")?;
        }
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path).context("Save the removed-apps list")?;
    Ok(())
}
