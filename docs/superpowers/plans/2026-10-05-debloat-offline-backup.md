# Clean up apps: offline backup and restore — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Before Clean up apps removes an app, keep a verified copy of the app (and each account's app data, encrypted) in Secblitz's protected state, so the app can be brought back any time without internet.

**Architecture:** Pure, host-tested Rust owns the store layout, manifest, validation, archive and encryption framing (`backup.rs`, `vault.rs`, `offline.rs`). Thin Windows-only modules do the privileged file work and crypto (`winfs.rs`, `wincrypto.rs`). PowerShell is limited to two small scripts that ask Windows which packages and accounts exist (`describe.ps1`) and register/provision packages (`register.ps1`). The GUI only gains plain-language copy, a Restore that prefers the saved copy, and "Delete saved copy".

**Tech Stack:** Rust 2021, iced 0.14 GUI, `windows-sys` 0.59 (CNG `BCrypt*`, DPAPI, file security), `sha2`, `rand` (`OsRng`), `serde_json`, PowerShell 5.1 inbox Appx module.

**Spec:** `docs/superpowers/specs/2026-10-05-debloat-offline-backup-design.md` (read it first; it holds the VM probe results this plan relies on).

## Global Constraints

- UI copy: plain words only. No hashes, manifests, paths, byte counts, package names, "DPAPI", "AES" or other technical terms (user: "no dev fluff in the UI").
- Every new UI string goes in `TEXT` in `src/i18n.rs` with all six languages (en, es, fr, de, pt, it); `every_gui_key_has_all_five_translations` must pass.
- No em dashes in UI strings.
- No new crates. Crypto is Windows CNG + DPAPI through `windows-sys` (add feature `Win32_Security_Cryptography` and `Win32_Security_Isolation`).
- Limits (verbatim from spec): at most 50 000 files and 4 GB per backup; free-space headroom 1 GB; AES-256-GCM, random 256-bit key per backup, random 96-bit nonce per 1 MB chunk.
- Store: `<state_dir>\App\AppBackups\` (inside `platform::app_dir()`, so it inherits the protected SYSTEM+Administrators ACL).
- Only Microsoft (`8wekyb3d8bbwe`) frameworks are backed up.
- No console window ever: every child process uses the existing `debloat::windows::run` (CREATE_NO_WINDOW, stdin script).
- Build/test commands (run from repo root, after `source target/build-tools/cross-env.sh`):
  - `cargo test --locked --all-targets --no-fail-fast`
  - `cargo clippy --locked --target x86_64-pc-windows-gnu --all-targets -- -D warnings`
  - `cargo build --locked --release --target x86_64-pc-windows-gnu`
- Commits: `git -c user.email=skeleton22121@gmail.com -c user.name=slay commit`, message ending with the line `Claude-Session: https://claude.ai/code/session_019sBc3UDNnKBXUfzPSA9bDy`.
- rustfmt only files that were rustfmt-clean before you touched them (`rustfmt --edition 2021 --check <file>` on `git show HEAD:<file>`); new files must be rustfmt-clean.
- VM work only on `Secblitz-W11-UI-Test` (4b70288b-b64d-4796-a725-006da3162d0f) with the tooling in `target/windows-validation-tools/` (`guest.py`, `vm.sh`, `run-desk.sh`, `tester-launch.sh`).

## Review Focus

1. **Two installed versions of the same app** (e.g. an update staged for one account): every package of the family must be in the backup and restore must not refuse because one version is already present. Test: `offline::tests::backup_keeps_every_version_of_the_family` (Task 6).
2. **App data folder that is a junction or contains links** planted by a standard user: backup must not read through it, restore must not write through it. Tests: `vault::tests::unpack_refuses_bad_paths` (Task 2) and VM step 5 (Task 8).
3. **Interrupted backup** (crash or power loss mid-copy): a half-written `.staging-*` folder must never be treated as a backup and must be cleaned on next use. Test: `backup::tests::staging_folders_are_never_backups_and_get_cleaned` (Task 1).
4. **Backup taken, then removal fails**: the backup stays, the app stays installed, the journal does not list it as removed, and a later removal replaces the copy. Test: `offline::tests::failed_removal_keeps_app_and_copy` (Task 6).
5. **Restore while the app is already installed** (user reinstalled from the Store): Restore must not overwrite a newer install; it reports the app is already there. Test: `offline::tests::restore_skips_when_already_installed` (Task 6).

---

## File Structure

| File | Responsibility |
|---|---|
| `src/debloat/backup.rs` (new) | Manifest types, package identity parsing, path/identity/manifest validation, tree hashing and verification, store layout, commit, delete, framework GC, size total. Host-testable (all paths injected). |
| `src/debloat/vault.rs` (new) | Plain archive format (pack/unpack through `Source`/`Sink` traits), chunked AEAD framing through a `Sealer` trait. Host-testable with a fake sealer. |
| `src/debloat/wincrypto.rs` (new, Windows) | `Aes` (CNG AES-256-GCM `Sealer`), `Key` (zeroed on drop), DPAPI machine seal/unseal. |
| `src/debloat/winfs.rs` (new, Windows) | Backup/restore privileges, known folders, reparse-safe reader (`TreeSource`), WindowsApps writer with template security, pinned user-data writer (`DataSink`), profile lookup, AppContainer SID. |
| `src/debloat/offline.rs` (new) | Orchestration behind a `Host` trait: `backup_family`, `restore_index`, `delete_index`, `saved_bytes`, `has_copy`. Host-tested with a fake host; `WindowsHost` wires the real modules. |
| `src/debloat/scripts/describe.ps1` (new) | JSON: packages (full name, kind), Microsoft framework dependencies, provisioned flag, account SIDs that have the app, ACL template source. |
| `src/debloat/scripts/register.ps1` (new) | Register manifests in order, optionally provision for all users, report status. |
| `src/debloat/mod.rs` (modify) | `ItemResult::Kept`, `Progress::Saving`, backup step inside `remove_with`/`remove`; `pub mod backup, vault, offline`. |
| `src/debloat/windows.rs` (modify) | Expose `DESCRIBE`, `REGISTER` script constants. |
| `src/gui/pages/debloat.rs` (modify) | Review line, Saving step, Kept result, Restore via saved copy, "Delete saved copy" sheet, saved-size line. |
| `src/i18n.rs` (modify) | New strings in all languages. |
| `Cargo.toml` (modify) | `windows-sys` features `Win32_Security_Cryptography`, `Win32_Security_Isolation`, `Win32_UI_Shell` already present. |

---

### Task 1: Backup store model (`backup.rs`)

**Files:**
- Create: `src/debloat/backup.rs`
- Modify: `src/debloat/mod.rs` (add `pub mod backup;` after `pub mod catalog;`)
- Test: inline `#[cfg(test)] mod tests` in `src/debloat/backup.rs`

**Interfaces:**
- Consumes: `super::catalog::{owner, is_protected}` (`fn owner(&str) -> Option<u16>`, `fn is_protected(&str) -> bool`).
- Produces (used by Tasks 2, 4, 6):
  - `pub enum Kind { Bundle, Main, Resource, Framework }`
  - `pub struct FileEntry { pub path: String, pub size: u64, pub sha256: String }`
  - `pub struct Package { pub full_name: String, pub kind: Kind, pub files: Vec<FileEntry> }`
  - `pub struct DataBlob { pub sid: String, pub plain_size: u64, pub sha256: String }`
  - `pub struct Manifest { pub schema: u32, pub created: u64, pub index: u16, pub family: String, pub packages: Vec<Package>, pub frameworks: Vec<String>, pub data: Vec<DataBlob>, pub provisioned: bool }`
  - `pub struct FrameworkCopy { pub schema: u32, pub full_name: String, pub files: Vec<FileEntry> }`
  - `pub struct Identity { pub name: String, pub version: String, pub arch: String, pub resource: String, pub publisher: String }` with `pub fn family(&self) -> String`
  - `pub fn parse_full_name(&str) -> anyhow::Result<Identity>`
  - `pub fn valid_relative(&str) -> bool`, `pub fn valid_sid(&str) -> bool`
  - `impl Manifest { pub fn check(&self, index: u16) -> anyhow::Result<()> }`
  - `pub fn hash_file(&Path) -> anyhow::Result<(u64, String)>`
  - `pub fn hash_tree(root: &Path) -> anyhow::Result<Vec<FileEntry>>`
  - `pub fn verify_tree(root: &Path, files: &[FileEntry]) -> anyhow::Result<()>`
  - `pub struct Store` with `pub fn at(root: PathBuf) -> Store`, `pub fn open() -> anyhow::Result<Store>`, `pub fn root(&self) -> &Path`, `pub fn family_dir(&self, family: &str) -> PathBuf`, `pub fn framework_dir(&self, full: &str) -> PathBuf`, `pub fn new_staging(&self) -> anyhow::Result<PathBuf>`, `pub fn clean_staging(&self)`, `pub fn load(&self, family: &str, index: u16) -> anyhow::Result<Option<Manifest>>`, `pub fn for_index(&self, index: u16) -> Vec<Manifest>`, `pub fn load_framework(&self, full: &str) -> anyhow::Result<Option<FrameworkCopy>>`, `pub fn commit(&self, staging: &Path, family: &str) -> anyhow::Result<()>`, `pub fn delete(&self, family: &str) -> anyhow::Result<()>`, `pub fn gc_frameworks(&self) -> anyhow::Result<()>`, `pub fn total_bytes(&self) -> u64`
  - constants `DIR`, `MANIFEST`, `FRAMEWORK_MANIFEST`, `SCHEMA`, `MAX_FILES`, `MAX_BYTES`, `MICROSOFT`, `HEADROOM`

- [ ] **Step 1: Write the failing tests**

