//! A saved file Secblitz cannot read is kept beside the original as `<name>.damaged`, never dropped without a trace. One copy is kept.
use std::path::{Path, PathBuf};

fn copy_path(path: &Path) -> Option<PathBuf> {
    let mut name = path.file_name()?.to_os_string();
    name.push(".damaged");
    Some(path.with_file_name(name))
}

/// Keeps the unreadable file. `take` moves it away, for a file that is replaced by an empty one;
/// otherwise it stays in place and a copy is made. Failures are logged, never raised.
pub fn keep(path: &Path, take: bool) {
    let Some(kept) = copy_path(path) else {
        return;
    };
    let result = if take {
        std::fs::remove_file(&kept)
            .or_else(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(e)
                }
            })
            .and_then(|()| std::fs::rename(path, &kept))
    } else if matches!((std::fs::read(path), std::fs::read(&kept)), (Ok(a), Ok(b)) if a == b) {
        return;
    } else {
        std::fs::copy(path, &kept).map(drop)
    };
    match result {
        Ok(()) => eprintln!(
            "{} could not be read; kept a copy as {}",
            path.display(),
            kept.display()
        ),
        Err(e) => eprintln!(
            "{} could not be read and no copy could be kept: {e}",
            path.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::keep;

    #[test]
    fn taking_moves_the_file_and_keeps_only_the_newest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        std::fs::write(&path, b"one").unwrap();
        keep(&path, true);
        assert!(!path.exists());
        std::fs::write(&path, b"two").unwrap();
        keep(&path, true);
        assert_eq!(
            std::fs::read(dir.path().join("a.json.damaged")).unwrap(),
            b"two"
        );
    }

    #[test]
    fn copying_leaves_the_file_and_skips_an_identical_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.jsonl");
        std::fs::write(&path, b"x\n").unwrap();
        keep(&path, false);
        assert_eq!(std::fs::read(&path).unwrap(), b"x\n");
        let kept = dir.path().join("a.jsonl.damaged");
        assert_eq!(std::fs::read(&kept).unwrap(), b"x\n");
        let first = std::fs::metadata(&kept).unwrap().modified().unwrap();
        keep(&path, false);
        assert_eq!(std::fs::metadata(&kept).unwrap().modified().unwrap(), first);
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        keep(&dir.path().join("none.json"), true);
        keep(&dir.path().join("none.json"), false);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
