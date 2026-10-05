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

pub const DIR: &str = crate::platform::APP_BACKUPS;
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

/// Windows permissions (SDDL) of a package folder and of its files, as
/// they were in WindowsApps when the copy was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sddl {
    pub dir: String,
    pub file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Package {
    pub full_name: String,
    pub kind: Kind,
    pub files: Vec<FileEntry>,
    /// Original permissions; checked again with `own_sddl` before use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sddl: Option<Sddl>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sddl: Option<Sddl>,
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
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
}

pub fn parse_full_name(full: &str) -> Result<Identity> {
    let parts: Vec<&str> = full.split('_').collect();
    ensure!(parts.len() == 5, "Unexpected package name");
    let (name, version, arch, resource, publisher) =
        (parts[0], parts[1], parts[2], parts[3], parts[4]);
    ensure!(
        plain_token(name, 50) && name.len() >= 3,
        "Unexpected package name"
    );
    let numbers: Vec<&str> = version.split('.').collect();
    ensure!(
        numbers.len() == 4
            && numbers
                .iter()
                .all(|n| n.bytes().all(|b| b.is_ascii_digit()) && n.parse::<u16>().is_ok()),
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

/// `CON`, `NUL`, `COM1`... with or without an extension (any case).
fn reserved_device(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or("").trim_end();
    let up = stem.to_ascii_uppercase();
    matches!(
        up.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ((up.starts_with("COM") || up.starts_with("LPT"))
        && up.len() == 4
        && up.as_bytes()[3].is_ascii_digit())
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
                && !reserved_device(p)
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
    h.len() == 64
        && h.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn check_files(files: &[FileEntry], count: &mut usize, bytes: &mut u64) -> Result<()> {
    for f in files {
        ensure!(valid_relative(&f.path), "Unexpected file in saved copy");
        ensure!(valid_hash(&f.sha256), "Unexpected file check");
        *count += 1;
        *bytes = bytes.saturating_add(f.size);
    }
    ensure!(
        *count <= MAX_FILES && *bytes <= MAX_BYTES,
        "Saved copy too large"
    );
    Ok(())
}

impl Manifest {
    /// Everything restore relies on, re-derived from the compiled catalog.
    pub fn check(&self, index: u16) -> Result<()> {
        ensure!(self.schema == SCHEMA, "Unknown saved copy version");
        ensure!(self.index == index, "Saved copy belongs to another app");
        let (name, publisher) = self.family.rsplit_once('_').context("Unexpected family")?;
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
        let mut seen = std::collections::BTreeSet::new();
        for p in &self.packages {
            ensure!(seen.insert(p.full_name.as_str()), "Duplicate package");
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
        let mut sids = std::collections::BTreeSet::new();
        for d in &self.data {
            ensure!(
                valid_sid(&d.sid)
                    && valid_hash(&d.sha256)
                    && d.plain_size <= MAX_BYTES
                    && sids.insert(d.sid.as_str()),
                "Unexpected saved data"
            );
        }
        Ok(())
    }
}

impl FrameworkCopy {
    pub fn check(&self, full: &str) -> Result<()> {
        ensure!(
            self.schema == SCHEMA && self.full_name == full,
            "Unexpected framework copy"
        );
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
    fn walk(
        root: &Path,
        dir: &Path,
        depth: usize,
        out: &mut Vec<FileEntry>,
        bytes: &mut u64,
    ) -> Result<()> {
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
                out.push(FileEntry {
                    path: rel,
                    size,
                    sha256,
                });
                ensure!(
                    out.len() <= MAX_FILES && *bytes <= MAX_BYTES,
                    "Saved copy too large"
                );
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

/// A single plain folder name that is not one of the store's own folders.
fn valid_family_dir(family: &str) -> bool {
    !family.is_empty()
        && !family.starts_with('.')
        && family != FRAMEWORKS
        && !family.contains(['/', '\\', ':'])
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
            if let Some(rest) = name.strip_prefix(OLD) {
                // An interrupted replace: if the family has no copy, the
                // `.old-` folder is the only complete one, so put it back.
                let family = rest
                    .get(17..)
                    .filter(|_| rest.as_bytes().get(16) == Some(&b'-'));
                if let Some(family) = family.filter(|f| valid_family_dir(f)) {
                    let target = self.family_dir(family);
                    if fs::symlink_metadata(&target).is_err()
                        && fs::rename(e.path(), &target).is_ok()
                    {
                        continue;
                    }
                }
                let _ = fs::remove_dir_all(e.path());
            } else if name.starts_with(STAGING) {
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
        ensure!(
            meta.is_file() && plain(&meta) && meta.len() <= MAX_MANIFEST,
            "Unexpected saved copy"
        );
        Ok(Some(
            serde_json::from_slice(&fs::read(path)?).context("Read saved copy")?,
        ))
    }

    /// The checked manifest stored under `family`, if any.
    pub fn load(&self, family: &str, index: u16) -> Result<Option<Manifest>> {
        ensure!(valid_family_dir(family), "Unexpected family");
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
        parse_full_name(full)?;
        let Some(f) =
            Self::read_json::<FrameworkCopy>(&self.framework_dir(full).join(FRAMEWORK_MANIFEST))?
        else {
            return Ok(None);
        };
        f.check(full)?;
        Ok(Some(f))
    }

    /// Atomically make `staging` the saved copy for `family`.
    pub fn commit(&self, staging: &Path, family: &str) -> Result<()> {
        ensure!(valid_family_dir(family), "Unexpected family");
        ensure!(
            staging
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(STAGING)),
            "Unexpected staging folder"
        );
        ensure!(
            staging.parent() == Some(self.root.as_path()),
            "Unexpected staging folder"
        );
        let target = self.family_dir(family);
        let mut tag = [0u8; 8];
        rand::rngs::OsRng.fill_bytes(&mut tag);
        // The family is part of the name so an interrupted replace can be undone.
        let old = self.root.join(format!("{OLD}{}-{family}", hex(&tag)));
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
        ensure!(valid_family_dir(family), "Unexpected family");
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
                if app.file_name() == FRAMEWORKS {
                    continue;
                }
                // An unreadable manifest might still need its frameworks:
                // keep everything rather than guess.
                match Self::read_json::<Manifest>(&app.path().join(MANIFEST)) {
                    Ok(Some(m)) => used.extend(m.frameworks),
                    Ok(None) => {}
                    Err(_) => return Ok(()),
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

const TRUSTED_INSTALLER: &str = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";

/// Rights that only read and run (FILE_GENERIC_READ|FILE_GENERIC_EXECUTE,
/// GENERIC_READ, GENERIC_EXECUTE and their two-letter forms).
fn read_only(rights: &str) -> bool {
    const ALLOWED: u32 = 0x0012_00a9 | 0x8000_0000 | 0x2000_0000; // FILE_GENERIC_READ|EXECUTE, GENERIC_READ, GENERIC_EXECUTE
    if let Some(hex) = rights.strip_prefix("0x") {
        return u32::from_str_radix(hex, 16).is_ok_and(|m| m & !ALLOWED == 0);
    }
    rights.len().is_multiple_of(2)
        && rights
            .as_bytes()
            .chunks(2)
            .all(|t| matches!(t, b"GR" | b"GX" | b"FR" | b"FX" | b"RC" | b"SW" | b"LO"))
}

fn valid_family(family: &str) -> bool {
    family.rsplit_once('_').is_some_and(|(name, publisher)| {
        plain_token(name, 50)
            && publisher.len() == 13
            && publisher
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    })
}

/// Check package permissions: owned by SYSTEM, Administrators or
/// TrustedInstaller; nobody else may write; no audit section. Returns the
/// text of every quoted literal in conditional ACEs (the app families that
/// `WIN://SYSAPPID Contains` names).
fn check_sddl(sddl: &str) -> Result<Vec<String>> {
    let rest = sddl.strip_prefix("O:").context("Unexpected owner")?;
    let (owner, rest) = rest.split_once("G:").context("Unexpected owner")?;
    ensure!(
        matches!(owner, "SY" | "BA") || owner == TRUSTED_INSTALLER,
        "Unexpected owner"
    );
    let dacl = rest.split_once("D:").context("No DACL")?.1;
    ensure!(!dacl.contains("S:"), "Unexpected audit section");
    let mut rest = dacl.trim_start_matches(|c: char| c.is_ascii_uppercase());
    let mut names = Vec::new();
    while !rest.is_empty() {
        ensure!(rest.starts_with('('), "Unexpected DACL");
        // Conditional ACEs contain parentheses; find the matching close.
        let mut depth = 0usize;
        let mut end = None;
        let mut quoted = false;
        for (i, c) in rest.char_indices() {
            match c {
                '"' => quoted = !quoted,
                '(' if !quoted => depth += 1,
                ')' if !quoted => {
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
        ensure!(
            matches!(kind, "A" | "XA" | "D" | "XD"),
            "Unexpected ACE type"
        );
        let trusted = matches!(sid, "SY") || sid == TRUSTED_INSTALLER;
        ensure!(
            kind.ends_with('D') || trusted || read_only(rights),
            "Permissions grant write access"
        );
        if let Some(condition) = fields.get(6) {
            let parts: Vec<&str> = condition.split('"').collect();
            ensure!(parts.len() % 2 == 1, "Unexpected condition");
            names.extend(parts.iter().skip(1).step_by(2).map(|p| p.to_string()));
        }
        rest = &rest[end + 1..];
    }
    Ok(names)
}

/// A package's own recorded permissions, safe to put back for `family`:
/// any app-only condition must name exactly this family.
pub fn own_sddl(sddl: &str, family: &str) -> Result<String> {
    ensure!(valid_family(family), "Unexpected family");
    let names = check_sddl(sddl)?;
    ensure!(
        names.iter().all(|n| n == family),
        "Permissions name another app"
    );
    Ok(sddl.to_owned())
}

/// Security of a live Microsoft package folder (or file), re-targeted at
/// `to_family`. Used only when a copy has no recorded permissions.
pub fn template_sddl(template: &str, from_family: &str, to_family: &str) -> Result<String> {
    ensure!(valid_family(to_family), "Unexpected family");
    let names = check_sddl(template)?;
    ensure!(
        !names.is_empty() && names.iter().all(|n| n == from_family),
        "Template does not name its app"
    );
    let quoted = format!("\"{from_family}\"");
    Ok(template.replace(&quoted, &format!("\"{to_family}\"")))
}

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
                    sddl: None,
                },
                Package {
                    full_name: "Microsoft.BingWeather_4.54.63045.0_x64__8wekyb3d8bbwe".into(),
                    kind: Kind::Main,
                    files: vec![entry("AppxManifest.xml")],
                    sddl: None,
                },
                Package {
                    full_name:
                        "Microsoft.BingWeather_4.54.63045.0_neutral_split.scale-100_8wekyb3d8bbwe"
                            .into(),
                    kind: Kind::Resource,
                    files: vec![entry("AppxManifest.xml")],
                    sddl: None,
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
        FileEntry {
            path: path.into(),
            size: 1,
            sha256: "0".repeat(64),
        }
    }

    #[test]
    fn parses_full_names() {
        let id = parse_full_name("Microsoft.BingWeather_4.54.63045.0_x64__8wekyb3d8bbwe").unwrap();
        assert_eq!(id.name, "Microsoft.BingWeather");
        assert_eq!(id.resource, "");
        assert_eq!(id.family(), "Microsoft.BingWeather_8wekyb3d8bbwe");
        assert_eq!(
            parse_full_name("Microsoft.BingWeather_4.54.63045.0_neutral_~_8wekyb3d8bbwe")
                .unwrap()
                .resource,
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
        for good in [
            "AppxManifest.xml",
            "Assets/Logo.png",
            "a/b/c.dll",
            "resources.pri",
        ] {
            assert!(valid_relative(good), "{good}");
        }
        for bad in [
            "",
            "/abs",
            "C:/x",
            "a/../b",
            "..",
            "./a",
            "a//b",
            "a\\b",
            "a:stream",
            "a/b ",
            "a/b.",
            "con\u{0}",
            "a/*",
            "a/<b>",
            "a/?",
            &"x".repeat(300),
        ] {
            assert!(!valid_relative(bad), "{bad:?}");
        }
        for bad in ["CON", "nul.txt", "a/Aux.b", "COM1", "lpt9.x", "a/con "] {
            assert!(!valid_relative(bad), "{bad:?}");
        }
        assert!(valid_relative("console.txt") && valid_relative("COM10"));
        let deep = vec!["d"; 40].join("/");
        assert!(!valid_relative(&deep));
    }

    #[test]
    fn sids_are_user_accounts_only() {
        assert!(valid_sid("S-1-5-21-1-2-3-1001"));
        assert!(valid_sid("S-1-5-21-3623811015-3361044348-30300820-1013"));
        for bad in [
            "S-1-5-18",
            "S-1-5-32-544",
            "S-1-5-21-1-2-3",
            "S-1-5-21-1-2-3-x",
            "",
            "S-1-5-21-1-2-3-4-5",
        ] {
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
        m.packages[1].full_name = m.packages[1].full_name.replace("_4.", "_+4.");
        assert!(m.check(index).is_err(), "plus sign in version");

        let mut m = manifest();
        m.packages[1].full_name = m.packages[0].full_name.clone();
        assert!(m.check(index).is_err(), "duplicate package");

        let mut m = manifest();
        m.data.push(m.data[0].clone());
        assert!(m.check(index).is_err(), "duplicate sid");

        let mut m = manifest();
        m.data[0].plain_size = MAX_BYTES + 1;
        assert!(m.check(index).is_err(), "data size cap");

        let mut m = manifest();
        m.packages[1].kind = Kind::Resource;
        assert!(m.check(index).is_err(), "kind must match the resource id");

        let mut m = manifest();
        m.packages[0].files = (0..MAX_FILES + 1)
            .map(|i| entry(&format!("f{i}")))
            .collect();
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
        assert!(store
            .load("Microsoft.BingNews_8wekyb3d8bbwe", news)
            .is_err());
    }

    #[test]
    fn staging_folders_are_never_backups_and_get_cleaned() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join(DIR));
        let staging = store.new_staging().unwrap();
        fs::write(
            staging.join(MANIFEST),
            serde_json::to_vec(&manifest()).unwrap(),
        )
        .unwrap();
        assert!(store.for_index(weather()).is_empty());
        store.clean_staging();
        assert!(!staging.exists());
    }

    #[test]
    fn commit_refuses_bad_family_or_staging() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join(DIR));
        let keep = store.framework_dir("Microsoft.VCLibs.140.00_14.0.33519.0_x64__8wekyb3d8bbwe");
        fs::create_dir_all(&keep).unwrap();
        for bad in ["frameworks", "../x", ".old-1", "a/b", ""] {
            let staging = store.new_staging().unwrap();
            assert!(store.commit(&staging, bad).is_err(), "{bad:?}");
            assert!(staging.exists());
        }
        assert!(keep.exists());
        let fam = store.family_dir("Microsoft.BingWeather_8wekyb3d8bbwe");
        fs::create_dir_all(&fam).unwrap();
        assert!(store
            .commit(&fam, "Microsoft.BingWeather_8wekyb3d8bbwe")
            .is_err());
        assert!(fam.exists());
    }

    #[test]
    fn interrupted_replace_is_restored() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join(DIR));
        let family = "Microsoft.BingWeather_8wekyb3d8bbwe";
        fs::create_dir_all(store.root()).unwrap();
        let old = store.root().join(format!("{OLD}0123456789abcdef-{family}"));
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("keep"), b"1").unwrap();
        let staging = store.new_staging().unwrap();
        store.clean_staging();
        assert!(!staging.exists() && !old.exists());
        assert!(store.family_dir(family).join("keep").exists());

        // With a current copy in place the leftover is just removed.
        let old = store.root().join(format!("{OLD}0123456789abcdef-{family}"));
        fs::create_dir_all(&old).unwrap();
        store.clean_staging();
        assert!(!old.exists() && store.family_dir(family).join("keep").exists());
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

    const DIR_SDDL: &str = "O:SYG:SYD:PAI(XA;;0x1200a9;;;BU;(WIN://SYSAPPID Contains \"Microsoft.WindowsAlarms_8wekyb3d8bbwe\"))(A;;0x1200a9;;;S-1-15-3-1288279408-4010470124-2163985056-447644096-1946037256-752919663-3751275627)(A;OICIIO;GXGR;;;BU)(A;OICIID;FA;;;S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464)(A;OICIID;0x1200a9;;;S-1-15-3-1024-3635283841-2530182609-996808640-1887759898-3848208603-3313616867-983405619-2501854204)(A;OICIID;FA;;;SY)(A;OICIID;0x1200a9;;;BA)(A;OICIID;0x1200a9;;;LS)(A;OICIID;0x1200a9;;;NS)(A;OICIID;0x1200a9;;;RC)";

    #[test]
    fn read_only_refuses_directory_service_bits_that_are_write_on_files() {
        for ok in ["GR", "GRGX", "FRFX", "RC", "SW", "LO", "0x1200a9"] {
            assert!(read_only(ok), "{ok}");
        }
        // LC (0x4) and RP (0x10) are FILE_APPEND_DATA and FILE_WRITE_EA on files.
        for bad in ["LC", "RP", "GRLC", "FRRP", "0x14", "0x1200a9ff"] {
            assert!(!read_only(bad), "{bad}");
        }
    }

    /// Real permissions read on Windows 11 (VCLibs framework, Calculator).
    const FRAMEWORK_DIR: &str = "O:BAG:S-1-5-21-583798214-2395324448-2099448207-513D:AI(A;OICI;0x1200a9;;;BU)(A;OICI;0x1200a9;;;AC)(A;OICI;0x1200a9;;;S-1-15-2-2)(A;OICIID;FA;;;S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464)(A;OICIID;0x1200a9;;;S-1-15-3-1024-3635283841-2530182609-996808640-1887759898-3848208603-3313616867-983405619-2501854204)(A;OICIID;FA;;;SY)(A;CIID;0x1200a9;;;BA)(A;OICIID;0x1200a9;;;LS)(A;OICIID;0x1200a9;;;NS)(A;OICIID;0x1200a9;;;RC)";
    const APP_FILE: &str = "O:SYG:SYD:AI(XA;ID;0x1200a9;;;BU;(WIN://SYSAPPID Contains \"Microsoft.WindowsCalculator_8wekyb3d8bbwe\"))(A;ID;0x1200a9;;;S-1-15-3-466767348-3739614953-2700836392-1801644223-4227750657-1087833535-2488631167)(A;ID;FR;;;BU)(A;ID;FA;;;S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464)(A;ID;0x1200a9;;;S-1-15-3-1024-3635283841-2530182609-996808640-1887759898-3848208603-3313616867-983405619-2501854204)(A;ID;FA;;;SY)(A;ID;0x1200a9;;;LS)(A;ID;0x1200a9;;;NS)(A;ID;0x1200a9;;;RC)";

    #[test]
    fn own_sddl_accepts_real_permissions_for_their_own_package_only() {
        let vclibs = "Microsoft.VCLibs.140.00_8wekyb3d8bbwe";
        let calc = "Microsoft.WindowsCalculator_8wekyb3d8bbwe";
        assert_eq!(own_sddl(FRAMEWORK_DIR, vclibs).unwrap(), FRAMEWORK_DIR);
        assert_eq!(own_sddl(APP_FILE, calc).unwrap(), APP_FILE);
        // An app-only rule naming another app is refused.
        assert!(own_sddl(APP_FILE, "Microsoft.BingWeather_8wekyb3d8bbwe").is_err());
        // Untrusted owner, write grants, audit sections are refused.
        assert!(own_sddl(&APP_FILE.replace("O:SY", "O:BU"), calc).is_err());
        assert!(own_sddl(&format!("{FRAMEWORK_DIR}(A;;FA;;;BU)"), vclibs).is_err());
        assert!(own_sddl(&format!("{FRAMEWORK_DIR}S:(AU;SA;FA;;;WD)"), vclibs).is_err());
        assert!(own_sddl("O:SYG:SYD:(A;;GA;;;WD)", vclibs).is_err());
        assert!(own_sddl(FRAMEWORK_DIR, "bad\"family_8wekyb3d8bbwe").is_err());
        // A parenthesis hidden in a quoted name can't end the ACE early.
        let sneaky = APP_FILE.replace(
            "Microsoft.WindowsCalculator_8wekyb3d8bbwe",
            "x))(A;;FA;;;WD)((",
        );
        assert!(own_sddl(&sneaky, calc).is_err());
    }

    #[test]
    fn template_sddl_swaps_family_and_refuses_write_grants() {
        let out = template_sddl(
            DIR_SDDL,
            "Microsoft.WindowsAlarms_8wekyb3d8bbwe",
            "Microsoft.BingWeather_8wekyb3d8bbwe",
        )
        .unwrap();
        assert!(out.contains("\"Microsoft.BingWeather_8wekyb3d8bbwe\""));
        assert!(!out.contains("WindowsAlarms"));
        // Family must appear in the template.
        assert!(template_sddl(
            DIR_SDDL,
            "Microsoft.Other_8wekyb3d8bbwe",
            "Microsoft.BingWeather_8wekyb3d8bbwe"
        )
        .is_err());
        // Owner must be SYSTEM.
        assert!(template_sddl(
            &DIR_SDDL.replace("O:SY", "O:BU"),
            "Microsoft.WindowsAlarms_8wekyb3d8bbwe",
            "Microsoft.BingWeather_8wekyb3d8bbwe"
        )
        .is_err());
        // Any write grant to someone other than SYSTEM/TrustedInstaller is refused.
        for evil in [
            "(A;;FA;;;BU)",
            "(A;;0x120116;;;WD)",
            "(A;OICI;GA;;;AU)",
            "(A;;WD;;;BA)",
            "(A;;FA;;;S-1-5-21-1-2-3-1001)",
        ] {
            let bad = DIR_SDDL.replacen(
                "(A;OICIID;FA;;;SY)",
                &format!("(A;OICIID;FA;;;SY){evil}"),
                1,
            );
            assert!(
                template_sddl(
                    &bad,
                    "Microsoft.WindowsAlarms_8wekyb3d8bbwe",
                    "Microsoft.BingWeather_8wekyb3d8bbwe"
                )
                .is_err(),
                "{evil}"
            );
        }
        // Target family is validated (no quote injection).
        assert!(template_sddl(
            DIR_SDDL,
            "Microsoft.WindowsAlarms_8wekyb3d8bbwe",
            "Evil\")(A;;FA;;;WD)_8wekyb3d8bbwe"
        )
        .is_err());
    }
}