Create `src/debloat/backup.rs` containing only the test module below plus `use` lines, so it fails to compile against missing items:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn weather() -> u16 {
        crate::debloat::catalog::owner("Microsoft.BingWeather").expect("in catalog")
    }

    fn manifest() -> Manifest {
        Manifest {
            schema: SCHEMA,
            created: 1,
            index: weather(),
            family: "Microsoft.BingWeather_8wekyb3d8bbwe".into(),
            packages: vec![
                Package {
                    full_name: "Microsoft.BingWeather_4.54.63045.0_neutral_~_8wekyb3d8bbwe".into(),
                    kind: Kind::Bundle,
                    files: vec![entry("AppxMetadata/AppxBundleManifest.xml")],
                },
                Package {
                    full_name: "Microsoft.BingWeather_4.54.63045.0_x64__8wekyb3d8bbwe".into(),
                    kind: Kind::Main,
                    files: vec![entry("AppxManifest.xml")],
                },
                Package {
                    full_name: "Microsoft.BingWeather_4.54.63045.0_neutral_split.scale-100_8wekyb3d8bbwe".into(),
                    kind: Kind::Resource,
                    files: vec![entry("AppxManifest.xml")],
                },
            ],
            frameworks: vec!["Microsoft.VCLibs.140.00_14.0.33519.0_x64__8wekyb3d8bbwe".into()],
            data: vec![DataBlob {
                sid: "S-1-5-21-1-2-3-1001".into(),
                plain_size: 10,
                sha256: "a".repeat(64),
            }],
            provisioned: false,
        }
    }

    fn entry(path: &str) -> FileEntry {
        FileEntry { path: path.into(), size: 1, sha256: "0".repeat(64) }
    }

    #[test]
    fn parses_full_names() {
        let id = parse_full_name("Microsoft.BingWeather_4.54.63045.0_x64__8wekyb3d8bbwe").unwrap();
        assert_eq!(id.name, "Microsoft.BingWeather");
        assert_eq!(id.resource, "");
        assert_eq!(id.family(), "Microsoft.BingWeather_8wekyb3d8bbwe");
        assert_eq!(
            parse_full_name("Microsoft.BingWeather_4.54.63045.0_neutral_~_8wekyb3d8bbwe").unwrap().resource,
            "~"
        );
        for bad in [
            "",
            "Microsoft.BingWeather",
            "Microsoft.BingWeather_4.54_x64__8wekyb3d8bbwe",
            "Microsoft.BingWeather_4.54.63045.0_sparc__8wekyb3d8bbwe",
            "Microsoft.BingWeather_4.54.63045.0_x64__8WEKYB3D8BBWE",
            "..\\x_1.0.0.0_x64__8wekyb3d8bbwe",
            "a_1.0.0.0_x64__8wekyb3d8bbwe_extra",
            "Micro soft_1.0.0.0_x64__8wekyb3d8bbwe",
            "Microsoft.BingWeather_70000.0.0.0_x64__8wekyb3d8bbwe",
        ] {
            assert!(parse_full_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn relative_paths_are_strict() {
        for good in ["AppxManifest.xml", "Assets/Logo.png", "a/b/c.dll", "resources.pri"] {
            assert!(valid_relative(good), "{good}");
        }
        for bad in [
            "", "/abs", "C:/x", "a/../b", "..", "./a", "a//b", "a\\b", "a:stream", "a/b ",
            "a/b.", "con\u{0}", "a/*", "a/<b>", "a/?", &"x".repeat(300),
        ] {
            assert!(!valid_relative(bad), "{bad:?}");
        }
        let deep = vec!["d"; 40].join("/");
        assert!(!valid_relative(&deep));
    }

    #[test]
    fn sids_are_user_accounts_only() {
        assert!(valid_sid("S-1-5-21-1-2-3-1001"));
        assert!(valid_sid("S-1-5-21-3623811015-3361044348-30300820-1013"));
        for bad in ["S-1-5-18", "S-1-5-32-544", "S-1-5-21-1-2-3", "S-1-5-21-1-2-3-x", "", "S-1-5-21-1-2-3-4-5"] {
            assert!(!valid_sid(bad), "{bad}");
        }
    }

    #[test]
    fn manifest_check_accepts_good_and_rejects_tampering() {
        let index = weather();
        manifest().check(index).unwrap();
        let other = crate::debloat::catalog::owner("Microsoft.ZuneMusic").expect("in catalog");
        assert!(manifest().check(other).is_err(), "wrong catalog entry");

        let mut m = manifest();
        m.schema = 99;
        assert!(m.check(index).is_err());

        let mut m = manifest();
        m.family = "Microsoft.WindowsStore_8wekyb3d8bbwe".into();
        assert!(m.check(index).is_err(), "protected or foreign family");

        let mut m = manifest();
        m.packages[1].full_name = "Microsoft.ZuneMusic_1.0.0.0_x64__8wekyb3d8bbwe".into();
        assert!(m.check(index).is_err(), "package outside the family");

        let mut m = manifest();
        m.packages.retain(|p| p.kind != Kind::Main);
        assert!(m.check(index).is_err(), "no main package");

        let mut m = manifest();
        m.packages[1].files[0].path = "../evil.dll".into();
        assert!(m.check(index).is_err());

        let mut m = manifest();
        m.packages[1].files[0].sha256 = "xyz".into();
        assert!(m.check(index).is_err());

        let mut m = manifest();
        m.frameworks = vec!["Contoso.Runtime_1.0.0.0_x64__abcdefghijklm".into()];
        assert!(m.check(index).is_err(), "only Microsoft frameworks");

        let mut m = manifest();
        m.data[0].sid = "S-1-5-18".into();
        assert!(m.check(index).is_err());

        let mut m = manifest();
        m.packages[1].kind = Kind::Resource;
        assert!(m.check(index).is_err(), "kind must match the resource id");

        let mut m = manifest();
        m.packages[0].files = (0..MAX_FILES + 1).map(|i| entry(&format!("f{i}"))).collect();
        assert!(m.check(index).is_err(), "file cap");
    }

    #[test]
    fn hash_and_verify_tree() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b")).unwrap();
        fs::write(dir.path().join("a/b/x.bin"), b"hello").unwrap();
        fs::write(dir.path().join("top.txt"), b"").unwrap();
        let files = hash_tree(dir.path()).unwrap();
        assert_eq!(
            files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            vec!["a/b/x.bin", "top.txt"]
        );
        assert_eq!(files[0].size, 5);
        assert_eq!(
            files[0].sha256,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        verify_tree(dir.path(), &files).unwrap();

        fs::write(dir.path().join("a/b/x.bin"), b"hellO").unwrap();
        assert!(verify_tree(dir.path(), &files).is_err(), "changed byte");
        fs::write(dir.path().join("a/b/x.bin"), b"hello").unwrap();
        fs::write(dir.path().join("extra"), b"1").unwrap();
        assert!(verify_tree(dir.path(), &files).is_err(), "extra file");
        fs::remove_file(dir.path().join("extra")).unwrap();
        fs::remove_file(dir.path().join("top.txt")).unwrap();
        assert!(verify_tree(dir.path(), &files).is_err(), "missing file");
    }

    #[cfg(unix)]
    #[test]
    fn hash_tree_refuses_links() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("real"), b"1").unwrap();
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("link")).unwrap();
        assert!(hash_tree(dir.path()).is_err());
    }

    #[test]
    fn store_commit_load_delete_and_gc() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join(DIR));
        let index = weather();
        let m = manifest();

        let staging = store.new_staging().unwrap();
        fs::write(staging.join(MANIFEST), serde_json::to_vec(&m).unwrap()).unwrap();
        store.commit(&staging, &m.family).unwrap();
        assert!(!staging.exists());
        assert_eq!(store.load(&m.family, index).unwrap(), Some(m.clone()));
        assert_eq!(store.for_index(index), vec![m.clone()]);

        // A framework copy referenced by the manifest survives GC, an
        // unreferenced one is removed.
        let used = store.framework_dir(&m.frameworks[0]);
        let unused = store.framework_dir("Microsoft.UI.Xaml.2.8_8.2511.26001.0_x64__8wekyb3d8bbwe");
        fs::create_dir_all(&used).unwrap();
        fs::create_dir_all(&unused).unwrap();
        store.gc_frameworks().unwrap();
        assert!(used.exists() && !unused.exists());

        // Replacing keeps exactly one copy.
        let staging = store.new_staging().unwrap();
        let mut newer = m.clone();
        newer.created = 2;
        fs::write(staging.join(MANIFEST), serde_json::to_vec(&newer).unwrap()).unwrap();
        store.commit(&staging, &m.family).unwrap();
        assert_eq!(store.load(&m.family, index).unwrap().unwrap().created, 2);

        store.delete(&m.family).unwrap();
        assert_eq!(store.load(&m.family, index).unwrap(), None);
        store.gc_frameworks().unwrap();
        assert!(!used.exists(), "last reference gone");
    }

    #[test]
    fn load_rejects_a_manifest_for_another_folder() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join(DIR));
        let m = manifest();
        let folder = store.family_dir("Microsoft.BingNews_8wekyb3d8bbwe");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(MANIFEST), serde_json::to_vec(&m).unwrap()).unwrap();
        let news = crate::debloat::catalog::owner("Microsoft.BingNews").expect("in catalog");
        assert!(store.load("Microsoft.BingNews_8wekyb3d8bbwe", news).is_err());
    }

    #[test]
    fn staging_folders_are_never_backups_and_get_cleaned() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join(DIR));
        let staging = store.new_staging().unwrap();
        fs::write(staging.join(MANIFEST), serde_json::to_vec(&manifest()).unwrap()).unwrap();
        assert!(store.for_index(weather()).is_empty());
        store.clean_staging();
        assert!(!staging.exists());
    }

    #[test]
    fn total_bytes_counts_everything() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join(DIR));
        let a = store.family_dir("Microsoft.BingWeather_8wekyb3d8bbwe");
        fs::create_dir_all(a.join("packages")).unwrap();
        fs::write(a.join("packages/x"), vec![0u8; 1000]).unwrap();
        let f = store.framework_dir("Microsoft.VCLibs.140.00_14.0.33519.0_x64__8wekyb3d8bbwe");
        fs::create_dir_all(&f).unwrap();
        fs::write(f.join("y"), vec![0u8; 24]).unwrap();
        assert_eq!(store.total_bytes(), 1024);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --locked --lib debloat::backup 2>&1 | tail -5`
Expected: compile errors (`cannot find type Manifest`, etc.).

- [ ] **Step 3: Implement**

Put this above the test module in `src/debloat/backup.rs`:

```rust
//! Saved copies of removed apps. Kept in the protected state directory so a
//! removed app can be brought back without internet. This module owns the
//! layout, the manifest and every check; it never runs PowerShell and never
//! touches WindowsApps.
use anyhow::{ensure, Context, Result};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const DIR: &str = "AppBackups";
pub const MANIFEST: &str = "backup.json";
pub const FRAMEWORK_MANIFEST: &str = "framework.json";
pub const FRAMEWORKS: &str = "frameworks";
pub const PACKAGES: &str = "packages";
pub const DATA: &str = "data";
pub const KEY: &str = "key.bin";
pub const SCHEMA: u32 = 1;
pub const MAX_FILES: usize = 50_000;
pub const MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// Free space kept on top of the measured size before saving a copy.
pub const HEADROOM: u64 = 1024 * 1024 * 1024;
/// Publisher id of Microsoft packages. Only Microsoft frameworks are kept.
pub const MICROSOFT: &str = "8wekyb3d8bbwe";
const MAX_PATH_CHARS: usize = 240;
const MAX_DEPTH: usize = 32;
const MAX_MANIFEST: u64 = 64 * 1024 * 1024;
const STAGING: &str = ".staging-";
const OLD: &str = ".old-";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Bundle,
    Main,
    Resource,
    Framework,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Relative, '/'-separated.
    pub path: String,
    pub size: u64,
    /// Lowercase hex SHA-256.
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Package {
    pub full_name: String,
    pub kind: Kind,
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataBlob {
    pub sid: String,
    pub plain_size: u64,
    /// SHA-256 of the encrypted file `data\<sid>.bin`.
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub created: u64,
    pub index: u16,
    pub family: String,
    pub packages: Vec<Package>,
    /// Full names of shared framework copies this app needs.
    pub frameworks: Vec<String>,
    pub data: Vec<DataBlob>,
    pub provisioned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkCopy {
    pub schema: u32,
    pub full_name: String,
    pub files: Vec<FileEntry>,
}

/// `Name_Version_Architecture_ResourceId_PublisherId`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub version: String,
    pub arch: String,
    pub resource: String,
    pub publisher: String,
}

impl Identity {
    pub fn family(&self) -> String {
        format!("{}_{}", self.name, self.publisher)
    }
}

fn plain_token(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
}

pub fn parse_full_name(full: &str) -> Result<Identity> {
    let parts: Vec<&str> = full.split('_').collect();
    ensure!(parts.len() == 5, "Unexpected package name");
    let (name, version, arch, resource, publisher) =
        (parts[0], parts[1], parts[2], parts[3], parts[4]);
    ensure!(plain_token(name, 50) && name.len() >= 3, "Unexpected package name");
    let numbers: Vec<&str> = version.split('.').collect();
    ensure!(
        numbers.len() == 4 && numbers.iter().all(|n| n.parse::<u16>().is_ok()),
        "Unexpected package version"
    );
    ensure!(
        matches!(arch, "x86" | "x64" | "arm" | "arm64" | "neutral"),
        "Unexpected architecture"
    );
    ensure!(
        resource.is_empty() || resource == "~" || plain_token(resource, 30),
        "Unexpected resource id"
    );
    ensure!(
        publisher.len() == 13
            && publisher
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
        "Unexpected publisher id"
    );
    Ok(Identity {
        name: name.into(),
        version: version.into(),
        arch: arch.into(),
        resource: resource.into(),
        publisher: publisher.into(),
    })
}

/// A '/'-separated relative path that can't escape its root on Windows.
pub fn valid_relative(path: &str) -> bool {
    if path.is_empty() || path.chars().count() > MAX_PATH_CHARS {
        return false;
    }
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() <= MAX_DEPTH
        && parts.iter().all(|p| {
            !p.is_empty()
                && *p != "."
                && *p != ".."
                && !p.ends_with('.')
                && !p.ends_with(' ')
                && p.chars().all(|c| {
                    !c.is_control() && !matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
                })
        })
}

/// A local or domain user account (`S-1-5-21-a-b-c-rid`).
pub fn valid_sid(sid: &str) -> bool {
    let Some(rest) = sid.strip_prefix("S-1-5-21-") else {
        return false;
    };
    let parts: Vec<&str> = rest.split('-').collect();
    parts.len() == 4 && parts.iter().all(|p| p.parse::<u32>().is_ok())
}

fn valid_hash(h: &str) -> bool {
    h.len() == 64 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn check_files(files: &[FileEntry], count: &mut usize, bytes: &mut u64) -> Result<()> {
    for f in files {
        ensure!(valid_relative(&f.path), "Unexpected file in saved copy");
        ensure!(valid_hash(&f.sha256), "Unexpected file check");
        *count += 1;
        *bytes = bytes.saturating_add(f.size);
    }
    ensure!(*count <= MAX_FILES && *bytes <= MAX_BYTES, "Saved copy too large");
    Ok(())
}

impl Manifest {
    /// Everything restore relies on, re-derived from the compiled catalog.
    pub fn check(&self, index: u16) -> Result<()> {
        ensure!(self.schema == SCHEMA, "Unknown saved copy version");
        ensure!(self.index == index, "Saved copy belongs to another app");
        let (name, publisher) = self
            .family
            .rsplit_once('_')
            .context("Unexpected family")?;
        ensure!(
            super::catalog::owner(name) == Some(index) && !super::catalog::is_protected(name),
            "Saved copy belongs to another app"
        );
        ensure!(publisher.len() == 13, "Unexpected family");
        ensure!(
            self.packages.iter().any(|p| p.kind == Kind::Main),
            "Saved copy has no app"
        );
        let (mut count, mut bytes) = (0usize, 0u64);
        for p in &self.packages {
            let id = parse_full_name(&p.full_name)?;
            ensure!(id.family() == self.family, "Package outside the family");
            let kind_ok = match p.kind {
                Kind::Bundle => id.resource == "~",
                Kind::Main => id.resource.is_empty(),
                Kind::Resource => !id.resource.is_empty() && id.resource != "~",
                Kind::Framework => false,
            };
            ensure!(kind_ok, "Unexpected package kind");
            check_files(&p.files, &mut count, &mut bytes)?;
        }
        for f in &self.frameworks {
            let id = parse_full_name(f)?;
            ensure!(
                id.publisher == MICROSOFT && id.resource.is_empty(),
                "Unexpected framework"
            );
        }
        for d in &self.data {
            ensure!(valid_sid(&d.sid) && valid_hash(&d.sha256), "Unexpected saved data");
        }
        Ok(())
    }
}

impl FrameworkCopy {
    pub fn check(&self, full: &str) -> Result<()> {
        ensure!(self.schema == SCHEMA && self.full_name == full, "Unexpected framework copy");
        let id = parse_full_name(full)?;
        ensure!(id.publisher == MICROSOFT, "Unexpected framework");
        check_files(&self.files, &mut 0, &mut 0)
    }
}

/// Refuse links of any kind: symlinks, junctions and other reparse points.
fn plain(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE: u32 = 0x400;
        if meta.file_attributes() & REPARSE != 0 {
            return false;
        }
    }
    true
}

pub fn hash_file(path: &Path) -> Result<(u64, String)> {
    let mut file = fs::File::open(path).with_context(|| format!("Read {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut size = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    Ok((size, hex(&hasher.finalize())))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Every file under `root`, sorted by path. Fails on any link.
pub fn hash_tree(root: &Path) -> Result<Vec<FileEntry>> {
    fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<FileEntry>, bytes: &mut u64) -> Result<()> {
        ensure!(depth <= MAX_DEPTH, "Folder too deep");
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let meta = fs::symlink_metadata(&path)?;
            ensure!(plain(&meta), "Link inside saved copy");
            if meta.is_dir() {
                walk(root, &path, depth + 1, out, bytes)?;
            } else {
                let rel = path
                    .strip_prefix(root)?
                    .components()
                    .map(|c| c.as_os_str().to_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
                    .context("Non-text file name")?
                    .join("/");
                ensure!(valid_relative(&rel), "Unexpected file name");
                let (size, sha256) = hash_file(&path)?;
                *bytes += size;
                out.push(FileEntry { path: rel, size, sha256 });
                ensure!(out.len() <= MAX_FILES && *bytes <= MAX_BYTES, "Saved copy too large");
            }
        }
        Ok(())
    }
    let meta = fs::symlink_metadata(root)?;
    ensure!(meta.is_dir() && plain(&meta), "Not a plain folder");
    let mut out = Vec::new();
    walk(root, root, 0, &mut out, &mut 0)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// The folder holds exactly `files`, byte for byte.
pub fn verify_tree(root: &Path, files: &[FileEntry]) -> Result<()> {
    let found = hash_tree(root)?;
    let mut want = files.to_vec();
    want.sort_by(|a, b| a.path.cmp(&b.path));
    ensure!(found == want, "Saved copy changed");
    Ok(())
}

pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn at(root: PathBuf) -> Store {
        Store { root }
    }

    /// `<state>\App\AppBackups`, created on first use inside the protected
    /// directory (it inherits the SYSTEM+Administrators ACL).
    pub fn open() -> Result<Store> {
        let root = crate::platform::app_dir()?.join(DIR);
        match fs::symlink_metadata(&root) {
            Ok(m) => ensure!(m.is_dir() && plain(&m), "Saved copies folder is not plain"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&root)?,
            Err(e) => return Err(e.into()),
        }
        Ok(Store { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn family_dir(&self, family: &str) -> PathBuf {
        self.root.join(family)
    }

    pub fn framework_dir(&self, full: &str) -> PathBuf {
        self.root.join(FRAMEWORKS).join(full)
    }

    fn ensure_root(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }

    pub fn new_staging(&self) -> Result<PathBuf> {
        self.ensure_root()?;
        let mut tag = [0u8; 8];
        rand::rngs::OsRng.fill_bytes(&mut tag);
        let dir = self.root.join(format!("{STAGING}{}", hex(&tag)));
        fs::create_dir(&dir)?;
        Ok(dir)
    }

    /// Remove leftovers of an interrupted save or replace.
    pub fn clean_staging(&self) {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with(STAGING) || name.starts_with(OLD) {
                let _ = fs::remove_dir_all(e.path());
            }
        }
    }

    fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
        let meta = match fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        ensure!(meta.is_file() && plain(&meta) && meta.len() <= MAX_MANIFEST, "Unexpected saved copy");
        Ok(Some(serde_json::from_slice(&fs::read(path)?).context("Read saved copy")?))
    }

    /// The checked manifest stored under `family`, if any.
    pub fn load(&self, family: &str, index: u16) -> Result<Option<Manifest>> {
        let Some(m) = Self::read_json::<Manifest>(&self.family_dir(family).join(MANIFEST))? else {
            return Ok(None);
        };
        ensure!(m.family == family, "Saved copy is in the wrong place");
        m.check(index)?;
        Ok(Some(m))
    }

    /// Every valid saved copy for catalog entry `index` (wildcard entries
    /// can own several families). Damaged ones are skipped here; restore
    /// reports them by loading the family directly.
    pub fn for_index(&self, index: u16) -> Vec<Manifest> {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut out: Vec<Manifest> = entries
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .filter(|n| !n.starts_with('.') && n != FRAMEWORKS)
            .filter_map(|family| self.load(&family, index).ok().flatten())
            .collect();
        out.sort_by(|a, b| a.family.cmp(&b.family));
        out
    }

    pub fn load_framework(&self, full: &str) -> Result<Option<FrameworkCopy>> {
        let Some(f) = Self::read_json::<FrameworkCopy>(&self.framework_dir(full).join(FRAMEWORK_MANIFEST))? else {
            return Ok(None);
        };
        f.check(full)?;
        Ok(Some(f))
    }

    /// Atomically make `staging` the saved copy for `family`.
    pub fn commit(&self, staging: &Path, family: &str) -> Result<()> {
        ensure!(staging.parent() == Some(self.root.as_path()), "Unexpected staging folder");
        let target = self.family_dir(family);
        let mut tag = [0u8; 8];
        rand::rngs::OsRng.fill_bytes(&mut tag);
        let old = self.root.join(format!("{OLD}{}", hex(&tag)));
        let had_old = target.exists();
        if had_old {
            fs::rename(&target, &old)?;
        }
        if let Err(e) = fs::rename(staging, &target) {
            if had_old {
                let _ = fs::rename(&old, &target);
            }
            return Err(e.into());
        }
        if had_old {
            let _ = fs::remove_dir_all(&old);
        }
        Ok(())
    }

    pub fn delete(&self, family: &str) -> Result<()> {
        ensure!(
            !family.is_empty() && !family.starts_with('.') && family != FRAMEWORKS && !family.contains(['/', '\\']),
            "Unexpected family"
        );
        let dir = self.family_dir(family);
        if dir.exists() {
            fs::remove_dir_all(&dir).context("Delete saved copy")?;
        }
        Ok(())
    }

    /// Remove framework copies no saved app needs any more.
    pub fn gc_frameworks(&self) -> Result<()> {
        let dir = self.root.join(FRAMEWORKS);
        let Ok(entries) = fs::read_dir(&dir) else {
            return Ok(());
        };
        let mut used = std::collections::BTreeSet::new();
        if let Ok(apps) = fs::read_dir(&self.root) {
            for app in apps.flatten() {
                let path = app.path().join(MANIFEST);
                if let Ok(Some(m)) = Self::read_json::<Manifest>(&path) {
                    used.extend(m.frameworks);
                }
            }
        }
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !used.contains(&name) {
                fs::remove_dir_all(e.path())?;
            }
        }
        Ok(())
    }

    /// Bytes used by all saved copies (best effort; links are not followed).
    pub fn total_bytes(&self) -> u64 {
        fn size(path: &Path) -> u64 {
            let Ok(meta) = fs::symlink_metadata(path) else {
                return 0;
            };
            if !plain(&meta) {
                return 0;
            }
            if meta.is_dir() {
                fs::read_dir(path)
                    .map(|it| it.flatten().map(|e| size(&e.path())).sum())
                    .unwrap_or(0)
            } else {
                meta.len()
            }
        }
        size(&self.root)
    }
}

```

Also add to `src/debloat/mod.rs`:

```rust
pub mod backup;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --locked --lib debloat::backup`
Expected: all tests in `debloat::backup::tests` PASS. Then `cargo clippy --locked --target x86_64-pc-windows-gnu --all-targets -- -D warnings` clean.

- [ ] **Step 5: Commit**

```bash
git add src/debloat/backup.rs src/debloat/mod.rs
git -c user.email=skeleton22121@gmail.com -c user.name=slay commit -m "Clean up apps: saved-copy store model and checks

Claude-Session: https://claude.ai/code/session_019sBc3UDNnKBXUfzPSA9bDy"
```

---

### Task 2: Archive and encryption framing (`vault.rs`)

**Files:**
- Create: `src/debloat/vault.rs`
- Modify: `src/debloat/mod.rs` (add `pub mod vault;`)
- Test: inline tests in `src/debloat/vault.rs`

**Interfaces:**
- Consumes: `backup::valid_relative`, `backup::MAX_FILES`, `backup::MAX_BYTES`.
- Produces (used by Tasks 3, 4, 6):
  - `pub trait Sealer { fn seal(&self, nonce: &[u8; 12], aad: &[u8], plain: &[u8]) -> anyhow::Result<Vec<u8>>; fn open(&self, nonce: &[u8; 12], aad: &[u8], sealed: &[u8]) -> anyhow::Result<Vec<u8>>; }` (sealed = ciphertext followed by a 16-byte tag)
  - `pub enum Item { Dir(String), File(String, u64) }`
  - `pub trait Source { fn items(&mut self) -> anyhow::Result<Vec<Item>>; fn read(&mut self, rel: &str, out: &mut dyn std::io::Write) -> anyhow::Result<u64>; }`
  - `pub trait Sink { fn dir(&mut self, rel: &str) -> anyhow::Result<()>; fn file(&mut self, rel: &str, size: u64, data: &mut dyn std::io::Read) -> anyhow::Result<()>; }`
  - `pub fn encrypt(source: &mut dyn Source, sealer: &dyn Sealer, family: &str, sid: &str, out: &mut dyn std::io::Write) -> anyhow::Result<u64>` (returns plaintext archive size)
  - `pub fn decrypt(input: &mut dyn std::io::Read, sealer: &dyn Sealer, family: &str, sid: &str, sink: &mut dyn Sink) -> anyhow::Result<()>`
  - `pub const CHUNK: usize = 1 << 20;`
  - `pub struct DirSource` / `pub struct DirSink` (plain std::fs, host tests and non-Windows only: `#[cfg(test)]`)

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Deterministic stand-in for AES-GCM: XOR keystream from SHA-256 and a
    /// 16-byte tag over nonce, aad and ciphertext. Enough to test framing.
    struct Fake([u8; 32]);
    impl Fake {
        fn stream(&self, nonce: &[u8; 12], len: usize) -> Vec<u8> {
            let mut out = Vec::with_capacity(len);
            let mut counter = 0u64;
            while out.len() < len {
                let mut h = Sha256::new();
                h.update(self.0);
                h.update(nonce);
                h.update(counter.to_le_bytes());
                out.extend_from_slice(&h.finalize());
                counter += 1;
            }
            out.truncate(len);
            out
        }
        fn tag(&self, nonce: &[u8; 12], aad: &[u8], ct: &[u8]) -> [u8; 16] {
            let mut h = Sha256::new();
            h.update(self.0);
            h.update(nonce);
            h.update((aad.len() as u64).to_le_bytes());
            h.update(aad);
            h.update(ct);
            h.finalize()[..16].try_into().unwrap()
        }
    }
    impl Sealer for Fake {
        fn seal(&self, nonce: &[u8; 12], aad: &[u8], plain: &[u8]) -> Result<Vec<u8>> {
            let mut ct: Vec<u8> = plain.iter().zip(self.stream(nonce, plain.len())).map(|(a, b)| a ^ b).collect();
            let tag = self.tag(nonce, aad, &ct);
            ct.extend_from_slice(&tag);
            Ok(ct)
        }
        fn open(&self, nonce: &[u8; 12], aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>> {
            ensure!(sealed.len() >= 16, "short");
            let (ct, tag) = sealed.split_at(sealed.len() - 16);
            ensure!(self.tag(nonce, aad, ct) == tag, "tag");
            Ok(ct.iter().zip(self.stream(nonce, ct.len())).map(|(a, b)| a ^ b).collect())
        }
    }

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("LocalState/sub")).unwrap();
        fs::create_dir_all(dir.path().join("Settings")).unwrap();
        fs::create_dir_all(dir.path().join("Empty")).unwrap();
        fs::write(dir.path().join("LocalState/sub/notes.txt"), b"my notes").unwrap();
        fs::write(dir.path().join("Settings/settings.dat"), vec![7u8; 3 * CHUNK + 5]).unwrap();
        dir
    }

    fn roundtrip(sealer: &dyn Sealer) -> (Vec<u8>, tempfile::TempDir) {
        let src = tree();
        let mut out = Vec::new();
        let plain = encrypt(&mut DirSource::new(src.path()), sealer, "Fam_8wekyb3d8bbwe", "S-1-5-21-1-2-3-1001", &mut out).unwrap();
        assert!(plain > 3 * CHUNK as u64);
        let dst = tempfile::tempdir().unwrap();
        decrypt(&mut &out[..], sealer, "Fam_8wekyb3d8bbwe", "S-1-5-21-1-2-3-1001", &mut DirSink::new(dst.path())).unwrap();
        (out, dst)
    }

    #[test]
    fn roundtrip_restores_every_file_and_folder() {
        let (_, dst) = roundtrip(&Fake([1; 32]));
        assert_eq!(fs::read(dst.path().join("LocalState/sub/notes.txt")).unwrap(), b"my notes");
        assert_eq!(fs::read(dst.path().join("Settings/settings.dat")).unwrap().len(), 3 * CHUNK + 5);
        assert!(dst.path().join("Empty").is_dir());
    }

    #[test]
    fn ciphertext_hides_content() {
        let (out, _) = roundtrip(&Fake([1; 32]));
        assert!(!out.windows(8).any(|w| w == b"my notes"));
    }

    #[test]
    fn wrong_key_family_or_account_fails() {
        let (out, _) = roundtrip(&Fake([1; 32]));
        let dst = tempfile::tempdir().unwrap();
        for (key, fam, sid) in [
            ([2u8; 32], "Fam_8wekyb3d8bbwe", "S-1-5-21-1-2-3-1001"),
            ([1u8; 32], "Other_8wekyb3d8bbwe", "S-1-5-21-1-2-3-1001"),
            ([1u8; 32], "Fam_8wekyb3d8bbwe", "S-1-5-21-1-2-3-1002"),
        ] {
            assert!(decrypt(&mut &out[..], &Fake(key), fam, sid, &mut DirSink::new(dst.path())).is_err());
        }
    }

    #[test]
    fn truncation_reordering_and_flips_are_detected() {
        let (out, _) = roundtrip(&Fake([1; 32]));
        let dst = tempfile::tempdir().unwrap();
        let open = |bytes: &[u8]| {
            decrypt(&mut &bytes[..], &Fake([1; 32]), "Fam_8wekyb3d8bbwe", "S-1-5-21-1-2-3-1001", &mut DirSink::new(dst.path()))
        };
        // Drop the last frame: the final-chunk flag is missing.
        let frame = 1 + 12 + 4 + CHUNK + 16;
        let without_last = &out[..MAGIC.len() + 3 * frame];
        assert!(open(without_last).is_err());
        // Swap the first two frames.
        let mut swapped = out.clone();
        let (a, b) = (MAGIC.len(), MAGIC.len() + frame);
        let first = out[a..a + frame].to_vec();
        swapped[a..a + frame].copy_from_slice(&out[b..b + frame]);
        swapped[b..b + frame].copy_from_slice(&first);
        assert!(open(&swapped).is_err());
        // Flip one byte.
        let mut flipped = out.clone();
        flipped[MAGIC.len() + 40] ^= 1;
        assert!(open(&flipped).is_err());
        // Trailing junk after the final frame.
        let mut junk = out.clone();
        junk.push(0);
        assert!(open(&junk).is_err());
    }

    /// A hand-made archive with an escaping path must be refused before
    /// anything is written.
    #[test]
    fn unpack_refuses_bad_paths() {
        for bad in ["../evil", "a/../../evil", "C:/Windows/x", "a\\b", "a:ads"] {
            let mut plain = Vec::new();
            plain.push(b'F');
            plain.extend_from_slice(&(bad.len() as u16).to_le_bytes());
            plain.extend_from_slice(bad.as_bytes());
            plain.extend_from_slice(&1u64.to_le_bytes());
            plain.push(b'x');
            plain.push(b'E');
            let sealer = Fake([1; 32]);
            let mut out = Vec::new();
            write_frames(&plain, &sealer, "Fam_8wekyb3d8bbwe", "S-1-5-21-1-2-3-1001", &mut out).unwrap();
            let dst = tempfile::tempdir().unwrap();
            assert!(decrypt(&mut &out[..], &sealer, "Fam_8wekyb3d8bbwe", "S-1-5-21-1-2-3-1001", &mut DirSink::new(dst.path())).is_err(), "{bad}");
            assert_eq!(fs::read_dir(dst.path()).unwrap().count(), 0, "{bad}");
        }
    }

    #[test]
    fn empty_folder_roundtrips() {
        let src = tempfile::tempdir().unwrap();
        let sealer = Fake([3; 32]);
        let mut out = Vec::new();
        encrypt(&mut DirSource::new(src.path()), &sealer, "F_8wekyb3d8bbwe", "S-1-5-21-1-2-3-1001", &mut out).unwrap();
        let dst = tempfile::tempdir().unwrap();
        decrypt(&mut &out[..], &sealer, "F_8wekyb3d8bbwe", "S-1-5-21-1-2-3-1001", &mut DirSink::new(dst.path())).unwrap();
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --locked --lib debloat::vault 2>&1 | tail -5`
Expected: compile errors.

- [ ] **Step 3: Implement**

```rust
//! App data in a saved copy: a plain archive of one account's app data
//! folder, encrypted in 1 MB chunks with an AEAD (`Sealer`).
//!
//! Frame: `last: u8 | nonce: [u8; 12] | len: u32 LE | sealed: [u8; len]`.
//! Associated data binds the family, the account and the chunk position,
//! and marks the final chunk, so frames can't be swapped between files,
//! reordered, dropped or extended.
//!
//! Archive (plaintext): `'D' len:u16 path` | `'F' len:u16 path size:u64 bytes`
//! | `'E'`. Paths are '/'-separated and checked with `backup::valid_relative`.
use super::backup::{valid_relative, MAX_BYTES, MAX_FILES};
use anyhow::{bail, ensure, Result};
use rand::RngCore;
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

pub const CHUNK: usize = 1 << 20;
pub(crate) const MAGIC: &[u8; 8] = b"SBXDATA1";
const TAG: usize = 16;

pub trait Sealer {
    fn seal(&self, nonce: &[u8; 12], aad: &[u8], plain: &[u8]) -> Result<Vec<u8>>;
    fn open(&self, nonce: &[u8; 12], aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>>;
}

pub enum Item {
    Dir(String),
    File(String, u64),
}

pub trait Source {
    /// Every folder and file to save, parents before children.
    fn items(&mut self) -> Result<Vec<Item>>;
    /// Copy one file into `out`; returns bytes written.
    fn read(&mut self, rel: &str, out: &mut dyn Write) -> Result<u64>;
}

pub trait Sink {
    fn dir(&mut self, rel: &str) -> Result<()>;
    fn file(&mut self, rel: &str, size: u64, data: &mut dyn Read) -> Result<()>;
}

fn aad(family: &str, sid: &str, index: u64, last: bool) -> Vec<u8> {
    let mut a = b"secblitz-appdata-v1\0".to_vec();
    a.extend_from_slice(family.as_bytes());
    a.push(0);
    a.extend_from_slice(sid.as_bytes());
    a.push(0);
    a.extend_from_slice(&index.to_le_bytes());
    a.push(last as u8);
    a
}

/// Buffers plaintext and writes sealed frames.
struct FrameWriter<'a> {
    sealer: &'a dyn Sealer,
    family: &'a str,
    sid: &'a str,
    out: &'a mut dyn Write,
    buf: Vec<u8>,
    index: u64,
    total: u64,
}

impl FrameWriter<'_> {
    fn frame(&mut self, last: bool) -> Result<()> {
        let mut nonce = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let sealed = self.sealer.seal(&nonce, &aad(self.family, self.sid, self.index, last), &self.buf)?;
        ensure!(sealed.len() == self.buf.len() + TAG, "Unexpected sealed size");
        self.out.write_all(&[last as u8])?;
        self.out.write_all(&nonce)?;
        self.out.write_all(&(sealed.len() as u32).to_le_bytes())?;
        self.out.write_all(&sealed)?;
        self.buf.clear();
        self.index += 1;
        Ok(())
    }
    fn finish(mut self) -> Result<u64> {
        self.frame(true)?;
        Ok(self.total)
    }
}

impl Write for FrameWriter<'_> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }
        if self.buf.len() == CHUNK {
            // More data follows a full chunk: it is not the last one.
            self.frame(false).map_err(std::io::Error::other)?;
        }
        let n = (CHUNK - self.buf.len()).min(data.len());
        self.buf.extend_from_slice(&data[..n]);
        self.total += n as u64;
        if self.total > MAX_BYTES {
            return Err(std::io::Error::other("Saved data too large"));
        }
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Seal an already-built plaintext (used by tests to build hostile archives).
#[cfg(test)]
pub(crate) fn write_frames(plain: &[u8], sealer: &dyn Sealer, family: &str, sid: &str, out: &mut dyn Write) -> Result<()> {
    out.write_all(MAGIC)?;
    let mut w = FrameWriter { sealer, family, sid, out, buf: Vec::new(), index: 0, total: 0 };
    w.write_all(plain)?;
    w.finish()?;
    Ok(())
}

pub fn encrypt(source: &mut dyn Source, sealer: &dyn Sealer, family: &str, sid: &str, out: &mut dyn Write) -> Result<u64> {
    out.write_all(MAGIC)?;
    let items = source.items()?;
    ensure!(items.len() <= MAX_FILES, "Too many files");
    let mut w = FrameWriter { sealer, family, sid, out, buf: Vec::new(), index: 0, total: 0 };
    for item in &items {
        match item {
            Item::Dir(rel) => {
                ensure!(valid_relative(rel), "Unexpected folder name");
                w.write_all(b"D")?;
                w.write_all(&(rel.len() as u16).to_le_bytes())?;
                w.write_all(rel.as_bytes())?;
            }
            Item::File(rel, size) => {
                ensure!(valid_relative(rel), "Unexpected file name");
                w.write_all(b"F")?;
                w.write_all(&(rel.len() as u16).to_le_bytes())?;
                w.write_all(rel.as_bytes())?;
                w.write_all(&size.to_le_bytes())?;
                let written = source.read(rel, &mut w)?;
                ensure!(written == *size, "A file changed while it was being saved");
            }
        }
    }
    w.write_all(b"E")?;
    w.finish()
}

/// Reads frames in order and yields plaintext.
struct FrameReader<'a> {
    sealer: &'a dyn Sealer,
    family: &'a str,
    sid: &'a str,
    input: &'a mut dyn Read,
    buf: Vec<u8>,
    pos: usize,
    index: u64,
    done: bool,
}

impl FrameReader<'_> {
    fn next_frame(&mut self) -> Result<()> {
        ensure!(!self.done, "Read past the end");
        let mut head = [0u8; 17];
        self.input.read_exact(&mut head)?;
        let last = match head[0] {
            0 => false,
            1 => true,
            _ => bail!("Damaged saved data"),
        };
        let nonce: [u8; 12] = head[1..13].try_into().expect("12 bytes");
        let len = u32::from_le_bytes(head[13..17].try_into().expect("4 bytes")) as usize;
        ensure!((TAG..=CHUNK + TAG).contains(&len), "Damaged saved data");
        let mut sealed = vec![0u8; len];
        self.input.read_exact(&mut sealed)?;
        self.buf = self.sealer.open(&nonce, &aad(self.family, self.sid, self.index, last), &sealed)?;
        self.pos = 0;
        self.index += 1;
        if last {
            self.done = true;
            let mut extra = [0u8; 1];
            ensure!(self.input.read(&mut extra)? == 0, "Damaged saved data");
        } else {
            ensure!(self.buf.len() == CHUNK, "Damaged saved data");
        }
        Ok(())
    }
}

impl Read for FrameReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        while self.pos == self.buf.len() {
            if self.done {
                return Ok(0);
            }
            self.next_frame().map_err(std::io::Error::other)?;
        }
        let n = (self.buf.len() - self.pos).min(out.len());
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

fn read_path(r: &mut dyn Read) -> Result<String> {
    let mut len = [0u8; 2];
    r.read_exact(&mut len)?;
    let len = u16::from_le_bytes(len) as usize;
    ensure!(len > 0 && len <= 1024, "Damaged saved data");
    let mut raw = vec![0u8; len];
    r.read_exact(&mut raw)?;
    let path = String::from_utf8(raw)?;
    ensure!(valid_relative(&path), "Unexpected name in saved data");
    Ok(path)
}

pub fn decrypt(input: &mut dyn Read, sealer: &dyn Sealer, family: &str, sid: &str, sink: &mut dyn Sink) -> Result<()> {
    let mut magic = [0u8; 8];
    input.read_exact(&mut magic)?;
    ensure!(&magic == MAGIC, "Damaged saved data");
    let mut r = FrameReader { sealer, family, sid, input, buf: Vec::new(), pos: 0, index: 0, done: false };
    // Authenticate and parse everything into a list first? No: data can be
    // large. Instead every frame is authenticated before its bytes are used,
    // and paths are checked before anything is created.
    let mut count = 0usize;
    loop {
        let mut tag = [0u8; 1];
        r.read_exact(&mut tag)?;
        match tag[0] {
            b'D' => {
                let path = read_path(&mut r)?;
                sink.dir(&path)?;
            }
            b'F' => {
                let path = read_path(&mut r)?;
                let mut size = [0u8; 8];
                r.read_exact(&mut size)?;
                let size = u64::from_le_bytes(size);
                ensure!(size <= MAX_BYTES, "Damaged saved data");
                let mut limited = (&mut r).take(size);
                sink.file(&path, size, &mut limited)?;
                ensure!(limited.limit() == 0, "Damaged saved data");
            }
            b'E' => break,
            _ => bail!("Damaged saved data"),
        }
        count += 1;
        ensure!(count <= MAX_FILES, "Too many files");
    }
    // The archive must end exactly at the final frame.
    let mut extra = [0u8; 1];
    ensure!(r.read(&mut extra)? == 0 && r.done, "Damaged saved data");
    Ok(())
}

/// Plain folder walker (tests and documentation of the contract; the
/// Windows reader in `winfs` adds link and ownership checks).
#[cfg(test)]
pub struct DirSource {
    root: std::path::PathBuf,
}

#[cfg(test)]
impl DirSource {
    pub fn new(root: &std::path::Path) -> Self {
        DirSource { root: root.to_path_buf() }
    }
}

#[cfg(test)]
impl Source for DirSource {
    fn items(&mut self) -> Result<Vec<Item>> {
        fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<Item>) -> Result<()> {
            let mut entries: Vec<_> = std::fs::read_dir(dir)?.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let rel = e.path().strip_prefix(root)?.to_string_lossy().replace('\\', "/");
                let meta = std::fs::symlink_metadata(e.path())?;
                ensure!(!meta.file_type().is_symlink(), "Link");
                if meta.is_dir() {
                    out.push(Item::Dir(rel));
                    walk(root, &e.path(), out)?;
                } else {
                    out.push(Item::File(rel, meta.len()));
                }
            }
            Ok(())
        }
        let mut out = Vec::new();
        walk(&self.root, &self.root, &mut out)?;
        Ok(out)
    }
    fn read(&mut self, rel: &str, out: &mut dyn Write) -> Result<u64> {
        let mut f = std::fs::File::open(self.root.join(rel))?;
        Ok(std::io::copy(&mut f, out)?)
    }
}

#[cfg(test)]
pub struct DirSink {
    root: std::path::PathBuf,
}

#[cfg(test)]
impl DirSink {
    pub fn new(root: &std::path::Path) -> Self {
        DirSink { root: root.to_path_buf() }
    }
}

#[cfg(test)]
impl Sink for DirSink {
    fn dir(&mut self, rel: &str) -> Result<()> {
        std::fs::create_dir_all(self.root.join(rel))?;
        Ok(())
    }
    fn file(&mut self, rel: &str, _size: u64, data: &mut dyn Read) -> Result<()> {
        let mut f = std::fs::File::create(self.root.join(rel))?;
        std::io::copy(data, &mut f)?;
        Ok(())
    }
}
```

Ensure `Sha256`/`Digest` imports are test-only (they are used only by the `Fake` sealer).

Note on `unpack_refuses_bad_paths`: `read_path` validates before `sink.file` is called, so nothing is written.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --locked --lib debloat::vault`
Expected: PASS. Clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/debloat/vault.rs src/debloat/mod.rs
git -c user.email=skeleton22121@gmail.com -c user.name=slay commit -m "Clean up apps: encrypted app-data archive format

Claude-Session: https://claude.ai/code/session_019sBc3UDNnKBXUfzPSA9bDy"
```

---

### Task 3: Windows crypto (`wincrypto.rs`)

**Files:**
- Create: `src/debloat/wincrypto.rs` (`#[cfg(windows)]`)
- Modify: `src/debloat/mod.rs` (`#[cfg(windows)] pub(crate) mod wincrypto;`), `Cargo.toml` (add `"Win32_Security_Cryptography"` to the `windows-sys` features list)
- Test: inline `#[cfg(test)]` tests, run **on the VM** (they are Windows-only)

**Interfaces:**
- Consumes: `vault::Sealer`.
- Produces (Task 6): `pub struct Key` (32 bytes, zeroed on drop, `pub fn random() -> Key`, `pub fn bytes(&self) -> &[u8; 32]`), `pub struct Aes` (`pub fn new(key: &Key) -> anyhow::Result<Aes>`, `impl Sealer`), `pub fn seal_key(key: &Key) -> anyhow::Result<Vec<u8>>`, `pub fn unseal_key(blob: &[u8]) -> anyhow::Result<Key>`.

- [ ] **Step 1: Write the failing tests** (inside `src/debloat/wincrypto.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::debloat::vault::Sealer;

    #[test]
    fn aes_gcm_roundtrip_and_tamper() {
        let key = Key::random();
        let aes = Aes::new(&key).unwrap();
        let nonce = [9u8; 12];
        let sealed = aes.seal(&nonce, b"aad", b"hello world").unwrap();
        assert_eq!(sealed.len(), 11 + 16);
        assert_eq!(aes.open(&nonce, b"aad", &sealed).unwrap(), b"hello world");
        assert!(aes.open(&nonce, b"aaD", &sealed).is_err(), "aad bound");
        let mut bad = sealed.clone();
        bad[0] ^= 1;
        assert!(aes.open(&nonce, b"aad", &bad).is_err(), "ciphertext bound");
        let mut bad = sealed.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(aes.open(&nonce, b"aad", &bad).is_err(), "tag bound");
        assert!(Aes::new(&Key::random()).unwrap().open(&nonce, b"aad", &sealed).is_err(), "key bound");
        // Empty plaintext is allowed (final frame of an empty archive).
        let empty = aes.seal(&nonce, b"", b"").unwrap();
        assert_eq!(aes.open(&nonce, b"", &empty).unwrap(), b"");
    }

    #[test]
    fn known_answer_matches_nist_gcm() {
        // NIST GCM test case 14 (AES-256, zero key, zero IV, 16 zero bytes).
        let key = Key::from_bytes([0u8; 32]);
        let aes = Aes::new(&key).unwrap();
        let sealed = aes.seal(&[0u8; 12], b"", &[0u8; 16]).unwrap();
        assert_eq!(
            crate::debloat::backup::hex(&sealed),
            "cea7403d4d606b6e074ec5d3baf39d18d0d1c8a799996bf0265b98b5d48ab919"
        );
    }

    #[test]
    fn dpapi_machine_seal_roundtrip() {
        let key = Key::random();
        let blob = seal_key(&key).unwrap();
        assert!(!blob.windows(32).any(|w| w == key.bytes()));
        assert_eq!(unseal_key(&blob).unwrap().bytes(), key.bytes());
        let mut bad = blob.clone();
        let mid = bad.len() / 2;
        bad[mid] ^= 1;
        assert!(unseal_key(&bad).is_err());
    }
}
```

- [ ] **Step 2: Build the Windows test binary to see it fail**

Run: `cargo test --locked --target x86_64-pc-windows-gnu --lib --no-run 2>&1 | tail -3`
Expected: compile errors (missing `Key`, `Aes`, ...).

- [ ] **Step 3: Implement**

```rust
//! AES-256-GCM through Windows CNG and the machine-bound DPAPI seal for the
//! per-backup key. No crypto is implemented here, only called.
use super::vault::Sealer;
use anyhow::{ensure, Result};
use rand::RngCore;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{LocalFree, STATUS_SUCCESS};
use windows_sys::Win32::Security::Cryptography::*;

const ENTROPY: &[u8] = b"Secblitz app backup key v1";

pub struct Key([u8; 32]);

impl Key {
    pub fn random() -> Key {
        let mut k = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut k);
        Key(k)
    }
    #[cfg(test)]
    pub fn from_bytes(b: [u8; 32]) -> Key {
        Key(b)
    }
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        for b in self.0.iter_mut() {
            // Volatile so the clear is not optimised away.
            unsafe { std::ptr::write_volatile(b, 0) };
        }
    }
}

pub struct Aes {
    alg: BCRYPT_ALG_HANDLE,
    key: BCRYPT_KEY_HANDLE,
}

// CNG handles are usable from any thread; Aes is used from one at a time.
unsafe impl Send for Aes {}

impl Aes {
    pub fn new(key: &Key) -> Result<Aes> {
        let mut alg: BCRYPT_ALG_HANDLE = null_mut();
        let status = unsafe { BCryptOpenAlgorithmProvider(&mut alg, BCRYPT_AES_ALGORITHM, null(), 0) };
        ensure!(status == STATUS_SUCCESS, "Encryption is unavailable ({status:#x})");
        let mode: Vec<u16> = "ChainingModeGCM\0".encode_utf16().collect();
        let status = unsafe {
            BCryptSetProperty(
                alg,
                BCRYPT_CHAINING_MODE,
                mode.as_ptr().cast(),
                (mode.len() * 2) as u32,
                0,
            )
        };
        if status != STATUS_SUCCESS {
            unsafe { BCryptCloseAlgorithmProvider(alg, 0) };
            anyhow::bail!("Encryption is unavailable ({status:#x})");
        }
        let mut handle: BCRYPT_KEY_HANDLE = null_mut();
        let status = unsafe {
            BCryptGenerateSymmetricKey(alg, &mut handle, null_mut(), 0, key.0.as_ptr(), 32, 0)
        };
        if status != STATUS_SUCCESS {
            unsafe { BCryptCloseAlgorithmProvider(alg, 0) };
            anyhow::bail!("Encryption is unavailable ({status:#x})");
        }
        Ok(Aes { alg, key: handle })
    }

    fn info(nonce: &[u8; 12], aad: &[u8], tag: &mut [u8; 16]) -> BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
        let mut info: BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as u32;
        info.dwInfoVersion = BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION;
        info.pbNonce = nonce.as_ptr() as *mut u8;
        info.cbNonce = 12;
        info.pbAuthData = if aad.is_empty() { null_mut() } else { aad.as_ptr() as *mut u8 };
        info.cbAuthData = aad.len() as u32;
        info.pbTag = tag.as_mut_ptr();
        info.cbTag = 16;
        info
    }
}

impl Drop for Aes {
    fn drop(&mut self) {
        unsafe {
            BCryptDestroyKey(self.key);
            BCryptCloseAlgorithmProvider(self.alg, 0);
        }
    }
}

impl Sealer for Aes {
    fn seal(&self, nonce: &[u8; 12], aad: &[u8], plain: &[u8]) -> Result<Vec<u8>> {
        let mut tag = [0u8; 16];
        let info = Self::info(nonce, aad, &mut tag);
        let mut out = vec![0u8; plain.len()];
        let mut written = 0u32;
        let status = unsafe {
            BCryptEncrypt(
                self.key,
                plain.as_ptr(),
                plain.len() as u32,
                (&info as *const BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast(),
                null_mut(),
                0,
                out.as_mut_ptr(),
                out.len() as u32,
                &mut written,
                0,
            )
        };
        ensure!(status == STATUS_SUCCESS && written as usize == plain.len(), "Encryption failed ({status:#x})");
        out.extend_from_slice(&tag);
        Ok(out)
    }

    fn open(&self, nonce: &[u8; 12], aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>> {
        ensure!(sealed.len() >= 16, "Damaged saved data");
        let (ct, tag_in) = sealed.split_at(sealed.len() - 16);
        let mut tag: [u8; 16] = tag_in.try_into().expect("16 bytes");
        let info = Self::info(nonce, aad, &mut tag);
        let mut out = vec![0u8; ct.len()];
        let mut written = 0u32;
        let status = unsafe {
            BCryptDecrypt(
                self.key,
                ct.as_ptr(),
                ct.len() as u32,
                (&info as *const BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast(),
                null_mut(),
                0,
                out.as_mut_ptr(),
                out.len() as u32,
                &mut written,
                0,
            )
        };
        ensure!(status == STATUS_SUCCESS && written as usize == ct.len(), "Damaged saved data");
        Ok(out)
    }
}

fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: bytes.len() as u32, pbData: bytes.as_ptr() as *mut u8 }
}

/// Seal the key to this PC (any elevated process here can open it; it is
/// useless if the saved copy is taken to another PC).
pub fn seal_key(key: &Key) -> Result<Vec<u8>> {
    let input = blob(&key.0);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: null_mut() };
    let ok = unsafe {
        CryptProtectData(
            &input,
            null(),
            &entropy,
            null(),
            null(),
            CRYPTPROTECT_LOCAL_MACHINE | CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    };
    ensure!(ok != 0, "Couldn't protect the saved data key: {}", std::io::Error::last_os_error());
    let sealed = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe { LocalFree(out.pbData.cast()) };
    Ok(sealed)
}

pub fn unseal_key(sealed: &[u8]) -> Result<Key> {
    let input = blob(sealed);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: null_mut() };
    let ok = unsafe {
        CryptUnprotectData(&input, null_mut(), &entropy, null(), null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
    };
    ensure!(ok != 0, "Damaged saved data key");
    let plain = unsafe { std::slice::from_raw_parts_mut(out.pbData, out.cbData as usize) };
    let result = if plain.len() == 32 {
        let mut k = [0u8; 32];
        k.copy_from_slice(plain);
        Ok(Key(k))
    } else {
        Err(anyhow::anyhow!("Damaged saved data key"))
    };
    for b in plain.iter_mut() {
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    unsafe { LocalFree(out.pbData.cast()) };
    result
}
```

If a `windows-sys` 0.59 name differs (for example `BCRYPT_ALG_HANDLE` vs `*mut c_void`, or `CRYPTPROTECT_LOCAL_MACHINE` type), adjust to the crate's actual signature; do not change behaviour. Check names with `grep -rn "fn BCryptEncrypt" ~/.cargo/registry/src/*/windows-sys-0.59*/src/Windows/Win32/Security/Cryptography/mod.rs`.

- [ ] **Step 4: Run the Windows tests on the VM**

```bash
source target/build-tools/cross-env.sh
cargo test --locked --target x86_64-pc-windows-gnu --lib --no-run 2>&1 | grep -o 'target/[^ )]*secblitz-[0-9a-f]*\.exe' | tail -1
```

Copy that file to the VM and run only these tests (guest control, any user):

```bash
cd target/windows-validation-tools
python3 guest.py put ../x86_64-pc-windows-gnu/debug/deps/secblitz-<hash>.exe 'C:\Users\Public\sb-tests.exe'
printf '& C:\\Users\\Public\\sb-tests.exe debloat::wincrypto --test-threads 1 2>&1 | Out-String\n' > t-wincrypto.ps1
python3 guest.py ps t-wincrypto.ps1
```

Expected: `test result: ok. 3 passed`. If the known-answer test fails, the CNG call is wrong; fix before going on.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/debloat/wincrypto.rs src/debloat/mod.rs
git -c user.email=skeleton22121@gmail.com -c user.name=slay commit -m "Clean up apps: AES-GCM and machine-bound key seal via Windows

Claude-Session: https://claude.ai/code/session_019sBc3UDNnKBXUfzPSA9bDy"
```

---

### Task 4: Windows file work (`winfs.rs`)

**Files:**
- Create: `src/debloat/winfs.rs` (`#[cfg(windows)]`)
- Modify: `src/debloat/mod.rs` (`#[cfg(windows)] pub(crate) mod winfs;`), `Cargo.toml` (add `"Win32_Security_Isolation"` to `windows-sys` features)
- Test: host tests for the pure SDDL check in `winfs` live in `backup.rs`? No: put the SDDL checker in `backup.rs` (pure) as `pub fn template_sddl(template: &str, from_family: &str, to_family: &str) -> Result<String>` with host tests; Windows behaviour is tested on the VM in Task 8.

**Interfaces:**
- Consumes: `backup::{FileEntry, hash_file, valid_relative, valid_sid, hex, MAX_FILES, MAX_BYTES}`, `vault::{Item, Source, Sink}`.
- Produces (Task 6):
  - `pub fn enable_privileges() -> anyhow::Result<()>` (SeBackupPrivilege + SeRestorePrivilege on the process token)
  - `pub fn windows_apps() -> anyhow::Result<PathBuf>` (`%ProgramFiles%\WindowsApps` from `SHGetKnownFolderPath(FOLDERID_ProgramFiles)`)
  - `pub fn free_bytes(path: &Path) -> anyhow::Result<u64>`
  - `pub fn copy_out(src: &Path, dst: &Path) -> anyhow::Result<Vec<FileEntry>>` (backup semantics, no links, hashes while copying)
  - `pub fn copy_in(src: &Path, files: &[FileEntry], dst: &Path, dir_sddl: &str, file_sddl: &str) -> anyhow::Result<()>` (restore semantics into WindowsApps; `dst` must not exist; hashes while copying; on error removes `dst`)
  - `pub fn remove_tree(path: &Path) -> anyhow::Result<()>` (backup semantics, no links followed)
  - `pub fn security_sddl(path: &Path) -> anyhow::Result<String>` (owner, group, DACL)
  - `pub fn profile_dir(sid: &str) -> anyhow::Result<PathBuf>` (ProfileList `ProfileImagePath`)
  - `pub struct TreeSource` (`pub fn open(sid: &str, family: &str) -> anyhow::Result<TreeSource>`, `impl Source`) — reads one account's app data, never through a link
  - `pub struct DataSink` (`pub fn open(sid: &str, family: &str) -> anyhow::Result<DataSink>`, `impl Sink`) — writes into that account's `AppData\Local\Packages\<family>`, never through a link

And in `backup.rs` (pure, host-tested):
  - `pub fn template_sddl(template: &str, from_family: &str, to_family: &str) -> anyhow::Result<String>`

- [ ] **Step 1: Write the failing host test for `template_sddl`** (append to `backup.rs` tests)

```rust
    const DIR_SDDL: &str = "O:SYG:SYD:PAI(XA;;0x1200a9;;;BU;(WIN://SYSAPPID Contains \"Microsoft.WindowsAlarms_8wekyb3d8bbwe\"))(A;;0x1200a9;;;S-1-15-3-1288279408-4010470124-2163985056-447644096-1946037256-752919663-3751275627)(A;OICIIO;GXGR;;;BU)(A;OICIID;FA;;;S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464)(A;OICIID;0x1200a9;;;S-1-15-3-1024-3635283841-2530182609-996808640-1887759898-3848208603-3313616867-983405619-2501854204)(A;OICIID;FA;;;SY)(A;OICIID;0x1200a9;;;BA)(A;OICIID;0x1200a9;;;LS)(A;OICIID;0x1200a9;;;NS)(A;OICIID;0x1200a9;;;RC)";

    #[test]
    fn template_sddl_swaps_family_and_refuses_write_grants() {
        let out = template_sddl(DIR_SDDL, "Microsoft.WindowsAlarms_8wekyb3d8bbwe", "Microsoft.BingWeather_8wekyb3d8bbwe").unwrap();
        assert!(out.contains("\"Microsoft.BingWeather_8wekyb3d8bbwe\""));
        assert!(!out.contains("WindowsAlarms"));
        // Family must appear in the template.
        assert!(template_sddl(DIR_SDDL, "Microsoft.Other_8wekyb3d8bbwe", "Microsoft.BingWeather_8wekyb3d8bbwe").is_err());
        // Owner must be SYSTEM.
        assert!(template_sddl(&DIR_SDDL.replace("O:SY", "O:BU"), "Microsoft.WindowsAlarms_8wekyb3d8bbwe", "Microsoft.BingWeather_8wekyb3d8bbwe").is_err());
        // Any write grant to someone other than SYSTEM/TrustedInstaller is refused.
        for evil in ["(A;;FA;;;BU)", "(A;;0x120116;;;WD)", "(A;OICI;GA;;;AU)", "(A;;WD;;;BA)", "(A;;FA;;;S-1-5-21-1-2-3-1001)"] {
            let bad = DIR_SDDL.replacen("(A;OICIID;FA;;;SY)", &format!("(A;OICIID;FA;;;SY){evil}"), 1);
            assert!(template_sddl(&bad, "Microsoft.WindowsAlarms_8wekyb3d8bbwe", "Microsoft.BingWeather_8wekyb3d8bbwe").is_err(), "{evil}");
        }
        // Target family is validated (no quote injection).
        assert!(template_sddl(DIR_SDDL, "Microsoft.WindowsAlarms_8wekyb3d8bbwe", "Evil\")(A;;FA;;;WD)_8wekyb3d8bbwe").is_err());
    }
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test --locked --lib debloat::backup::tests::template_sddl`
Expected: compile error (`template_sddl` missing).

- [ ] **Step 3: Implement `template_sddl` in `backup.rs`**

```rust
const TRUSTED_INSTALLER: &str = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";

/// Rights that only read and run (FILE_GENERIC_READ|FILE_GENERIC_EXECUTE,
/// GENERIC_READ, GENERIC_EXECUTE and their two-letter forms).
fn read_only(rights: &str) -> bool {
    const ALLOWED: u32 = 0x0012_00a9 | 0x8000_0000 | 0x2000_0000; // FILE_GENERIC_READ|EXECUTE, GENERIC_READ, GENERIC_EXECUTE
    if let Some(hex) = rights.strip_prefix("0x") {
        return u32::from_str_radix(hex, 16).is_ok_and(|m| m & !ALLOWED == 0);
    }
    rights.len() % 2 == 0
        && rights
            .as_bytes()
            .chunks(2)
            .all(|t| matches!(t, b"GR" | b"GX" | b"FR" | b"FX" | b"RC" | b"LC" | b"SW" | b"RP" | b"LO"))
}

/// Security of a live Microsoft package folder (or file), re-targeted at
/// `to_family`. Refuses anything that would let a non-system account write.
pub fn template_sddl(template: &str, from_family: &str, to_family: &str) -> Result<String> {
    let (to_name, to_pub) = to_family.rsplit_once('_').context("Unexpected family")?;
    ensure!(
        plain_token(to_name, 50) && to_pub.len() == 13 && to_pub.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
        "Unexpected family"
    );
    ensure!(template.starts_with("O:SYG:SY"), "Unexpected owner");
    let quoted = format!("\"{from_family}\"");
    ensure!(template.contains(&quoted), "Template does not name its app");
    let dacl = template.split_once("D:").context("No DACL")?.1;
    ensure!(!dacl.contains("S:"), "Unexpected audit section");
    let mut rest = dacl.trim_start_matches(|c: char| c.is_ascii_uppercase());
    while !rest.is_empty() {
        ensure!(rest.starts_with('('), "Unexpected DACL");
        // Conditional ACEs contain parentheses; find the matching close.
        let mut depth = 0usize;
        let mut end = None;
        for (i, c) in rest.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end = end.context("Unexpected DACL")?;
        let ace = &rest[1..end];
        let fields: Vec<&str> = ace.splitn(7, ';').collect();
        ensure!(fields.len() >= 6, "Unexpected ACE");
        let (kind, rights, sid) = (fields[0], fields[2], fields[5]);
        ensure!(matches!(kind, "A" | "XA" | "D" | "XD"), "Unexpected ACE type");
        let trusted = matches!(sid, "SY") || sid == TRUSTED_INSTALLER;
        ensure!(kind.ends_with('D') || trusted || read_only(rights), "Template grants write access");
        rest = &rest[end + 1..];
    }
    Ok(template.replace(&quoted, &format!("\"{to_family}\"")))
}
```

- [ ] **Step 4: Run the host test**

Run: `cargo test --locked --lib debloat::backup`
Expected: PASS.

- [ ] **Step 5: Implement `winfs.rs`**

```rust
//! Privileged file work for saved copies. Every path is opened without
//! following links, and every handle is checked to still be inside the
//! folder it should be in (a planted junction is refused, never followed).
use super::backup::{hex, valid_relative, FileEntry, MAX_BYTES, MAX_FILES};
use super::vault::{Item, Sink, Source};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::ffi::{c_void, OsString};
use std::fs::File;
use std::io::{Read, Write};
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Security::Authorization::*;
use windows_sys::Win32::Security::*;
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath};

fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}

struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

pub fn enable_privileges() -> Result<()> {
    unsafe {
        let mut token: HANDLE = null_mut();
        ensure!(
            OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) != 0,
            "Couldn't get permission to save app files"
        );
        let token = OwnedHandle(token);
        for name in ["SeBackupPrivilege", "SeRestorePrivilege"] {
            let mut luid: LUID = zeroed();
            ensure!(LookupPrivilegeValueW(null(), wide(name).as_ptr(), &mut luid) != 0, "Unknown privilege");
            let tp = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
            };
            ensure!(AdjustTokenPrivileges(token.0, 0, &tp, 0, null_mut(), null_mut()) != 0, "Couldn't get permission to save app files");
            ensure!(GetLastError() != ERROR_NOT_ALL_ASSIGNED, "Couldn't get permission to save app files");
        }
    }
    Ok(())
}

struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub fn windows_apps() -> Result<PathBuf> {
    let mut raw: windows_sys::core::PWSTR = null_mut();
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFiles, 0, null_mut(), &mut raw) };
    ensure!(hr == 0 && !raw.is_null(), "Program Files not found");
    let len = (0..).take_while(|&i| unsafe { *raw.add(i) } != 0).count();
    let path = PathBuf::from(OsString::from_wide(unsafe { std::slice::from_raw_parts(raw, len) }));
    unsafe { windows_sys::Win32::System::Com::CoTaskMemFree(raw.cast()) };
    ensure!(path.is_absolute(), "Program Files not found");
    Ok(path.join("WindowsApps"))
}

pub fn free_bytes(path: &Path) -> Result<u64> {
    let mut free = 0u64;
    ensure!(
        unsafe { GetDiskFreeSpaceExW(wide(path).as_ptr(), &mut free, null_mut(), null_mut()) } != 0,
        "Couldn't read free space"
    );
    Ok(free)
}

/// Open without following a link, with backup semantics.
fn open_raw(path: &Path, access: u32, share: u32, disposition: u32, sa: *const SECURITY_ATTRIBUTES, extra: u32) -> Result<File> {
    let h = unsafe {
        CreateFileW(
            wide(path).as_ptr(),
            access,
            share,
            sa,
            disposition,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT | extra,
            null_mut(),
        )
    };
    ensure!(h != INVALID_HANDLE_VALUE, "{}: {}", path.display(), std::io::Error::last_os_error());
    Ok(unsafe { File::from_raw_handle(h) })
}

fn info(f: &File) -> Result<BY_HANDLE_FILE_INFORMATION> {
    let mut i: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
    ensure!(unsafe { GetFileInformationByHandle(f.as_raw_handle(), &mut i) } != 0, "File information unavailable");
    Ok(i)
}

fn is_link(i: &BY_HANDLE_FILE_INFORMATION) -> bool {
    i.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn is_dir(i: &BY_HANDLE_FILE_INFORMATION) -> bool {
    i.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0
}

/// Resolved path of an open handle (`\\?\C:\...`), lowercased for compare.
fn final_path(f: &File) -> Result<String> {
    let mut buf = vec![0u16; 32768];
    let n = unsafe { GetFinalPathNameByHandleW(f.as_raw_handle(), buf.as_mut_ptr(), buf.len() as u32, 0) } as usize;
    ensure!(n > 0 && n < buf.len(), "Couldn't resolve a path");
    Ok(String::from_utf16_lossy(&buf[..n]).to_lowercase())
}

fn inside(child: &File, root: &str) -> Result<()> {
    let p = final_path(child)?;
    ensure!(p.starts_with(root) && p[root.len()..].starts_with('\\'), "A link points outside the app's folder");
    Ok(())
}

fn rel_path(root: &Path, rel: &str) -> Result<PathBuf> {
    ensure!(valid_relative(rel), "Unexpected name");
    Ok(rel.split('/').fold(root.to_path_buf(), |p, part| p.join(part)))
}

/// Children of an open directory (names only), via FindFirstFileExW on the path
/// after the directory handle proved the path is the real, link-free folder.
fn children(dir: &Path) -> Result<Vec<(String, bool, u64, bool)>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let name = e.file_name().into_string().map_err(|_| anyhow::anyhow!("Non-text file name"))?;
        let meta = std::fs::symlink_metadata(e.path())?;
        use std::os::windows::fs::MetadataExt;
        let link = meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || meta.file_type().is_symlink();
        out.push((name, meta.is_dir(), meta.len(), link));
    }
    out.sort();
    Ok(out)
}

/// Copy a WindowsApps package folder into the store, hashing as it goes.
pub fn copy_out(src: &Path, dst: &Path) -> Result<Vec<FileEntry>> {
    let root = open_raw(src, FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, OPEN_EXISTING, null(), 0)?;
    let ri = info(&root)?;
    ensure!(is_dir(&ri) && !is_link(&ri), "Unexpected app folder");
    let root_final = final_path(&root)?;
    std::fs::create_dir_all(dst)?;
    let mut files = Vec::new();
    let mut bytes = 0u64;
    fn walk(src: &Path, dst: &Path, rel: &str, root_final: &str, files: &mut Vec<FileEntry>, bytes: &mut u64, depth: usize) -> Result<()> {
        ensure!(depth <= 32, "Folder too deep");
        for (name, dir, _len, link) in children(src)? {
            ensure!(!link, "Link inside an app folder");
            let child_rel = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            ensure!(valid_relative(&child_rel), "Unexpected file name");
            let s = src.join(&name);
            let d = dst.join(&name);
            if dir {
                std::fs::create_dir(&d)?;
                walk(&s, &d, &child_rel, root_final, files, bytes, depth + 1)?;
            } else {
                let mut input = open_raw(&s, GENERIC_READ, FILE_SHARE_READ, OPEN_EXISTING, null(), FILE_FLAG_SEQUENTIAL_SCAN)?;
                let i = info(&input)?;
                ensure!(!is_link(&i) && !is_dir(&i), "Unexpected file");
                inside(&input, root_final)?;
                let mut out = File::create(&d)?;
                let mut hasher = Sha256::new();
                let mut buf = vec![0u8; 1 << 16];
                let mut size = 0u64;
                loop {
                    let n = input.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    hasher.update(&buf[..n]);
                    out.write_all(&buf[..n])?;
                    size += n as u64;
                }
                out.sync_all()?;
                *bytes += size;
                files.push(FileEntry { path: child_rel, size, sha256: hex(&hasher.finalize()) });
                ensure!(files.len() <= MAX_FILES && *bytes <= MAX_BYTES, "App too large to save");
            }
        }
        Ok(())
    }
    walk(src, dst, "", &root_final, &mut files, &mut bytes, 0)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn descriptor(sddl: &str) -> Result<Local> {
    let mut p = null_mut();
    ensure!(
        unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(wide(sddl).as_ptr(), 1, &mut p, null_mut()) } != 0,
        "Unexpected folder permissions"
    );
    Ok(Local(p))
}

/// Copy a saved package back to `dst` (inside WindowsApps), creating every
/// folder and file new with the given security and checking each byte
/// against the saved list while copying. `dst` must not exist.
pub fn copy_in(src: &Path, files: &[FileEntry], dst: &Path, dir_sddl: &str, file_sddl: &str) -> Result<()> {
    ensure!(!dst.exists(), "The app's folder is already there");
    let result = (|| -> Result<()> {
        let dsd = descriptor(dir_sddl)?;
        let fsd = descriptor(file_sddl)?;
        let dsa = SECURITY_ATTRIBUTES { nLength: size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: dsd.0, bInheritHandle: 0 };
        let fsa = SECURITY_ATTRIBUTES { nLength: size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: fsd.0, bInheritHandle: 0 };
        let mkdir = |p: &Path| -> Result<()> {
            if unsafe { CreateDirectoryW(wide(p).as_ptr(), &dsa) } == 0 {
                ensure!(unsafe { GetLastError() } == ERROR_ALREADY_EXISTS, "Couldn't create {}", p.display());
                let d = open_raw(p, FILE_READ_ATTRIBUTES, FILE_SHARE_READ | FILE_SHARE_WRITE, OPEN_EXISTING, null(), 0)?;
                let i = info(&d)?;
                ensure!(is_dir(&i) && !is_link(&i), "Unexpected folder");
            }
            Ok(())
        };
        mkdir(dst)?;
        for f in files {
            let target = rel_path(dst, &f.path)?;
            let mut parent = dst.to_path_buf();
            let parts: Vec<&str> = f.path.split('/').collect();
            for part in &parts[..parts.len() - 1] {
                parent = parent.join(part);
                mkdir(&parent)?;
            }
            let mut input = File::open(rel_path(src, &f.path)?)?;
            let mut out = open_raw(&target, GENERIC_WRITE, 0, CREATE_NEW, &fsa, 0)?;
            let mut hasher = Sha256::new();
            let mut buf = vec![0u8; 1 << 16];
            let mut size = 0u64;
            loop {
                let n = input.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                out.write_all(&buf[..n])?;
                size += n as u64;
            }
            out.sync_all()?;
            ensure!(size == f.size && hex(&hasher.finalize()) == f.sha256, "Saved copy changed");
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = remove_tree(dst);
    }
    result
}

/// Delete a folder tree with backup semantics; links are deleted, never followed.
pub fn remove_tree(path: &Path) -> Result<()> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    use std::os::windows::fs::MetadataExt;
    let link = meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    if meta.is_dir() && !link {
        for (name, _, _, _) in children(path)? {
            remove_tree(&path.join(name))?;
        }
        std::fs::remove_dir(path)?;
    } else if meta.is_dir() {
        std::fs::remove_dir(path)?; // removes the junction itself
    } else {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub fn security_sddl(path: &Path) -> Result<String> {
    let (mut sd, mut out) = (null_mut(), null_mut());
    let what = OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    ensure!(
        unsafe { GetNamedSecurityInfoW(wide(path).as_ptr(), SE_FILE_OBJECT, what, null_mut(), null_mut(), null_mut(), null_mut(), &mut sd) } == 0,
        "Couldn't read folder permissions"
    );
    let _sd = Local(sd);
    let mut len = 0u32;
    ensure!(
        unsafe { ConvertSecurityDescriptorToStringSecurityDescriptorW(sd, 1, what, &mut out, &mut len) } != 0,
        "Couldn't read folder permissions"
    );
    let _out = Local(out.cast());
    let text = unsafe { std::slice::from_raw_parts(out, len as usize) };
    Ok(String::from_utf16_lossy(text).trim_end_matches('\0').to_owned())
}

/// `ProfileImagePath` for a user account (expanded), from the registry.
pub fn profile_dir(sid: &str) -> Result<PathBuf> {
    ensure!(super::backup::valid_sid(sid), "Unexpected account");
    let key = format!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\ProfileList\\{sid}");
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(HKEY_LOCAL_MACHINE, wide(&key).as_ptr(), wide("ProfileImagePath").as_ptr(), RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ, null_mut(), buf.as_mut_ptr().cast(), &mut size)
    };
    ensure!(status == 0, "Account folder not found");
    let len = (size as usize / 2).saturating_sub(1);
    let path = PathBuf::from(OsString::from_wide(&buf[..len]));
    ensure!(path.is_absolute(), "Account folder not found");
    Ok(path)
}

/// One account's app data folder, read with backup semantics. Links inside
/// are skipped (not followed, not saved); every file is proven to resolve
/// inside the folder before it is read.
pub struct TreeSource {
    root: PathBuf,
    root_final: String,
    _pins: Vec<File>,
}

/// Open each component from the profile down to the app's folder, refusing
/// links, holding the handles without delete sharing so none of them can be
/// swapped for a link while we work.
fn pin_chain(profile: &Path, family: &str) -> Result<(PathBuf, Vec<File>)> {
    let mut pins = Vec::new();
    let mut path = profile.to_path_buf();
    let p = open_raw(&path, FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | READ_CONTROL, FILE_SHARE_READ | FILE_SHARE_WRITE, OPEN_EXISTING, null(), 0)?;
    let i = info(&p)?;
    ensure!(is_dir(&i) && !is_link(&i), "Unexpected account folder");
    pins.push(p);
    for part in ["AppData", "Local", "Packages", family] {
        path = path.join(part);
        let p = open_raw(&path, FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | READ_CONTROL, FILE_SHARE_READ | FILE_SHARE_WRITE, OPEN_EXISTING, null(), 0)?;
        let i = info(&p)?;
        ensure!(is_dir(&i) && !is_link(&i), "A link was found in the app's data folder");
        pins.push(p);
    }
    Ok((path, pins))
}

impl TreeSource {
    pub fn open(sid: &str, family: &str) -> Result<TreeSource> {
        let profile = profile_dir(sid)?;
        let (root, pins) = pin_chain(&profile, family)?;
        let root_final = final_path(pins.last().expect("pinned"))?;
        Ok(TreeSource { root, root_final, _pins: pins })
    }
}

impl Source for TreeSource {
    fn items(&mut self) -> Result<Vec<Item>> {
        fn walk(root: &Path, dir: &Path, rel: &str, out: &mut Vec<Item>, depth: usize) -> Result<()> {
            ensure!(depth <= 32 && out.len() <= MAX_FILES, "Too much app data");
            for (name, is_dir, len, link) in children(dir)? {
                if link {
                    continue; // never follow or save links
                }
                let child = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
                if !valid_relative(&child) {
                    continue;
                }
                if is_dir {
                    out.push(Item::Dir(child.clone()));
                    walk(root, &dir.join(&name), &child, out, depth + 1)?;
                } else {
                    out.push(Item::File(child, len));
                }
            }
            Ok(())
        }
        let mut out = Vec::new();
        walk(&self.root, &self.root, "", &mut out, 0)?;
        Ok(out)
    }

    fn read(&mut self, rel: &str, out: &mut dyn Write) -> Result<u64> {
        let path = rel_path(&self.root, rel)?;
        let mut f = open_raw(&path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, OPEN_EXISTING, null(), FILE_FLAG_SEQUENTIAL_SCAN)?;
        let i = info(&f)?;
        ensure!(!is_link(&i) && !is_dir(&i) && i.nNumberOfLinks == 1, "Unexpected file in app data");
        inside(&f, &self.root_final)?;
        Ok(std::io::copy(&mut f, out)?)
    }
}

/// Writes one account's app data back. The app's folder must already exist
/// (Windows creates it, with the right permissions, when the app is
/// registered for that account); nothing is created above it.
pub struct DataSink {
    root: PathBuf,
    root_final: String,
    owner: Local,
    _pins: Vec<File>,
}

impl DataSink {
    pub fn open(sid: &str, family: &str) -> Result<DataSink> {
        let profile = profile_dir(sid)?;
        let (root, pins) = pin_chain(&profile, family)?;
        let root_final = final_path(pins.last().expect("pinned"))?;
        let mut owner = null_mut();
        ensure!(unsafe { ConvertStringSidToSidW(wide(sid).as_ptr(), &mut owner) } != 0, "Unexpected account");
        Ok(DataSink { root, root_final, owner: Local(owner), _pins: pins })
    }

    fn give_to_owner(&self, f: &File) -> Result<()> {
        let status = unsafe { SetSecurityInfo(f.as_raw_handle(), SE_FILE_OBJECT, OWNER_SECURITY_INFORMATION, self.owner.0, null_mut(), null(), null()) };
        ensure!(status == 0, "Couldn't hand the file back to its account");
        Ok(())
    }
}

impl Sink for DataSink {
    fn dir(&mut self, rel: &str) -> Result<()> {
        let path = rel_path(&self.root, rel)?;
        if unsafe { CreateDirectoryW(wide(&path).as_ptr(), null()) } == 0 {
            ensure!(unsafe { GetLastError() } == ERROR_ALREADY_EXISTS, "Couldn't create a folder");
        }
        let d = open_raw(&path, FILE_READ_ATTRIBUTES | WRITE_OWNER | READ_CONTROL, FILE_SHARE_READ | FILE_SHARE_WRITE, OPEN_EXISTING, null(), 0)?;
        let i = info(&d)?;
        ensure!(is_dir(&i) && !is_link(&i), "A link was found in the app's data folder");
        inside(&d, &self.root_final)?;
        self.give_to_owner(&d)
    }

    fn file(&mut self, rel: &str, size: u64, data: &mut dyn Read) -> Result<()> {
        let path = rel_path(&self.root, rel)?;
        // Replace an existing plain file (the app may have created defaults),
        // never write through a link or a hard link.
        let mut f = match open_raw(&path, GENERIC_WRITE | WRITE_OWNER | READ_CONTROL, 0, CREATE_NEW, null(), 0) {
            Ok(f) => f,
            Err(_) => {
                let f = open_raw(&path, GENERIC_WRITE | WRITE_OWNER | READ_CONTROL, 0, OPEN_EXISTING, null(), 0)?;
                let i = info(&f)?;
                ensure!(!is_link(&i) && !is_dir(&i) && i.nNumberOfLinks == 1, "A link was found in the app's data folder");
                f.set_len(0)?;
                f
            }
        };
        inside(&f, &self.root_final)?;
        let written = std::io::copy(data, &mut f)?;
        ensure!(written == size, "Damaged saved data");
        f.sync_all()?;
        self.give_to_owner(&f)
    }
}

```

Remove any unused import once it compiles (clippy `-D warnings` must pass). If `windows-sys` names differ, adapt to the crate's real signatures without changing behaviour.

Decision recorded here (it refines the spec's step 7): Secblitz never creates an account's app data folder. Windows creates it when the app is registered for that account: right away for the account that restores, and at next sign-in for other accounts (through provisioning). So restore writes data only where the folder exists, and keeps the rest as *pending* (Task 6: `pending.json` beside the manifest). Secblitz runs elevated as whoever opens it, so when another account opens Secblitz later, `offline::finish_pending()` writes that account's data and clears its entry. Until then the result says "Some of its saved data couldn't be put back" only if a write *failed*, not for accounts that are merely pending.

- [ ] **Step 6: Build and clippy**

Run: `cargo clippy --locked --target x86_64-pc-windows-gnu --all-targets -- -D warnings` and `cargo test --locked --lib debloat::`
Expected: clean, PASS. (Windows behaviour is exercised in Task 6's VM smoke step and Task 8.)

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/debloat/winfs.rs src/debloat/backup.rs src/debloat/mod.rs
git -c user.email=skeleton22121@gmail.com -c user.name=slay commit -m "Clean up apps: link-safe copy in and out for saved copies

Claude-Session: https://claude.ai/code/session_019sBc3UDNnKBXUfzPSA9bDy"
```

---

### Task 5: PowerShell scripts (`describe.ps1`, `register.ps1`)

**Files:**
- Create: `src/debloat/scripts/describe.ps1`, `src/debloat/scripts/register.ps1`
- Modify: `src/debloat/windows.rs` (add `pub const DESCRIBE: &str = include_str!("scripts/describe.ps1");` and `pub const REGISTER: &str = include_str!("scripts/register.ps1");`)
- Modify: `src/debloat/offline.rs` will parse their output (Task 6). Add the parser tests there.

**Interfaces:**
- `describe.ps1` input: `$env:SECBLITZ_APP` (package name, already validated by `catalog::owner`). Output, one JSON line:

```json
{"packages":[{"fullName":"...","kind":"bundle|main|resource","family":"..."}],
 "frameworks":["Microsoft.VCLibs.140.00_..._x64__8wekyb3d8bbwe"],
 "provisioned":false,
 "users":["S-1-5-21-..."],
 "template":{"family":"Microsoft.WindowsCalculator_8wekyb3d8bbwe","fullName":"Microsoft.WindowsCalculator_11.2607.0.0_x64__8wekyb3d8bbwe"},
 "error":null}
```

- `register.ps1` input: `$env:SECBLITZ_REGISTER` = base64 UTF-8 JSON `{"manifests":["C:\\Program Files\\WindowsApps\\...\\AppxManifest.xml", ...], "provision":"Family_pub" | ""}`. Output: `{"ok":true|false,"error":null|"...","status":[{"fullName":"...","status":"Ok"}]}`.

- [ ] **Step 1: Write `describe.ps1`**

```powershell
# Describes ONE app (name in $env:SECBLITZ_APP, validated by the caller) so
# Secblitz can save a copy before removing it. Read-only. Output: one JSON
# object. Paths are not reported: Secblitz derives them itself.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$name = [string]$env:SECBLITZ_APP
$result = [ordered]@{ packages = @(); frameworks = @(); provisioned = $false; users = @(); template = $null; error = $null }
try {
    if ([string]::IsNullOrWhiteSpace($name)) { throw 'No app name' }
    $all = @(Get-AppxPackage -AllUsers -Name $name -PackageTypeFilter Main,Bundle,Resource)
    $frameworks = @{}
    $users = @{}
    foreach ($p in $all) {
        $kind = if ($p.IsBundle) { 'bundle' } elseif ($p.IsResourcePackage) { 'resource' } else { 'main' }
        $result.packages += ,([ordered]@{ fullName = [string]$p.PackageFullName; kind = $kind; family = [string]$p.PackageFamilyName })
        if ($kind -eq 'main') {
            foreach ($d in @($p.Dependencies)) {
                if ($d.IsFramework) { $frameworks[[string]$d.PackageFullName] = $true }
            }
            foreach ($u in @($p.PackageUserInformation)) {
                $sid = [string]$u.UserSecurityId.Sid
                if ($sid -like 'S-1-5-21-*' -and [string]$u.InstallState -eq 'Installed') { $users[$sid] = $true }
            }
        }
    }
    $result.frameworks = @($frameworks.Keys | Sort-Object)
    $result.users = @($users.Keys | Sort-Object)
    $result.provisioned = @(Get-AppxProvisionedPackage -Online | Where-Object { $_.DisplayName -eq $name }).Count -gt 0
    # A registered Microsoft app whose folder security is copied for restore.
    $t = Get-AppxPackage -PackageTypeFilter Main | Where-Object {
        $_.PackageFamilyName -like '*_8wekyb3d8bbwe' -and -not $_.IsFramework -and $_.SignatureKind -eq 'Store' -and
        $_.Name -ne $name -and [string]$_.Status -eq 'Ok'
    } | Sort-Object Name | Select-Object -First 1
    if ($t) { $result.template = [ordered]@{ family = [string]$t.PackageFamilyName; fullName = [string]$t.PackageFullName } }
}
catch {
    $result.error = [string]$_.Exception.Message
}
ConvertTo-Json -InputObject $result -Compress -Depth 4
```

- [ ] **Step 2: Write `register.ps1`**

```powershell
# Registers saved app packages that Secblitz has already copied back into
# WindowsApps and checked. Input: base64 JSON in $env:SECBLITZ_REGISTER with
# "manifests" (in order: frameworks, then bundle or main) and "provision"
# (family name or ""). Output: one JSON object.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$result = [ordered]@{ ok = $false; error = $null; status = @() }
try {
    $json = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String([string]$env:SECBLITZ_REGISTER))
    $request = ConvertFrom-Json -InputObject $json
    foreach ($m in @($request.manifests)) {
        Add-AppxPackage -Register ([string]$m) -DisableDevelopmentMode -ErrorAction Stop
    }
    $family = [string]$request.provision
    if ($family) {
        $null = [Windows.Management.Deployment.PackageManager, Windows.Management.Deployment, ContentType = WindowsRuntime]
        Add-Type -AssemblyName System.Runtime.WindowsRuntime
        $asTask = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
            $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and
            $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperationWithProgress`2'
        } | Select-Object -First 1
        $pm = [Windows.Management.Deployment.PackageManager]::new()
        $op = $pm.ProvisionPackageForAllUsersAsync($family)
        $task = $asTask.MakeGenericMethod([Windows.Management.Deployment.DeploymentResult], [Windows.Management.Deployment.DeploymentProgress]).Invoke($null, @($op))
        if (-not $task.Wait(600000)) { throw 'Making the app available to every account timed out' }
        if ($task.Result.ExtendedErrorCode) { throw $task.Result.ErrorText }
    }
    foreach ($m in @($request.manifests)) {
        $full = Split-Path -Leaf (Split-Path -Parent ([string]$m))
        if ($full -eq 'AppxMetadata') { $full = Split-Path -Leaf (Split-Path -Parent (Split-Path -Parent ([string]$m))) }
        $p = Get-AppxPackage -PackageTypeFilter Main,Bundle,Framework | Where-Object { $_.PackageFullName -eq $full } | Select-Object -First 1
        $result.status += ,([ordered]@{ fullName = $full; status = if ($p) { [string]$p.Status } else { 'Missing' } })
    }
    $result.ok = @($result.status | Where-Object { $_.status -ne 'Ok' }).Count -eq 0
    if (-not $result.ok) { $result.error = 'Windows did not accept the saved copy' }
}
catch {
    $result.error = [string]$_.Exception.Message
}
ConvertTo-Json -InputObject $result -Compress -Depth 4
```

- [ ] **Step 3: Add the constants to `src/debloat/windows.rs`**

```rust
pub const DESCRIBE: &str = include_str!("scripts/describe.ps1");
pub const REGISTER: &str = include_str!("scripts/register.ps1");
```

The PRELUDE already imports `Appx` and `Dism` by absolute path. The `Windows.Management.Deployment` WinRT type and `System.Runtime.WindowsRuntime` are inbox and need no module.

- [ ] **Step 4: Run both scripts on the VM through the real runner** (smoke; full checks in Task 8)

Build a debug Windows exe with a temporary `#[test] #[ignore]` in `offline.rs` that prints `windows::run(windows::DESCRIBE, &[("SECBLITZ_APP","Microsoft.BingWeather")], ..)`. Run it on the VM **from the desktop session** (AppX needs a real profile):

```bash
cd target/windows-validation-tools
cat > t-describe.ps1 <<'EOF'
& C:\Users\Public\sb-tests.exe debloat::offline::tests::smoke_describe --ignored --nocapture --test-threads 1 2>&1
EOF
python3 guest.py put ../x86_64-pc-windows-gnu/debug/deps/secblitz-<hash>.exe 'C:\Users\Public\sb-tests.exe'
./run-desk.sh t-describe.ps1 300
```

Expected JSON lists the bundle, main and scale-100 resource of Weather, five Microsoft frameworks, the Tester SID, and a template app. Remove the temporary test afterwards.

- [ ] **Step 5: Commit**

```bash
git add src/debloat/scripts/describe.ps1 src/debloat/scripts/register.ps1 src/debloat/windows.rs
git -c user.email=skeleton22121@gmail.com -c user.name=slay commit -m "Clean up apps: describe and register scripts for saved copies

Claude-Session: https://claude.ai/code/session_019sBc3UDNnKBXUfzPSA9bDy"
```

---

### Task 6: Orchestration (`offline.rs`) and removal integration

**Files:**
- Create: `src/debloat/offline.rs`
- Modify: `src/debloat/mod.rs` (`pub mod offline;`, `ItemResult::Kept`, `Progress::Saving`, backup step in `remove_with`/`remove`)
- Modify: `src/debloat/tests.rs` (update existing `remove_with` tests for the new parameter)
- Modify: `src/launcher.rs` (`user_sid_string` becomes `pub(crate)` and is re-exported from `launcher` under `#[cfg(windows)]`)
- Test: inline tests in `offline.rs`

**Interfaces:**
- Consumes: Tasks 1–5.
- Produces (Task 7):
  - `pub enum Kept { NoSpace, NoCopy(String) }` and `ItemResult::Kept(Kept)` (in `mod.rs`)
  - `Progress::Saving(u16)` (in `mod.rs`)
  - `pub enum Restored { Back, BackWithoutSomeData, AlreadyThere, Damaged, NoCopy }`
  - `pub fn restore_index(index: u16) -> anyhow::Result<Restored>` (blocking, elevated GUI process)
  - `pub fn has_copy(index: u16) -> bool`
  - `pub fn delete_index(index: u16) -> anyhow::Result<()>`
  - `pub fn saved_bytes() -> u64`
  - `pub fn finish_pending()` (GUI calls it once at start, in a blocking task)
  - `pub trait Host` and `pub(crate) fn backup_family_with(host: &dyn Host, store: &Store, index: u16, name: &str) -> Result<BackupOutcome, Kept>` / `pub(crate) fn restore_with(host: &dyn Host, store: &Store, index: u16) -> Result<Restored>`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};

    const MAIN: &str = "Microsoft.BingWeather_4.54.63045.0_x64__8wekyb3d8bbwe";
    const MAIN2: &str = "Microsoft.BingWeather_4.55.1.0_x64__8wekyb3d8bbwe";
    const BUNDLE: &str = "Microsoft.BingWeather_4.54.63045.0_neutral_~_8wekyb3d8bbwe";
    const FW: &str = "Microsoft.VCLibs.140.00_14.0.33519.0_x64__8wekyb3d8bbwe";
    const FAMILY: &str = "Microsoft.BingWeather_8wekyb3d8bbwe";
    const TESTER: &str = "S-1-5-21-1-2-3-1001";

    fn index() -> u16 {
        crate::debloat::catalog::owner("Microsoft.BingWeather").unwrap()
    }

    #[derive(Default)]
    struct Fake {
        described: Described,
        free: u64,
        installed: RefCell<BTreeSet<String>>, // full names in WindowsApps
        registered: RefCell<Vec<Vec<String>>>,
        log: RefCell<Vec<String>>,
        fail_register: bool,
        fail_data_restore: bool,
        data: RefCell<BTreeMap<String, Vec<u8>>>, // sid -> plaintext marker
        signed_out: RefCell<BTreeSet<String>>, // accounts with no data folder yet
        me: RefCell<String>,
    }

    impl Host for Fake {
        fn describe(&self, _name: &str) -> Result<Described> {
            Ok(self.described.clone())
        }
        fn free_bytes(&self) -> Result<u64> {
            Ok(self.free)
        }
        fn copy_out(&self, full: &str, dst: &Path) -> Result<Vec<FileEntry>> {
            self.log.borrow_mut().push(format!("out {full}"));
            std::fs::create_dir_all(dst)?;
            std::fs::write(dst.join("AppxManifest.xml"), full.as_bytes())?;
            crate::debloat::backup::hash_tree(dst)
        }
        fn present(&self, full: &str) -> bool {
            self.installed.borrow().contains(full)
        }
        fn save_data(&self, sid: &str, _family: &str, sealer: &dyn Sealer, out: &mut dyn Write) -> Result<u64> {
            self.log.borrow_mut().push(format!("save {sid}"));
            let plain = self.data.borrow().get(sid).cloned().unwrap_or_default();
            crate::debloat::vault::encrypt(&mut OneFile(plain), sealer, FAMILY, sid, out)
        }
        fn new_key(&self) -> Result<(Box<dyn Sealer>, Vec<u8>)> {
            Ok((Box::new(Plain), b"sealed-key".to_vec()))
        }
        fn open_key(&self, sealed: &[u8]) -> Result<Box<dyn Sealer>> {
            ensure!(sealed == b"sealed-key", "bad key");
            Ok(Box::new(Plain))
        }
        fn copy_in(&self, full: &str, _src: &Path, _files: &[FileEntry], _template: &Template) -> Result<()> {
            self.log.borrow_mut().push(format!("in {full}"));
            self.installed.borrow_mut().insert(full.to_owned());
            Ok(())
        }
        fn remove_copy(&self, full: &str) -> Result<()> {
            self.log.borrow_mut().push(format!("undo {full}"));
            self.installed.borrow_mut().remove(full);
            Ok(())
        }
        fn template(&self, _family: &str) -> Result<Template> {
            Ok(Template { dir: "D".into(), file: "F".into() })
        }
        fn register(&self, fulls: &[String], provision: Option<&str>) -> Result<()> {
            self.log.borrow_mut().push(format!("register {} provision={}", fulls.join(","), provision.unwrap_or("")));
            ensure!(!self.fail_register, "Windows said no");
            self.registered.borrow_mut().push(fulls.to_vec());
            Ok(())
        }
        fn restore_data(&self, sid: &str, _family: &str, sealer: &dyn Sealer, input: &mut dyn Read) -> Result<()> {
            self.log.borrow_mut().push(format!("data {sid}"));
            ensure!(!self.fail_data_restore, "link found");
            let mut sink = Capture::default();
            crate::debloat::vault::decrypt(input, sealer, FAMILY, sid, &mut sink)?;
            self.data.borrow_mut().insert(sid.to_owned(), sink.0);
            Ok(())
        }
        fn registered_ok(&self, _family: &str) -> Result<bool> {
            Ok(!self.registered.borrow().is_empty())
        }
        fn data_folder_ready(&self, sid: &str, _family: &str) -> bool {
            !self.signed_out.borrow().contains(sid)
        }
        fn current_sid(&self) -> Result<String> {
            Ok(self.me.borrow().clone())
        }
    }

    /// Identity "cipher" for orchestration tests (framing is tested in vault).
    struct Plain;
    impl Sealer for Plain {
        fn seal(&self, _: &[u8; 12], _: &[u8], p: &[u8]) -> Result<Vec<u8>> {
            let mut v = p.to_vec();
            v.extend_from_slice(&[0u8; 16]);
            Ok(v)
        }
        fn open(&self, _: &[u8; 12], _: &[u8], s: &[u8]) -> Result<Vec<u8>> {
            Ok(s[..s.len() - 16].to_vec())
        }
    }
    struct OneFile(Vec<u8>);
    impl crate::debloat::vault::Source for OneFile {
        fn items(&mut self) -> Result<Vec<crate::debloat::vault::Item>> {
            Ok(vec![crate::debloat::vault::Item::File("LocalState/marker".into(), self.0.len() as u64)])
        }
        fn read(&mut self, _: &str, out: &mut dyn Write) -> Result<u64> {
            out.write_all(&self.0)?;
            Ok(self.0.len() as u64)
        }
    }
    #[derive(Default)]
    struct Capture(Vec<u8>);
    impl crate::debloat::vault::Sink for Capture {
        fn dir(&mut self, _: &str) -> Result<()> {
            Ok(())
        }
        fn file(&mut self, _: &str, _: u64, data: &mut dyn Read) -> Result<()> {
            data.read_to_end(&mut self.0)?;
            Ok(())
        }
    }

    fn weather() -> Fake {
        let fake = Fake {
            described: Described {
                packages: vec![
                    Pkg { full_name: BUNDLE.into(), kind: Kind::Bundle },
                    Pkg { full_name: MAIN.into(), kind: Kind::Main },
                ],
                frameworks: vec![FW.into()],
                provisioned: false,
                users: vec![TESTER.into()],
                template_family: "Microsoft.WindowsCalculator_8wekyb3d8bbwe".into(),
            },
            free: 100 * GB,
            ..Fake::default()
        };
        fake.installed.borrow_mut().extend([BUNDLE.to_string(), MAIN.to_string(), FW.to_string()]);
        fake.data.borrow_mut().insert(TESTER.into(), b"favourite city".to_vec());
        fake
    }
    const GB: u64 = 1024 * 1024 * 1024;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join(crate::debloat::backup::DIR));
        (dir, store)
    }

    #[test]
    fn backup_saves_packages_frameworks_and_data_then_commits() {
        let (_d, store) = store();
        let host = weather();
        backup_family_with(&host, &store, index(), "Microsoft.BingWeather").unwrap();
        let m = store.load(FAMILY, index()).unwrap().expect("committed");
        assert_eq!(m.packages.len(), 2);
        assert_eq!(m.frameworks, vec![FW.to_string()]);
        assert_eq!(m.data.len(), 1);
        assert!(store.load_framework(FW).unwrap().is_some());
        assert!(store.family_dir(FAMILY).join("key.bin").exists());
    }

    #[test]
    fn shared_framework_is_copied_once() {
        let (_d, store) = store();
        let host = weather();
        backup_family_with(&host, &store, index(), "Microsoft.BingWeather").unwrap();
        host.log.borrow_mut().clear();
        backup_family_with(&host, &store, index(), "Microsoft.BingWeather").unwrap();
        assert!(!host.log.borrow().iter().any(|l| l == &format!("out {FW}")));
    }

    #[test]
    fn backup_keeps_every_version_of_the_family() {
        let (_d, store) = store();
        let mut host = weather();
        host.described.packages.push(Pkg { full_name: MAIN2.into(), kind: Kind::Main });
        host.installed.borrow_mut().insert(MAIN2.into());
        backup_family_with(&host, &store, index(), "Microsoft.BingWeather").unwrap();
        let m = store.load(FAMILY, index()).unwrap().unwrap();
        assert!(m.packages.iter().any(|p| p.full_name == MAIN2));
    }

    #[test]
    fn low_space_keeps_the_app() {
        let (_d, store) = store();
        let mut host = weather();
        host.free = GB / 2;
        assert!(matches!(backup_family_with(&host, &store, index(), "Microsoft.BingWeather"), Err(Kept::NoSpace)));
        assert!(store.for_index(index()).is_empty());
    }

    #[test]
    fn foreign_package_in_description_is_refused() {
        let (_d, store) = store();
        let mut host = weather();
        host.described.packages.push(Pkg { full_name: "Microsoft.WindowsStore_1.0.0.0_x64__8wekyb3d8bbwe".into(), kind: Kind::Main });
        assert!(matches!(backup_family_with(&host, &store, index(), "Microsoft.BingWeather"), Err(Kept::NoCopy(_))));
        assert!(store.for_index(index()).is_empty());
    }

    #[test]
    fn restore_runs_frameworks_then_app_then_data() {
        let (_d, store) = store();
        let host = weather();
        backup_family_with(&host, &store, index(), "Microsoft.BingWeather").unwrap();
        host.installed.borrow_mut().clear();
        host.data.borrow_mut().clear();
        host.log.borrow_mut().clear();
        assert_eq!(restore_with(&host, &store, index()).unwrap(), Restored::Back);
        let log = host.log.borrow().clone();
        let pos = |s: &str| log.iter().position(|l| l.starts_with(s)).unwrap_or_else(|| panic!("{s} in {log:?}"));
        assert!(pos(&format!("in {FW}")) < pos(&format!("in {MAIN}")));
        assert!(pos("register") > pos(&format!("in {BUNDLE}")));
        assert!(pos("data") > pos("register"));
        assert_eq!(host.registered.borrow()[0], vec![FW.to_string(), BUNDLE.to_string()]);
        assert_eq!(host.data.borrow().get(TESTER).unwrap(), b"favourite city");
    }

    #[test]
    fn restore_skips_frameworks_already_present() {
        let (_d, store) = store();
        let host = weather();
        backup_family_with(&host, &store, index(), "Microsoft.BingWeather").unwrap();
        host.installed.borrow_mut().retain(|f| f == FW);
        host.log.borrow_mut().clear();
        restore_with(&host, &store, index()).unwrap();
        assert!(!host.log.borrow().iter().any(|l| l == &format!("in {FW}")));
        assert_eq!(host.registered.borrow()[0], vec![BUNDLE.to_string()]);
    }

    #[test]
    fn restore_skips_when_already_installed() {
        let (_d, store) = store();
        let host = weather();
        backup_family_with(&host, &store, index(), "Microsoft.BingWeather").unwrap();
        // Still installed (e.g. reinstalled from the Store).
        assert_eq!(restore_with(&host, &store, index()).unwrap(), Restored::AlreadyThere);
        assert!(!host.log.borrow().iter().any(|l| l.starts_with("in ") || l.starts_with("register")));
    }

    #[test]
    fn damaged_copy_touches_nothing() {
        let (_d, store) = store();
        let host = weather();
        backup_family_with(&host, &store, index(), "Microsoft.BingWeather").unwrap();
        host.installed.borrow_mut().clear();
        let file = store.family_dir(FAMILY).join("packages").join(MAIN).join("AppxManifest.xml");
        std::fs::write(&file, b"tampered").unwrap();
        host.log.borrow_mut().clear();
        assert_eq!(restore_with(&host, &store, index()).unwrap(), Restored::Damaged);
        assert!(host.log.borrow().is_empty(), "{:?}", host.log.borrow());
    }

    #[test]
    fn windows_refusal_undoes_copied_folders() {
        let (_d, store) = store();
        let mut host = weather();
        backup_family_with(&host, &store, index(), "Microsoft.BingWeather").unwrap();
        host.installed.borrow_mut().clear();
        host.fail_register = true;
        assert!(restore_with(&host, &store, index()).is_err());
        assert!(host.installed.borrow().is_empty(), "copied folders removed again");
    }

    #[test]
    fn data_problem_still_restores_the_app() {
        let (_d, store) = store();
        let mut host = weather();
        backup_family_with(&host, &store, index(), "Microsoft.BingWeather").unwrap();
        host.installed.borrow_mut().clear();
        host.fail_data_restore = true;
        assert_eq!(restore_with(&host, &store, index()).unwrap(), Restored::BackWithoutSomeData);
    }

    #[test]
    fn other_accounts_data_waits_until_they_open_secblitz() {
        const OTHER: &str = "S-1-5-21-1-2-3-1002";
        let (_d, store) = store();
        let mut host = weather();
        host.described.users.push(OTHER.into());
        host.data.borrow_mut().insert(OTHER.into(), b"other city".to_vec());
        backup_family_with(&host, &store, index(), "Microsoft.BingWeather").unwrap();
        host.installed.borrow_mut().clear();
        host.data.borrow_mut().clear();
        host.signed_out.borrow_mut().insert(OTHER.into());
        assert_eq!(restore_with(&host, &store, index()).unwrap(), Restored::Back);
        assert!(host.data.borrow().get(OTHER).is_none());
        assert_eq!(read_pending(&store, FAMILY), vec![OTHER.to_string()]);

        // Still signed out: nothing happens.
        *host.me.borrow_mut() = OTHER.into();
        finish_pending_with(&host, &store).unwrap();
        assert!(host.data.borrow().get(OTHER).is_none());

        // They sign in (Windows makes the folder) and open Secblitz.
        host.signed_out.borrow_mut().clear();
        finish_pending_with(&host, &store).unwrap();
        assert_eq!(host.data.borrow().get(OTHER).unwrap(), b"other city");
        assert!(read_pending(&store, FAMILY).is_empty());
        assert!(!store.family_dir(FAMILY).join(PENDING).exists());
    }

    #[test]
    fn no_copy_means_store_fallback() {
        let (_d, store) = store();
        let host = weather();
        assert_eq!(restore_with(&host, &store, index()).unwrap(), Restored::NoCopy);
    }

    #[test]
    fn failed_removal_keeps_app_and_copy() {
        // remove_with integration: backup ok, removal fails.
        let (_d, store) = store();
        let host = weather();
        let installed = vec![crate::debloat::Installed { index: index(), package: "Microsoft.BingWeather".into(), version: "4.54".into() }];
        let backup = |i: &crate::debloat::Installed| backup_family_with(&host, &store, i.index, &i.package).map(|_| ());
        let batch = crate::debloat::remove_with(
            &[index()],
            &installed,
            &backup,
            &|_| crate::debloat::PackageOutcome::Failed("busy".into()),
            &|_| {},
        )
        .unwrap();
        assert!(batch.removed.is_empty());
        assert_eq!(batch.failed.len(), 1);
        assert!(store.load(FAMILY, index()).unwrap().is_some());
    }
}
```

Update `src/debloat/tests.rs`: every existing `remove_with(&indices, &installed, &run, &emit)` call gains a backup argument `&|_| Ok(())` in third position. Add one test there:

```rust
#[test]
fn no_copy_means_no_removal() {
    let index = crate::debloat::catalog::owner("Microsoft.BingWeather").unwrap();
    let installed = vec![Installed { index, package: "Microsoft.BingWeather".into(), version: "1".into() }];
    let ran = std::cell::Cell::new(false);
    let batch = remove_with(
        &[index],
        &installed,
        &|_| Err(Kept::NoSpace),
        &|_| {
            ran.set(true);
            PackageOutcome::Removed
        },
        &|_| {},
    )
    .unwrap();
    assert!(!ran.get(), "never removed without a copy");
    assert!(batch.removed.is_empty() && batch.failed.is_empty());
    assert_eq!(batch.kept, vec![index]);
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test --locked --lib debloat:: 2>&1 | tail -5`
Expected: compile errors.

- [ ] **Step 3: Implement `mod.rs` changes**

In `src/debloat/mod.rs`:

```rust
pub mod backup;
pub mod offline;
pub mod vault;
#[cfg(windows)]
pub(crate) mod wincrypto;
#[cfg(windows)]
pub(crate) mod winfs;

/// Why an app was left installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kept {
    /// Not enough free space to save a copy first.
    NoSpace,
    /// The copy couldn't be made (technical reason for the details only).
    NoCopy(String),
}
```

`ItemResult` gains `Kept(Kept)`; `Progress` gains `Saving(u16)`; `Batch` gains `#[serde(default)] pub kept: Vec<u16>`.

`remove_with` signature becomes:

```rust
pub(crate) fn remove_with(
    indices: &[u16],
    installed: &[Installed],
    backup: &dyn Fn(&Installed) -> std::result::Result<(), Kept>,
    run: &dyn Fn(&str) -> PackageOutcome,
    emit: &dyn Fn(Progress),
) -> Result<Batch>
```

and inside the per-index loop, before `emit(Progress::Started(index))`:

```rust
        emit(Progress::Saving(index));
        let mut kept = None;
        for p in &packages {
            if let Err(k) = backup(p) {
                kept = Some(k);
                break;
            }
        }
        if let Some(k) = kept {
            batch.kept.push(index);
            emit(Progress::Finished(index, ItemResult::Kept(k)));
            continue;
        }
```

`PackageOutcome` becomes `pub` (was `pub(crate)`) only if the offline tests need it from another module; they are in the same crate, so `pub(crate)` suffices.

`remove()` builds the store once and passes:

```rust
    #[cfg(windows)]
    let store = backup::Store::open()?;
    #[cfg(windows)]
    let backup = |p: &Installed| -> std::result::Result<(), Kept> {
        offline::backup_family_with(&offline::WindowsHost, &store, p.index, &p.package).map(|_| ())
    };
    #[cfg(not(windows))]
    let backup = |_: &Installed| -> std::result::Result<(), Kept> { Err(Kept::NoCopy("Only available on Windows".into())) };
    #[cfg(windows)]
    store.clean_staging();
```

Journal: `debloat.jsonl` lines without `kept` still parse (`#[serde(default)]`).

- [ ] **Step 4: Implement `offline.rs`**

```rust
//! Saving a copy before removal and bringing an app back from it. All
//! decisions live here and are tested with a fake `Host`; `WindowsHost`
//! does the real work through `winfs`, `wincrypto` and two scripts.
use super::backup::{self, FileEntry, FrameworkCopy, Kind, Manifest, Store, SCHEMA};
use super::vault::Sealer;
use super::Kept;
use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pkg {
    pub full_name: String,
    pub kind: Kind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Described {
    pub packages: Vec<Pkg>,
    pub frameworks: Vec<String>,
    pub provisioned: bool,
    pub users: Vec<String>,
    pub template_family: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub dir: String,
    pub file: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restored {
    Back,
    BackWithoutSomeData,
    AlreadyThere,
    Damaged,
    NoCopy,
}

pub struct BackupOutcome {
    pub bytes: u64,
}

pub trait Host {
    fn describe(&self, name: &str) -> Result<Described>;
    fn free_bytes(&self) -> Result<u64>;
    /// Copy WindowsApps\<full> to `dst`; returns the hashed file list.
    fn copy_out(&self, full: &str, dst: &Path) -> Result<Vec<FileEntry>>;
    /// Is WindowsApps\<full> there?
    fn present(&self, full: &str) -> bool;
    fn save_data(&self, sid: &str, family: &str, sealer: &dyn Sealer, out: &mut dyn Write) -> Result<u64>;
    /// A fresh key: its sealer and its machine-sealed form.
    fn new_key(&self) -> Result<(Box<dyn Sealer>, Vec<u8>)>;
    fn open_key(&self, sealed: &[u8]) -> Result<Box<dyn Sealer>>;
    fn template(&self, family: &str) -> Result<Template>;
    fn copy_in(&self, full: &str, src: &Path, files: &[FileEntry], template: &Template) -> Result<()>;
    fn remove_copy(&self, full: &str) -> Result<()>;
    /// Register, in order; then optionally provision `family` for all accounts.
    fn register(&self, fulls: &[String], provision: Option<&str>) -> Result<()>;
    fn restore_data(&self, sid: &str, family: &str, sealer: &dyn Sealer, input: &mut dyn Read) -> Result<()>;
    fn registered_ok(&self, family: &str) -> Result<bool>;
    /// Has Windows created this account's data folder for the app yet?
    fn data_folder_ready(&self, sid: &str, family: &str) -> bool;
    /// The account Secblitz is running as.
    fn current_sid(&self) -> Result<String>;
}

pub const PENDING: &str = "pending.json";

fn no_copy(e: impl std::fmt::Display) -> Kept {
    Kept::NoCopy(crate::text::excerpt(&e.to_string(), 300))
}

/// Save a verified copy of every package of `name` (and its data), then
/// commit it. Nothing is removed here.
pub(crate) fn backup_family_with(host: &dyn Host, store: &Store, index: u16, name: &str) -> std::result::Result<BackupOutcome, Kept> {
    let d = host.describe(name).map_err(no_copy)?;
    ensure_or(!d.packages.is_empty(), "Nothing to save")?;
    let mut family = None::<String>;
    for p in &d.packages {
        let id = backup::parse_full_name(&p.full_name).map_err(no_copy)?;
        ensure_or(super::catalog::owner(&id.name) == Some(index), "Package outside the app")?;
        ensure_or(family.get_or_insert_with(|| id.family()) == &id.family(), "Mixed families")?;
    }
    let family = family.expect("non-empty");
    // Space: package sizes are unknown until copied; require headroom plus
    // a generous estimate (4x the previous saved size, or 1 GB).
    let free = host.free_bytes().map_err(no_copy)?;
    if free < backup::HEADROOM.saturating_mul(2) {
        return Err(Kept::NoSpace);
    }
    let staging = store.new_staging().map_err(no_copy)?;
    let result = (|| -> Result<Manifest> {
        let mut packages = Vec::new();
        for p in &d.packages {
            let dst = staging.join(backup::PACKAGES).join(&p.full_name);
            let files = host.copy_out(&p.full_name, &dst)?;
            packages.push(backup::Package { full_name: p.full_name.clone(), kind: p.kind, files });
        }
        let mut frameworks = Vec::new();
        for f in &d.frameworks {
            let id = backup::parse_full_name(f)?;
            if id.publisher != backup::MICROSOFT {
                continue;
            }
            if store.load_framework(f).ok().flatten().is_none() && host.present(f) {
                let tmp = store.new_staging()?;
                let files = host.copy_out(f, &tmp.join("files"))?;
                let copy = FrameworkCopy { schema: SCHEMA, full_name: f.clone(), files };
                std::fs::write(tmp.join(backup::FRAMEWORK_MANIFEST), serde_json::to_vec(&copy)?)?;
                let target = store.framework_dir(f);
                std::fs::create_dir_all(target.parent().context("framework folder")?)?;
                let _ = std::fs::remove_dir_all(&target);
                std::fs::rename(&tmp, &target)?;
            }
            frameworks.push(f.clone());
        }
        let (sealer, sealed_key) = host.new_key()?;
        std::fs::write(staging.join(backup::KEY), &sealed_key)?;
        std::fs::create_dir_all(staging.join(backup::DATA))?;
        let mut data = Vec::new();
        for sid in &d.users {
            if !backup::valid_sid(sid) {
                continue;
            }
            let path = staging.join(backup::DATA).join(format!("{sid}.bin"));
            let mut out = std::fs::File::create(&path)?;
            match host.save_data(sid, &family, sealer.as_ref(), &mut out) {
                Ok(plain_size) => {
                    out.sync_all()?;
                    drop(out);
                    let (_, sha256) = backup::hash_file(&path)?;
                    data.push(backup::DataBlob { sid: sid.clone(), plain_size, sha256 });
                }
                // No data folder for this account (or a link was found):
                // the app is still saved; that account's data is not.
                Err(_) => {
                    drop(out);
                    std::fs::remove_file(&path)?;
                }
            }
        }
        let manifest = Manifest {
            schema: SCHEMA,
            created: super::now(),
            index,
            family: family.clone(),
            packages,
            frameworks,
            data,
            provisioned: d.provisioned,
        };
        manifest.check(index)?;
        std::fs::write(staging.join(backup::MANIFEST), serde_json::to_vec(&manifest)?)?;
        // Re-read everything from disk before trusting it.
        for p in &manifest.packages {
            backup::verify_tree(&staging.join(backup::PACKAGES).join(&p.full_name), &p.files)?;
        }
        Ok(manifest)
    })();
    match result {
        Ok(manifest) => {
            store.commit(&staging, &manifest.family).map_err(no_copy)?;
            let _ = store.gc_frameworks();
            Ok(BackupOutcome { bytes: manifest.packages.iter().flat_map(|p| &p.files).map(|f| f.size).sum() })
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            Err(no_copy(format!("{e:#}")))
        }
    }
}

fn ensure_or(ok: bool, why: &str) -> std::result::Result<(), Kept> {
    if ok {
        Ok(())
    } else {
        Err(Kept::NoCopy(why.into()))
    }
}

fn verify(store: &Store, m: &Manifest) -> Result<()> {
    let dir = store.family_dir(&m.family);
    for p in &m.packages {
        backup::verify_tree(&dir.join(backup::PACKAGES).join(&p.full_name), &p.files)?;
    }
    for f in &m.frameworks {
        if let Some(copy) = store.load_framework(f)? {
            backup::verify_tree(&store.framework_dir(f).join("files"), &copy.files)?;
        }
    }
    for b in &m.data {
        let (_, sha) = backup::hash_file(&dir.join(backup::DATA).join(format!("{}.bin", b.sid)))?;
        ensure!(sha == b.sha256, "Saved data changed");
    }
    Ok(())
}

/// Bring back every saved family of catalog entry `index`.
pub(crate) fn restore_with(host: &dyn Host, store: &Store, index: u16) -> Result<Restored> {
    let manifests = store.for_index(index);
    if manifests.is_empty() {
        return Ok(Restored::NoCopy);
    }
    let mut outcome = Restored::Back;
    for m in &manifests {
        if verify(store, m).is_err() {
            return Ok(Restored::Damaged);
        }
        if m.packages.iter().any(|p| host.present(&p.full_name)) {
            outcome = Restored::AlreadyThere;
            continue;
        }
        let template = host.template(&m.family)?;
        let dir = store.family_dir(&m.family);
        let mut copied: Vec<String> = Vec::new();
        let mut order: Vec<String> = Vec::new();
        let attempt = (|| -> Result<()> {
            for f in &m.frameworks {
                if host.present(f) {
                    continue;
                }
                let copy = store.load_framework(f)?.context("A part the app needs is missing")?;
                host.copy_in(f, &store.framework_dir(f).join("files"), &copy.files, &template)?;
                copied.push(f.clone());
                order.push(f.clone());
            }
            for p in &m.packages {
                host.copy_in(&p.full_name, &dir.join(backup::PACKAGES).join(&p.full_name), &p.files, &template)?;
                copied.push(p.full_name.clone());
            }
            // Bundles register their main and resource packages; without a
            // bundle, register each main package.
            let bundles: Vec<String> = m.packages.iter().filter(|p| p.kind == Kind::Bundle).map(|p| p.full_name.clone()).collect();
            if bundles.is_empty() {
                order.extend(m.packages.iter().filter(|p| p.kind == Kind::Main).map(|p| p.full_name.clone()));
            } else {
                order.extend(bundles);
            }
            let provision = (m.provisioned || m.data.len() > 1).then_some(m.family.as_str());
            host.register(&order, provision)?;
            ensure!(host.registered_ok(&m.family)?, "Windows did not accept the saved copy");
            Ok(())
        })();
        if let Err(e) = attempt {
            for full in copied.iter().rev() {
                let _ = host.remove_copy(full);
            }
            return Err(e);
        }
        let mut pending = Vec::new();
        for b in &m.data {
            if !host.data_folder_ready(&b.sid, &m.family) {
                // Windows makes the folder at that account's next sign-in;
                // finish_pending writes it when that account opens Secblitz.
                pending.push(b.sid.clone());
            } else if put_back(host, store, m, &b.sid).is_err() {
                outcome = Restored::BackWithoutSomeData;
            }
        }
        if pending.is_empty() {
            let _ = std::fs::remove_file(dir.join(PENDING));
        } else {
            std::fs::write(dir.join(PENDING), serde_json::to_vec(&pending)?)?;
        }
    }
    Ok(outcome)
}

fn put_back(host: &dyn Host, store: &Store, m: &Manifest, sid: &str) -> Result<()> {
    let dir = store.family_dir(&m.family);
    let sealer = host.open_key(&std::fs::read(dir.join(backup::KEY))?)?;
    let mut f = std::fs::File::open(dir.join(backup::DATA).join(format!("{sid}.bin")))?;
    host.restore_data(sid, &m.family, sealer.as_ref(), &mut f)
}

fn read_pending(store: &Store, family: &str) -> Vec<String> {
    std::fs::read(store.family_dir(family).join(PENDING))
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|s| backup::valid_sid(s))
        .collect()
}

/// Put back the current account's saved data for apps restored while it
/// was signed out. Quiet and best effort: runs at Secblitz start.
pub(crate) fn finish_pending_with(host: &dyn Host, store: &Store) -> Result<()> {
    let me = host.current_sid()?;
    for index in 0..super::catalog().len() as u16 {
        for m in store.for_index(index) {
            let mut pending = read_pending(store, &m.family);
            if !pending.contains(&me) || !m.data.iter().any(|b| b.sid == me) {
                continue;
            }
            if !host.data_folder_ready(&me, &m.family) {
                continue;
            }
            // Data that changed on disk is never decrypted.
            if verify(store, &m).is_err() {
                continue;
            }
            if put_back(host, store, &m, &me).is_ok() {
                pending.retain(|s| s != &me);
                let path = store.family_dir(&m.family).join(PENDING);
                if pending.is_empty() {
                    std::fs::remove_file(path)?;
                } else {
                    std::fs::write(path, serde_json::to_vec(&pending)?)?;
                }
            }
        }
    }
    Ok(())
}

/// Called once from the GUI's startup task (it runs elevated).
pub fn finish_pending() {
    #[cfg(windows)]
    if let Ok(store) = Store::open() {
        let _ = finish_pending_with(&WindowsHost, &store);
    }
}

pub fn has_copy(index: u16) -> bool {
    Store::open().map(|s| !s.for_index(index).is_empty()).unwrap_or(false)
}

pub fn delete_index(index: u16) -> Result<()> {
    let store = Store::open()?;
    for m in store.for_index(index) {
        store.delete(&m.family)?;
    }
    store.gc_frameworks()
}

pub fn saved_bytes() -> u64 {
    Store::open().map(|s| s.total_bytes()).unwrap_or(0)
}

pub fn restore_index(index: u16) -> Result<Restored> {
    #[cfg(windows)]
    {
        let store = Store::open()?;
        store.clean_staging();
        restore_with(&WindowsHost, &store, index)
    }
    #[cfg(not(windows))]
    {
        let _ = index;
        bail!("Only available on Windows")
    }
}

// ---- describe.ps1 output ---------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDescribed {
    #[serde(default)]
    packages: Vec<RawPkg>,
    #[serde(default)]
    frameworks: Vec<String>,
    #[serde(default)]
    provisioned: bool,
    #[serde(default)]
    users: Vec<String>,
    template: Option<RawTemplate>,
    error: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPkg {
    full_name: String,
    kind: String,
}
#[derive(Deserialize)]
struct RawTemplate {
    family: String,
}

pub(crate) fn parse_described(json: &str) -> Result<Described> {
    let raw: RawDescribed = serde_json::from_str(json).context("Read the app description")?;
    if let Some(e) = raw.error {
        bail!("{}", crate::text::excerpt(&e, 300));
    }
    let packages = raw
        .packages
        .into_iter()
        .map(|p| {
            let kind = match p.kind.as_str() {
                "bundle" => Kind::Bundle,
                "main" => Kind::Main,
                "resource" => Kind::Resource,
                _ => bail!("Unexpected package kind"),
            };
            backup::parse_full_name(&p.full_name)?;
            Ok(Pkg { full_name: p.full_name, kind })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Described {
        packages,
        frameworks: raw.frameworks.into_iter().filter(|f| backup::parse_full_name(f).is_ok()).collect(),
        provisioned: raw.provisioned,
        users: raw.users.into_iter().filter(|s| backup::valid_sid(s)).collect(),
        template_family: raw.template.map(|t| t.family).unwrap_or_default(),
    })
}

// ---- the real host ---------------------------------------------------------

#[cfg(windows)]
pub struct WindowsHost;

#[cfg(windows)]
impl Host for WindowsHost {
    fn describe(&self, name: &str) -> Result<Described> {
        ensure!(super::catalog::owner(name).is_some(), "Unknown app");
        let json = super::windows::run(super::windows::DESCRIBE, &[("SECBLITZ_APP", name)], std::time::Duration::from_secs(180))?;
        parse_described(&json)
    }
    fn free_bytes(&self) -> Result<u64> {
        super::winfs::free_bytes(&crate::platform::app_dir()?)
    }
    fn copy_out(&self, full: &str, dst: &Path) -> Result<Vec<FileEntry>> {
        backup::parse_full_name(full)?;
        super::winfs::enable_privileges()?;
        super::winfs::copy_out(&super::winfs::windows_apps()?.join(full), dst)
    }
    fn present(&self, full: &str) -> bool {
        backup::parse_full_name(full).is_ok()
            && super::winfs::windows_apps().is_ok_and(|w| std::fs::symlink_metadata(w.join(full)).is_ok())
    }
    fn save_data(&self, sid: &str, family: &str, sealer: &dyn Sealer, out: &mut dyn Write) -> Result<u64> {
        super::winfs::enable_privileges()?;
        let mut source = super::winfs::TreeSource::open(sid, family)?;
        super::vault::encrypt(&mut source, sealer, family, sid, out)
    }
    fn new_key(&self) -> Result<(Box<dyn Sealer>, Vec<u8>)> {
        let key = super::wincrypto::Key::random();
        let sealed = super::wincrypto::seal_key(&key)?;
        Ok((Box::new(super::wincrypto::Aes::new(&key)?), sealed))
    }
    fn open_key(&self, sealed: &[u8]) -> Result<Box<dyn Sealer>> {
        let key = super::wincrypto::unseal_key(sealed)?;
        Ok(Box::new(super::wincrypto::Aes::new(&key)?))
    }
    fn template(&self, family: &str) -> Result<Template> {
        // Re-describe to find a live Microsoft app whose security to copy.
        let name = family.rsplit_once('_').context("family")?.0;
        let d = self.describe(name).or_else(|_| self.describe("Microsoft.WindowsCalculator"))?;
        let from = d.template_family;
        ensure!(!from.is_empty(), "No app to copy folder permissions from");
        let apps = super::winfs::windows_apps()?;
        let dir = std::fs::read_dir(&apps)?
            .flatten()
            .find(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                backup::parse_full_name(&n).is_ok_and(|id| id.family() == from && id.resource.is_empty())
            })
            .context("No app to copy folder permissions from")?
            .path();
        let file = std::fs::read_dir(&dir)?
            .flatten()
            .find(|e| e.file_name() == "AppxManifest.xml")
            .context("No app to copy folder permissions from")?
            .path();
        Ok(Template {
            dir: backup::template_sddl(&super::winfs::security_sddl(&dir)?, &from, family)?,
            file: backup::template_sddl(&super::winfs::security_sddl(&file)?, &from, family)?,
        })
    }
    fn copy_in(&self, full: &str, src: &Path, files: &[FileEntry], template: &Template) -> Result<()> {
        backup::parse_full_name(full)?;
        super::winfs::enable_privileges()?;
        super::winfs::copy_in(src, files, &super::winfs::windows_apps()?.join(full), &template.dir, &template.file)
    }
    fn remove_copy(&self, full: &str) -> Result<()> {
        backup::parse_full_name(full)?;
        super::winfs::enable_privileges()?;
        super::winfs::remove_tree(&super::winfs::windows_apps()?.join(full))
    }
    fn register(&self, fulls: &[String], provision: Option<&str>) -> Result<()> {
        let apps = super::winfs::windows_apps()?;
        let manifests: Vec<String> = fulls
            .iter()
            .map(|f| {
                let id = backup::parse_full_name(f)?;
                let m = if id.resource == "~" {
                    apps.join(f).join("AppxMetadata").join("AppxBundleManifest.xml")
                } else {
                    apps.join(f).join("AppxManifest.xml")
                };
                Ok(m.to_string_lossy().into_owned())
            })
            .collect::<Result<_>>()?;
        let request = serde_json::json!({ "manifests": manifests, "provision": provision.unwrap_or("") });
        use base64::Engine as _;
        let encoded = base64::engine::general_purpose::STANDARD.encode(request.to_string());
        let json = super::windows::run(super::windows::REGISTER, &[("SECBLITZ_REGISTER", &encoded)], std::time::Duration::from_secs(900))?;
        let v: serde_json::Value = serde_json::from_str(&json).context("Read the answer")?;
        ensure!(v.get("ok") == Some(&serde_json::Value::Bool(true)), "{}", v.get("error").and_then(|e| e.as_str()).unwrap_or("Windows did not accept the saved copy"));
        Ok(())
    }
    fn restore_data(&self, sid: &str, family: &str, sealer: &dyn Sealer, input: &mut dyn Read) -> Result<()> {
        super::winfs::enable_privileges()?;
        let mut sink = super::winfs::DataSink::open(sid, family)?;
        super::vault::decrypt(input, sealer, family, sid, &mut sink)
    }
    fn registered_ok(&self, _family: &str) -> Result<bool> {
        // register.ps1 already checked every registered package's Status.
        Ok(true)
    }
    fn data_folder_ready(&self, sid: &str, family: &str) -> bool {
        super::winfs::profile_dir(sid).is_ok_and(|p| {
            std::fs::symlink_metadata(p.join("AppData").join("Local").join("Packages").join(family))
                .is_ok_and(|m| m.is_dir())
        })
    }
    fn current_sid(&self) -> Result<String> {
        // Make launcher::imp::user_sid_string pub(crate) and re-export it
        // as `#[cfg(windows)] pub(crate) use imp::user_sid_string;`.
        crate::launcher::user_sid_string()
    }
}
```

Add a `parse_described` test:

```rust
    #[test]
    fn parses_description_and_drops_junk() {
        let json = r#"{"packages":[{"fullName":"Microsoft.BingWeather_4.54.63045.0_x64__8wekyb3d8bbwe","kind":"main","family":"x"}],
            "frameworks":["Microsoft.VCLibs.140.00_14.0.33519.0_x64__8wekyb3d8bbwe","bad name"],
            "provisioned":true,"users":["S-1-5-21-1-2-3-1001","S-1-5-18"],
            "template":{"family":"Microsoft.WindowsCalculator_8wekyb3d8bbwe","fullName":"x"},"error":null}"#;
        let d = parse_described(json).unwrap();
        assert_eq!(d.packages.len(), 1);
        assert_eq!(d.frameworks.len(), 1);
        assert_eq!(d.users, vec!["S-1-5-21-1-2-3-1001".to_string()]);
        assert!(d.provisioned);
        assert!(parse_described(r#"{"error":"boom"}"#).is_err());
        assert!(parse_described(r#"{"packages":[{"fullName":"../x","kind":"main"}]}"#).is_err());
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test --locked --all-targets --no-fail-fast 2>&1 | grep -E "^test result|FAILED|panicked"`
Expected: all PASS (existing debloat tests updated). Clippy clean. Release build succeeds.

- [ ] **Step 6: VM smoke (no GUI yet)**

Add a temporary `#[test] #[ignore] fn smoke_roundtrip()` in `offline.rs` that, on Windows, calls `backup_family_with(&WindowsHost, &Store::open()?, idx, "Microsoft.BingWeather")`, then runs `windows::REMOVE`, then `restore_index(idx)` and prints each result. Run it on the VM from the desktop session as Tester (elevated) with `run-desk.sh`. Expected: backup OK, removed, `Back`. Check `Get-AppxPackage Microsoft.BingWeather` shows `Status Ok` and the app launches (`explorer.exe shell:AppsFolder\Microsoft.BingWeather_8wekyb3d8bbwe!App`). Remove the temporary test.

- [ ] **Step 7: Commit**

```bash
git add src/debloat/
git -c user.email=skeleton22121@gmail.com -c user.name=slay commit -m "Clean up apps: save a copy before removing, restore from it offline

Claude-Session: https://claude.ai/code/session_019sBc3UDNnKBXUfzPSA9bDy"
```

---

### Task 7: GUI and translations

**Files:**
- Modify: `src/gui/pages/debloat.rs`
- Modify: `src/i18n.rs` (TEXT rows)
- Test: existing GUI tests + new unit tests in `debloat.rs` for row actions

**Interfaces:**
- Consumes: `debloat::offline::{restore_index, has_copy, delete_index, saved_bytes, Restored}`, `debloat::{Kept, ItemResult::Kept, Progress::Saving}`.

Behaviour:

1. Review sheet: add one line under the existing list: **"Secblitz keeps a copy, so you can bring these apps back any time, even without internet."**
2. Working sheet: `Progress::Saving(i)` shows the spinner with **"Saving a copy…"**; `Started(i)` keeps **"Removing…"**.
3. `ItemResult::Kept(Kept::NoSpace)` row: info icon, **"Kept: not enough free space to save a copy"**. `Kept::NoCopy(_)` row: info icon, **"Kept: couldn't save a copy"** (technical reason only in the existing Technical details expander).
4. State gains `copies: BTreeSet<u16>` (indices with a saved copy) and `saved_bytes: u64`, loaded with the journal (`Task::perform(blocking(...))` returning both) and refreshed after remove/restore/delete.
5. Removed apps rows:
   - With a copy: trailing = `Restore` (Secondary button) + overflow menu with **"Get it from the Microsoft Store"** (only if `store_id.is_some()`) and **"Delete saved copy"** (danger). With only one of those, the existing single-item rule shows it as a button; keep the order Restore, then the menu.
   - Without a copy: unchanged (Store restore button / "look for it in the Microsoft Store yourself").
   - Subtitle with a copy: `ago(...)` + " · " + **"Can be brought back without internet"**.
6. Above the Removed list (only if `saved_bytes > 0`): small muted line **"Saved copies use about {size}."** where `{size}` = `crate::app::tools::size_phrase(saved_bytes)` (already plain: "230 MB", "1.2 GB").
7. `Msg::Restore(index)`: if `state.copies.contains(&index)` run `blocking(move || offline::restore_index(index))` → `Msg::RestoredOffline(index, Result<Restored, String>)`; else the existing broker Store path.
   - `Back` → existing success path (mark restored, history, inventory, toast **"{name} is back on your PC."**).
   - `BackWithoutSomeData` → same success path, toast **"{name} is back. Some of its saved data couldn't be put back."** (Tone::Neutral).
   - `AlreadyThere` → mark restored, toast **"{name} is already on your PC."**
   - `Damaged` → toast **"The saved copy of {name} is damaged, so it can't be brought back from Secblitz."** and, if `store_id` exists, fall through to the Store path automatically.
   - `NoCopy` → Store path.
   - `Err(_)` → toast **"We couldn't bring back {name}. Please try again later."** (existing string pieces) and keep the row.
8. `Msg::AskDelete(index)` opens a confirm sheet: title **"Delete the saved copy?"**, body **"{name} can then only come back from the Microsoft Store."** (or, if no `store_id`, **"{name} can't come back after this."**), buttons **Cancel** / **Delete** (Danger). `Msg::Delete(index)` runs `offline::delete_index` blocking, then reloads copies/size, toast **"Saved copy deleted."**
9. While restoring from a copy the row shows the existing spinner with **"Bringing back {name}…"** replacing "Restoring…" for that path.
10. At GUI start (the existing startup batch in `src/gui/mod.rs` / the debloat page's first load, whichever already runs blocking startup work), run `blocking(debloat::offline::finish_pending)` once, silently. No UI.

- [ ] **Step 1: Write failing tests** (in `src/gui/pages/debloat.rs` test module)

```rust
    #[test]
    fn saved_copy_rows_offer_restore_and_delete() {
        let index = debloat::catalog::owner("Microsoft.BingWeather").unwrap();
        let mut state = State::default();
        state.copies.insert(index);
        let actions = row_actions(&state, index, true);
        assert_eq!(actions.primary, Some(Msg::Restore(index)).map(|m| format!("{m:?}")));
        assert!(actions.menu.iter().any(|m| m.contains("AskDelete")));
        state.copies.clear();
        let actions = row_actions(&state, index, true);
        assert!(!actions.menu.iter().any(|m| m.contains("AskDelete")));
    }
```

where `row_actions(state, index, enabled) -> RowActions { primary: Option<String>, menu: Vec<String> }` is a small pure helper the view uses (debug-formatted messages make it testable without rendering). Implement the helper and have `removed_tab` build its trailing widgets from it.

- [ ] **Step 2: Run to see it fail**

Run: `cargo test --locked --all-targets gui::pages::debloat 2>&1 | tail -5` → compile error.

- [ ] **Step 3: Implement the behaviour above**

Follow existing patterns in `debloat.rs`: `blocking(...)` for work, `toast(text, Tone)` for results, `widgets::action` / `widgets::overflow_menu` for buttons, `Sheet` enum for the confirm (add `Sheet::Delete(u16)`), `refresh_removed(state)` after journal changes. Add `Msg::{RestoredOffline(u16, Result<offline::Restored, String>), AskDelete(u16), Delete(u16), Deleted(u16, Result<(), String>), Copies(BTreeSet<u16>, u64)}`. Load copies with:

```rust
fn copies_task() -> Task<Message> {
    Task::perform(
        blocking(|| {
            let copies = debloat::catalog()
                .iter()
                .enumerate()
                .map(|(i, _)| i as u16)
                .filter(|i| debloat::offline::has_copy(*i))
                .collect::<BTreeSet<u16>>();
            (copies, debloat::offline::saved_bytes())
        }),
        |(c, b)| wrap(Msg::Copies(c, b)),
    )
}
```

Batch it with the journal load at page start, and after removal Done, restore, and delete.

`has_copy` opens the store per call; for ~60 catalog entries that's fine (small directory). If clippy or profiling says otherwise, add `offline::copies() -> BTreeSet<u16>` that reads the store once.

- [ ] **Step 4: Add translations** (append rows to `TEXT` in `src/i18n.rs`, all six languages, no em dashes)

```rust
    ["Secblitz keeps a copy, so you can bring these apps back any time, even without internet.", "Secblitz guarda una copia para que puedas recuperar estas aplicaciones en cualquier momento, incluso sin internet.", "Secblitz garde une copie pour que vous puissiez récupérer ces applications à tout moment, même sans internet.", "Secblitz behält eine Kopie, damit Sie diese Apps jederzeit zurückholen können, auch ohne Internet.", "O Secblitz guarda uma cópia para que possa recuperar estas aplicações a qualquer momento, mesmo sem internet.", "Secblitz conserva una copia, così puoi riavere queste app in qualsiasi momento, anche senza internet."],
    ["Saving a copy…", "Guardando una copia…", "Enregistrement d'une copie…", "Kopie wird gespeichert …", "A guardar uma cópia…", "Salvataggio di una copia…"],
    ["Kept: not enough free space to save a copy", "Se mantuvo: no hay espacio libre para guardar una copia", "Conservée : pas assez d'espace libre pour enregistrer une copie", "Behalten: nicht genug freier Speicher für eine Kopie", "Mantida: não há espaço livre suficiente para guardar uma cópia", "Mantenuta: spazio libero insufficiente per salvare una copia"],
    ["Kept: couldn't save a copy", "Se mantuvo: no se pudo guardar una copia", "Conservée : impossible d'enregistrer une copie", "Behalten: Kopie konnte nicht gespeichert werden", "Mantida: não foi possível guardar uma cópia", "Mantenuta: impossibile salvare una copia"],
    ["Can be brought back without internet", "Se puede recuperar sin internet", "Récupérable sans internet", "Ohne Internet wiederherstellbar", "Pode ser recuperada sem internet", "Recuperabile senza internet"],
    ["Saved copies use about {size}.", "Las copias guardadas ocupan unos {size}.", "Les copies enregistrées occupent environ {size}.", "Gespeicherte Kopien belegen etwa {size}.", "As cópias guardadas ocupam cerca de {size}.", "Le copie salvate occupano circa {size}."],
    ["Get it from the Microsoft Store", "Obtenerla en Microsoft Store", "L'obtenir dans le Microsoft Store", "Aus dem Microsoft Store holen", "Obter na Microsoft Store", "Scaricala dal Microsoft Store"],
    ["Delete saved copy", "Eliminar la copia guardada", "Supprimer la copie enregistrée", "Gespeicherte Kopie löschen", "Eliminar a cópia guardada", "Elimina la copia salvata"],
    ["Delete the saved copy?", "¿Eliminar la copia guardada?", "Supprimer la copie enregistrée ?", "Gespeicherte Kopie löschen?", "Eliminar a cópia guardada?", "Eliminare la copia salvata?"],
    ["{name} can then only come back from the Microsoft Store.", "Después, {name} solo podrá volver desde Microsoft Store.", "{name} ne pourra ensuite revenir que depuis le Microsoft Store.", "{name} kann danach nur noch aus dem Microsoft Store zurückkommen.", "Depois, {name} só poderá voltar a partir da Microsoft Store.", "{name} potrà poi tornare solo dal Microsoft Store."],
    ["{name} can't come back after this.", "{name} no podrá volver después de esto.", "{name} ne pourra plus revenir ensuite.", "{name} kann danach nicht mehr zurückkommen.", "{name} não poderá voltar depois disto.", "{name} non potrà più tornare dopo questo."],
    ["Delete", "Eliminar", "Supprimer", "Löschen", "Eliminar", "Elimina"],
    ["Saved copy deleted.", "Copia guardada eliminada.", "Copie enregistrée supprimée.", "Gespeicherte Kopie gelöscht.", "Cópia guardada eliminada.", "Copia salvata eliminata."],
    ["Bringing back {name}…", "Recuperando {name}…", "Récupération de {name}…", "{name} wird zurückgeholt …", "A recuperar {name}…", "Recupero di {name}…"],
    ["{name} is back. Some of its saved data couldn't be put back.", "{name} ha vuelto. Algunos de sus datos guardados no se pudieron recuperar.", "{name} est de retour. Certaines de ses données n'ont pas pu être remises en place.", "{name} ist zurück. Einige gespeicherte Daten konnten nicht wiederhergestellt werden.", "{name} voltou. Alguns dos dados guardados não puderam ser repostos.", "{name} è tornata. Alcuni dati salvati non è stato possibile ripristinarli."],
    ["{name} is already on your PC.", "{name} ya está en tu PC.", "{name} est déjà sur votre PC.", "{name} ist bereits auf Ihrem PC.", "{name} já está no seu PC.", "{name} è già sul tuo PC."],
    ["The saved copy of {name} is damaged, so it can't be brought back from Secblitz.", "La copia guardada de {name} está dañada, así que no se puede recuperar desde Secblitz.", "La copie enregistrée de {name} est endommagée, elle ne peut donc pas être récupérée depuis Secblitz.", "Die gespeicherte Kopie von {name} ist beschädigt und kann nicht über Secblitz zurückgeholt werden.", "A cópia guardada de {name} está danificada, por isso não pode ser recuperada a partir do Secblitz.", "La copia salvata di {name} è danneggiata, quindi non può essere recuperata da Secblitz."],
```

Skip any row whose English source already exists in `TEXT` ("Delete" may exist: `grep -n '\["Delete",' src/i18n.rs`). Match the punctuation of neighbouring German rows ("…" preceded by a space as in existing rows).

- [ ] **Step 5: Tests, clippy, release build**

Run the three Global Constraints commands. Expected: all pass, including `every_gui_key_has_all_five_translations`.

- [ ] **Step 6: Commit**

```bash
git add src/gui/pages/debloat.rs src/i18n.rs
git -c user.email=skeleton22121@gmail.com -c user.name=slay commit -m "Clean up apps: restore from the saved copy, delete saved copies

Claude-Session: https://claude.ai/code/session_019sBc3UDNnKBXUfzPSA9bDy"
```

---

### Task 8: Full VM verification and docs

**Files:**
- Modify: `docs/superpowers/specs/2026-10-05-debloat-offline-backup-design.md` (Feasibility table: add observed results; note the other-account data outcome)
- Modify: `docs/MOTION.md` only if a new animation was added (none expected)

Each step is done on `Secblitz-W11-UI-Test` with the release build launched by `tester-launch.sh` (Tester, split-token admin), screenshots read after every action. Record every result in the commit message body.

- [ ] **Step 1: Remove through the GUI.** Select Weather, News and Solitaire games; Review shows the new copy line; Working shows "Saving a copy…" then "Removing…"; all end "Removed". Check on the VM: `Get-AppxPackage -AllUsers` has none of them; `C:\ProgramData\...\App\AppBackups\` has three family folders and a `frameworks` folder; no `.staging-*` left.
- [ ] **Step 2: Offline restore.** `VBoxManage controlvm 4b70288b-b64d-4796-a725-006da3162d0f setlinkstate1 off`. Removed apps tab: each row says "Can be brought back without internet"; the size line shows. Restore Weather: spinner "Bringing back Weather…", toast "Weather is back on your PC.", row moves to Apps to remove. Launch Weather via `explorer.exe shell:AppsFolder\Microsoft.BingWeather_8wekyb3d8bbwe!App`; it opens. `Get-AppxPackage Microsoft.BingWeather` → `Status Ok`, `SignatureKind Store`, `IsDevelopmentMode False`. Repeat for News and Solitaire. `setlinkstate1 on` afterwards.
- [ ] **Step 3: Data round trip, two accounts.** As Tester and as Administrator, write a marker file `LocalState\secblitz-marker.txt` (random content, note its SHA-256) into `%LOCALAPPDATA%\Packages\Microsoft.BingWeather_8wekyb3d8bbwe` (open Weather once first so the folder exists). Remove Weather as Tester. Check `data\<sid>.bin` exists for both SIDs and contains no marker plaintext (`Select-String`). Restore. Tester: marker present, same hash, owner Tester. Right after restore, `pending.json` lists the Administrator SID (its folder isn't there yet). Sign in as Administrator, open Secblitz once (elevated), close it: the marker is present with the same hash, owned by Administrator, and `pending.json` is gone. Record the result in the spec.
- [ ] **Step 4: Tampered copy.** Remove Weather; flip one byte in a file under `AppBackups\Microsoft.BingWeather_8wekyb3d8bbwe\packages\...` (elevated PowerShell). Restore: toast "The saved copy of Weather is damaged…", then the Store path runs (online) or the offline message shows. Nothing created in WindowsApps (`Test-Path`).
- [ ] **Step 5: Junction attack.** As a new standard user `Std1` (create with `net user Std1 <random> /add`, sign in once, open Weather once): replace `%LOCALAPPDATA%\Packages\Microsoft.BingWeather_8wekyb3d8bbwe\LocalState` with a junction to `C:\Windows\System32\drivers\etc` (`cmd /c mklink /J`). As Tester: remove Weather (backup must not contain `hosts`: decrypt not needed, check `plain_size` is tiny and the removal still succeeds), then restore: Weather back, `C:\Windows\System32\drivers\etc` unchanged (hashes before/after), toast mentions some data couldn't be put back. Delete `Std1` afterwards.
- [ ] **Step 6: Missing framework.** After backing up an app, remove one of its frameworks that no other app uses (`Remove-AppxPackage` on e.g. `Microsoft.NET.Native.Runtime.2.2` if unused; skip and note if every framework is still in use). Restore: framework comes back first, app launches.
- [ ] **Step 7: Low disk.** Fill the system drive to < 2 GB free (`fsutil file createnew C:\fill.bin <bytes>`). Remove an app: row ends "Kept: not enough free space to save a copy", app still installed. Delete `C:\fill.bin`.
- [ ] **Step 8: Delete saved copy.** Removed apps → menu → "Delete saved copy" → confirm. Folder gone, frameworks GC'd when unused, row now shows the Store restore.
- [ ] **Step 9: No console windows.** Run `q-watch.ps1 -Seconds 600` during steps 1–2; it must log no `conhost`/`powershell` window on the desktop session.
- [ ] **Step 10: Dark mode and languages.** Switch to Dark and to Deutsch; screenshot Review, Working, Removed apps, Delete sheet; check nothing is clipped.
- [ ] **Step 11: Update the spec's Feasibility section with what Steps 2–6 observed; commit.**

```bash
git add docs/superpowers/specs/2026-10-05-debloat-offline-backup-design.md
git -c user.email=skeleton22121@gmail.com -c user.name=slay commit -m "Spec: record VM results for offline app restore

Claude-Session: https://claude.ai/code/session_019sBc3UDNnKBXUfzPSA9bDy"
```
