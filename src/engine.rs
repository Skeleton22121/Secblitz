//! Serialized, crash-recoverable preference transactions.
//!
//! Windows callers must use platform::state_dir(): its ACL/owner checks and
//! pinned ancestors are the trust boundary (a hostile administrator is excluded).
//! The engine additionally rejects links, opens files without delete sharing on
//! Windows, and never interprets journal data as a path, command, or target.
//! Appends publish a flushed copy-on-write snapshot. Only a recognizable,
//! incomplete final append can be recovered from old JSONL journals; complete
//! malformed records and damaged committed prefixes always fail closed.
//! Recovery retains byte-for-byte evidence before retiring any damaged file.
//! The crash model is an interrupted sequential append / atomic same-directory
//! rename with honored file flushes. Schema 1 has no integrity checksum: it
//! cannot distinguish post-commit truncation or valid-looking media corruption
//! from a crash prefix. It is not a general corruption-repair format.
use crate::model::{
    validate_observation, Authority, Backend, Control, EffectiveFirewall, Finding, InboundAction,
    Observation, Readiness,
};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

const SCHEMA: u32 = 1;
// Exact service descriptor envelopes can exceed 4 KiB. The descriptor parser
// imposes its own bound; the complete WAL remains capped at 1 MiB.
const MAX_LINE: usize = 128 * 1024;
const MAX_WAL: u64 = 1024 * 1024;
const MAX_TRANSACTIONS: usize = 2048;
const LOCK_NAME: &str = "engine.lock";
const MAX_EVIDENCE: usize = 4096;
const LEGACY_UPDATE_FILES: [&str; 5] = [
    "update.lock",
    "update-status.json",
    "update-manifest.json",
    "update-installer.exe",
    "update-worker.exe",
];

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub transaction: Option<String>,
    pub results: Vec<Outcome>,
    pub findings: Vec<Finding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readiness: Option<Readiness>,
}

#[derive(Debug, Default, Serialize)]
pub struct Outcome {
    pub id: String,
    pub title: String,
    pub status: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective: Option<EffectiveFirewall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<Authority>,
}

/// Downcast an Engine::open/operation error to this type to offer diagnostics.
/// Approval alone cannot make an ambiguous original safe: there is deliberately
/// no force-truncate API. Restore a verified journal backup under engine.lock,
/// or obtain independent evidence before designing an explicit recovery action.
#[derive(Debug, Serialize)]
pub struct JournalRecoveryRequired {
    pub transaction: String,
    pub validated_bytes: usize,
    pub reason: &'static str,
}

impl std::fmt::Display for JournalRecoveryRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Journal {} requires review at byte {}: {}; original bytes retained",
            self.transaction, self.validated_bytes, self.reason
        )
    }
}

impl std::error::Error for JournalRecoveryRequired {}

/// Callback arguments are (control id or phase, stable ASCII status). Methods require
/// &mut self because Backend's probes can be stateful. The OS lock lasts through
/// validation, probes, writes, callbacks, and the final findings probe.
pub struct Engine {
    dir: PathBuf,
    backend: Box<dyn Backend>,
    controls: Vec<Control>,
    machine: String,
    storage_failed: bool,
    // Fixtures use isolated stores/backends, not native machine namespaces.
    // Injection exercises the same locked pre-mutation boundary on every host.
    #[cfg(test)]
    mutation_check: Option<Box<MutationCheck>>,
}

#[cfg(test)]
type MutationCheck = dyn Fn(&File) -> Result<()>;

#[cfg(windows)]
fn native_mutation_interlocks(held: &File) -> Result<()> {
    crate::updater::interlock::ensure_others_idle(
        crate::updater::interlock::Activity::Hardening,
        held,
    )
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Record {
    Header {
        schema: u32,
        machine: String,
        transaction: String,
        sequence: u64,
    },
    Prepare {
        id: String,
        #[serde(deserialize_with = "deserialize_before")]
        before: Value,
    },
    Applied {
        id: String,
    },
    Sealed,
    Reverting,
    RestorePending {
        id: String,
    },
    Restored {
        id: String,
    },
    Reverted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Pending,
    Applied,
    Restoring,
    Restored,
}

struct Entry {
    id: String,
    before: Value,
    state: State,
}
struct Transaction {
    name: String,
    sequence: u64,
    entries: Vec<Entry>,
    sealed: bool,
    reverting: bool,
    reverted: bool,
    file: Option<File>,
    // Length of the exact file we created/validated. Never publish a replacement
    // from a file that changed behind this transaction's handle.
    length: u64,
    // Exact validated logical snapshot, and the bytes held on disk. These differ
    // only for a legacy incomplete tail, whose original stays in place until a
    // later COW commit. Never reconstruct a before-image from current probes.
    bytes: Vec<u8>,
    disk_bytes: Vec<u8>,
}

impl Transaction {
    fn incomplete(&self) -> bool {
        !self.sealed || self.reverting || self.bytes != self.disk_bytes
    }
}

// Do not deserialize before images directly as Value: Value silently accepts
// duplicate object keys. Typed parsing also excludes arbitrary restore payloads
// before the id-specific domain check is reached.
fn deserialize_before<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Value, D::Error> {
    #[derive(Deserialize, Serialize)]
    #[serde(deny_unknown_fields)]
    struct Uac {
        present: bool,
        #[serde(deserialize_with = "required_value")]
        value: Option<u32>,
    }
    fn required_value<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<Option<u32>, D::Error> {
        Option::<u32>::deserialize(d)
    }
    /// Extended hardening slice: `{"items": {key: u32 | null}}`. Duplicate
    /// keys are rejected here; id-specific domains are checked by the catalog.
    #[derive(Serialize)]
    #[serde(deny_unknown_fields)]
    struct Items {
        items: std::collections::BTreeMap<String, Option<u32>>,
    }
    impl<'de> Deserialize<'de> for Items {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Raw {
                items: UniqueMap,
            }
            struct UniqueMap(std::collections::BTreeMap<String, Option<u32>>);
            impl<'de> Deserialize<'de> for UniqueMap {
                fn deserialize<D: serde::Deserializer<'de>>(
                    d: D,
                ) -> std::result::Result<Self, D::Error> {
                    struct V;
                    impl<'de> serde::de::Visitor<'de> for V {
                        type Value = UniqueMap;
                        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                            f.write_str("a map of unique hardening items")
                        }
                        fn visit_map<A: serde::de::MapAccess<'de>>(
                            self,
                            mut a: A,
                        ) -> std::result::Result<UniqueMap, A::Error> {
                            let mut map = std::collections::BTreeMap::new();
                            while let Some((k, v)) = a.next_entry::<String, Option<u32>>()? {
                                if map.insert(k, v).is_some() {
                                    return Err(serde::de::Error::custom(
                                        "duplicate hardening item",
                                    ));
                                }
                            }
                            Ok(UniqueMap(map))
                        }
                    }
                    d.deserialize_map(V)
                }
            }
            Ok(Items {
                items: Raw::deserialize(d)?.items.0,
            })
        }
    }
    #[derive(Deserialize, Serialize)]
    #[serde(untagged)]
    enum Before {
        Boolean(bool),
        Inbound(String),
        Uac(Uac),
        Items(Items),
    }
    serde_json::to_value(Before::deserialize(d)?).map_err(serde::de::Error::custom)
}

// Kept here rather than trusting Backend or serialized Control data. This is
// intentionally the same typed domain as platform.rs and the fixed service
// controls in permissions.rs; journal data cannot name arbitrary objects.
fn target(id: &str) -> Result<Value> {
    if let Some(spec) = crate::hardening::spec(id) {
        return Ok(spec.catalog_target());
    }
    Ok(match id {
        "defender.realtime" | "defender.behavior" | "defender.ioav" | "defender.archive" => {
            json!(false)
        }
        "firewall.domain.enabled" | "firewall.private.enabled" | "firewall.public.enabled" => {
            json!(true)
        }
        "firewall.domain.inbound" | "firewall.private.inbound" | "firewall.public.inbound" => {
            json!("Block")
        }
        "uac.enabled" => json!({"present": true, "value": 1}),
        "uac.consent" => json!({"present": true, "value": 5}),
        "installer.always_install_elevated" | "wdigest.use_logon_credential" => {
            json!({"present": true, "value": 0})
        }
        "lsa.restrict_anonymous_sam" | "lsa.limit_blank_password_use" => {
            json!({"present": true, "value": 1})
        }
        "permissions.service.bits" | "permissions.service.wuauserv" => {
            json!("service-dacl-repair-v1")
        }
        _ => bail!("Unknown control id: {id}"),
    })
}

fn permission_control(id: &str) -> bool {
    matches!(
        id,
        "permissions.service.bits" | "permissions.service.wuauserv"
    )
}

fn machine_registry_control(id: &str) -> bool {
    matches!(
        id,
        "installer.always_install_elevated"
            | "lsa.restrict_anonymous_sam"
            | "lsa.limit_blank_password_use"
            | "wdigest.use_logon_credential"
    )
}

/// Catalog targets are fixed. Only the two compiled service IDs derive a write
/// value from an exact, validated before-image; the sentinel is never written.
fn target_for(id: &str, before: &Value) -> Result<Value> {
    if let Some(spec) = crate::hardening::spec(id) {
        return spec.derive_target(before);
    }
    let fixed = target(id)?;
    if permission_control(id) {
        validate_value(id, before)?;
        let repaired = crate::permissions::repair_target(id, before)?;
        validate_value(id, &repaired)?;
        Ok(repaired)
    } else {
        Ok(fixed)
    }
}

fn validate_value(id: &str, value: &Value) -> Result<()> {
    if let Some(spec) = crate::hardening::spec(id) {
        return spec.validate(value);
    }
    let expected = target(id)?;
    if permission_control(id) {
        return crate::permissions::validate_value(id, value);
    } else if expected.is_boolean() {
        ensure!(value.is_boolean(), "Invalid boolean preference for {id}");
    } else if expected.is_string() {
        ensure!(
            matches!(value.as_str(), Some("Block" | "Allow" | "NotConfigured")),
            "Invalid inbound preference"
        );
    } else {
        let obj = value.as_object().context("Invalid UAC preference")?;
        ensure!(
            obj.len() == 2 && obj.contains_key("present") && obj.contains_key("value"),
            "Invalid UAC fields"
        );
        match obj["present"].as_bool() {
            Some(false) => ensure!(
                obj["value"].is_null(),
                "Absent UAC preference must have null value"
            ),
            Some(true) => {
                let max = if id == "uac.consent" { 5 } else { 1 };
                ensure!(
                    obj["value"].as_u64().is_some_and(|n| n <= max),
                    "Invalid UAC DWORD"
                );
            }
            None => bail!("Invalid UAC presence flag"),
        }
    }
    Ok(())
}

fn metadata_safe(m: &Metadata, directory: bool) -> Result<()> {
    ensure!(!m.file_type().is_symlink(), "Journal links are forbidden");
    ensure!(
        if directory { m.is_dir() } else { m.is_file() },
        "Unexpected journal file type"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            directory || m.nlink() == 1,
            "Journal hard links are forbidden"
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            m.file_attributes() & 0x400 == 0,
            "Journal reparse points are forbidden"
        );
    }
    Ok(())
}

fn file_safe(file: &File) -> Result<()> {
    metadata_safe(&file.metadata()?, false)?;
    #[cfg(windows)]
    {
        // std's by-handle link-count metadata is not stable on all supported
        // toolchains. Use the native query for this one additional check.
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        ensure!(
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } != 0,
            "Cannot inspect journal handle: {}",
            std::io::Error::last_os_error()
        );
        ensure!(info.nNumberOfLinks == 1, "Journal hard links are forbidden");
    }
    Ok(())
}

fn open_file(path: &Path, create: bool) -> Result<File> {
    if !create {
        metadata_safe(&fs::symlink_metadata(path)?, false)?;
    }
    let mut options = OpenOptions::new();
    options.read(true).append(true).create_new(create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // OPEN_REPARSE_POINT; share read/write, never delete. The protected
        // directory prevents unprivileged replacement races on other platforms.
        options
            .custom_flags(0x00200000 | 0x80000000)
            .share_mode(0x1 | 0x2);
    }
    let file = options
        .open(path)
        .with_context(|| format!("Open journal {}", path.display()))?;
    file_safe(&file)?;
    Ok(file)
}

// Published updaters staged these exact files in the journal root. Inspect
// metadata only: they are updater-owned data, never journal records or commands.
// In particular, do not request append access to a running installer/worker.
fn validate_update_file(path: &Path) -> Result<()> {
    metadata_safe(&fs::symlink_metadata(path)?, false)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // OPEN_REPARSE_POINT; allow the updater's existing read/write/delete
        // handles. The protected root is the replacement-race trust boundary.
        options.custom_flags(0x00200000).share_mode(0x1 | 0x2 | 0x4);
    }
    file_safe(&options.open(path)?)
}

fn same_file(file: &File, path: &Path) -> Result<()> {
    let path_metadata = fs::symlink_metadata(path)?;
    metadata_safe(&path_metadata, false)?;
    file_safe(file)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let held = file.metadata()?;
        ensure!(
            held.dev() == path_metadata.dev() && held.ino() == path_metadata.ino(),
            "Journal file identity changed"
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let current = open_file(path, false)?;
        let identity = |f: &File| -> Result<(u32, u32, u32)> {
            let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            ensure!(
                unsafe { GetFileInformationByHandle(f.as_raw_handle(), &mut info) } != 0,
                "Cannot inspect journal identity: {}",
                std::io::Error::last_os_error()
            );
            Ok((
                info.dwVolumeSerialNumber,
                info.nFileIndexHigh,
                info.nFileIndexLow,
            ))
        };
        ensure!(
            identity(file)? == identity(&current)?,
            "Journal file identity changed"
        );
    }
    Ok(())
}

fn sync_directory(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(dir)?.sync_all()?;
    // Windows files use WRITE_THROUGH and FlushFileBuffers via sync_all;
    // snapshot publication also uses MOVEFILE_WRITE_THROUGH. Windows does not
    // support the Unix directory-fsync contract.
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

fn journal_name(stem: &str) -> Result<u64> {
    let (sequence, uuid) = stem.split_once('-').context("Invalid journal filename")?;
    let seq: u64 = sequence.parse()?;
    ensure!(
        seq > 0 && sequence == format!("{seq:020}"),
        "Invalid journal sequence"
    );
    ensure!(
        uuid::Uuid::parse_str(uuid)?.to_string() == uuid,
        "Invalid transaction UUID"
    );
    Ok(seq)
}

fn read_bytes(file: &mut File) -> Result<Vec<u8>> {
    file_safe(file)?;
    ensure!(
        file.metadata()?.len() <= MAX_WAL,
        "Journal exceeds size limit"
    );
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    Read::by_ref(file)
        .take(MAX_WAL + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_WAL, "Journal exceeds size limit");
    Ok(bytes)
}

fn record_bytes(record: &Record) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(record)?;
    ensure!(bytes.len() <= MAX_LINE, "Journal record exceeds limit");
    bytes.push(b'\n');
    Ok(bytes)
}

fn publish_snapshot(from: &Path, to: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        ensure!(
            unsafe {
                MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } != 0,
            "Publish journal snapshot: {}",
            std::io::Error::last_os_error()
        );
    }
    #[cfg(not(windows))]
    fs::rename(from, to)?;
    Ok(())
}

// Test-only failures use the real filesystem up to the chosen boundary. A
// thread-local avoids interference with parallel tests and other engine users.
#[cfg(test)]
thread_local! {
    static IO_FAULT: std::cell::RefCell<Option<(&'static str, usize)>> = const { std::cell::RefCell::new(None) };
}

fn io_boundary(point: &'static str) -> Result<()> {
    #[cfg(test)]
    IO_FAULT.with(|fault| {
        let mut fault = fault.borrow_mut();
        if let Some((name, remaining)) = fault.as_mut() {
            if *name == point {
                if *remaining == 0 {
                    *fault = None;
                    bail!("Injected journal I/O failure at {point}");
                }
                *remaining -= 1;
            }
        }
        Ok(())
    })?;
    let _ = point;
    Ok(())
}

#[cfg(test)]
fn write_snapshot_with_fault(writer: &mut impl Write, bytes: &[u8]) -> Result<()> {
    // A short write followed by failure must leave the exact same prefix as
    // the original byte-at-a-time injector, including a count spanning calls.
    // Do not issue one WRITE_THROUGH syscall per byte on Windows: even tests
    // with NO armed fault otherwise perform tens of thousands of disk flushes.
    let cut = IO_FAULT.with(|fault| {
        let mut fault = fault.borrow_mut();
        if let Some(("snapshot_byte", remaining)) = fault.as_mut() {
            if *remaining < bytes.len() {
                let cut = *remaining;
                *fault = None;
                return Some(cut);
            }
            *remaining -= bytes.len();
        }
        None
    });
    if let Some(cut) = cut {
        writer.write_all(&bytes[..cut])?;
        bail!("Injected journal I/O failure at snapshot_byte");
    }
    writer.write_all(bytes)?;
    Ok(())
}

impl Engine {
    pub fn open(dir: PathBuf, backend: Box<dyn Backend>) -> Result<Self> {
        #[cfg(all(windows, not(test)))]
        ensure!(
            dir == crate::platform::state_dir()?,
            "Engine requires the protected platform journal directory"
        );
        // Check existing ancestors before create_dir_all can follow a link.
        for ancestor in dir.ancestors().filter(|p| !p.as_os_str().is_empty()) {
            match fs::symlink_metadata(ancestor) {
                Ok(m) => metadata_safe(&m, true)?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
        }
        fs::create_dir_all(&dir)?;
        metadata_safe(&fs::symlink_metadata(&dir)?, true)?;
        let controls = backend.controls();
        let mut ids = HashSet::new();
        for c in &controls {
            ensure!(ids.insert(c.id.clone()), "Duplicate backend control");
            ensure!(
                c.target == target(&c.id)?,
                "Backend target differs from compiled target"
            );
        }
        let mut engine = Self {
            dir,
            backend,
            controls,
            machine: String::new(),
            storage_failed: false,
            #[cfg(test)]
            mutation_check: None,
        };
        let _lock = engine.lock()?;
        let machine = engine.backend.machine_id()?;
        ensure!(
            !machine.is_empty() && machine.len() <= 256 && !machine.chars().any(char::is_control),
            "Invalid machine identity"
        );
        engine.machine = machine;
        engine.load()?;
        Ok(engine)
    }

    fn lock(&self) -> Result<File> {
        ensure!(
            !self.storage_failed,
            "Journal storage failed; reopen the engine after resolving storage failure"
        );
        metadata_safe(&fs::symlink_metadata(&self.dir)?, true)?;
        let path = self.dir.join(LOCK_NAME);
        let file = match fs::symlink_metadata(&path) {
            Ok(_) => open_file(&path, false)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => match open_file(&path, true) {
                Ok(f) => f,
                Err(e)
                    if e.downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::AlreadyExists) =>
                {
                    open_file(&path, false)?
                }
                Err(e) => return Err(e),
            },
            Err(e) => return Err(e.into()),
        };
        fs2::FileExt::try_lock_exclusive(&file)
            .context("Another Secblitz operation holds the journal lock")?;
        same_file(&file, &path)?;
        Ok(file)
    }

    fn control(&self, id: &str) -> Result<&Control> {
        self.controls
            .iter()
            .find(|c| c.id == id)
            .context("Journal control is not supported by this backend")
    }

    fn mutation_interlocks(&self, held: &File) -> Result<()> {
        #[cfg(test)]
        if let Some(check) = &self.mutation_check {
            return check(held);
        }
        #[cfg(all(windows, not(test)))]
        return native_mutation_interlocks(held);
        #[cfg(any(not(windows), test))]
        {
            let _ = held;
            Ok(())
        }
    }

    fn observe(&mut self, id: &str) -> Result<Observation> {
        let obs = self.backend.observe(id)?;
        validate_value(id, &obs.value)?;
        validate_observation(id, &obs)?;
        Ok(obs)
    }

    fn decode(&self, stem: &str, file: Option<File>, bytes: Vec<u8>) -> Result<Transaction> {
        ensure!(bytes.len() as u64 <= MAX_WAL, "Journal exceeds size limit");
        let seq = journal_name(stem)?;
        let end = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |n| n + 1);
        ensure!(
            end > 0,
            "Missing complete journal header; manual review required"
        );
        let mut records = bytes[..end - 1].split(|b| *b == b'\n');
        let parse = |line: &[u8]| -> Result<Record> {
            ensure!(
                !line.is_empty() && line.len() <= MAX_LINE,
                "Invalid journal record size"
            );
            let record: Record = serde_json::from_slice(line).context("Invalid journal record")?;
            // Serde's internally tagged unit variants otherwise accept
            // extra fields despite deny_unknown_fields on the enum.
            if matches!(
                record,
                Record::Sealed | Record::Reverting | Record::Reverted
            ) {
                let object: serde_json::Map<String, Value> = serde_json::from_slice(line)?;
                ensure!(object.len() == 1, "Unexpected fields in journal marker");
            }
            Ok(record)
        };
        match parse(records.next().context("Missing header")?)? {
            Record::Header {
                schema,
                machine,
                transaction,
                sequence,
            } => {
                ensure!(
                    schema == SCHEMA
                        && machine == self.machine
                        && transaction == stem
                        && sequence == seq,
                    "Journal schema, machine, or transaction identity mismatch"
                );
            }
            _ => bail!("Journal must start with a header"),
        }
        let mut tx = Transaction {
            name: stem.into(),
            sequence: seq,
            entries: Vec::new(),
            sealed: false,
            reverting: false,
            reverted: false,
            file,
            length: bytes.len() as u64,
            bytes: bytes[..end].to_vec(),
            disk_bytes: bytes.clone(),
        };
        for line in records {
            let record = parse(line)?;
            ensure!(!tx.reverted, "Records after transaction completion");
            match record {
                Record::Prepare { id, before } => {
                    self.control(&id)?;
                    validate_value(&id, &before)?;
                    ensure!(
                        before != target_for(&id, &before)?,
                        "Redundant before image"
                    );
                    ensure!(
                        !tx.sealed
                            && !tx.reverting
                            && tx
                                .entries
                                .iter()
                                .all(|e| e.state == State::Applied && e.id != id),
                        "Invalid prepare ordering or duplicate before image"
                    );
                    tx.entries.push(Entry {
                        id,
                        before,
                        state: State::Pending,
                    });
                }
                Record::Applied { id } => {
                    ensure!(!tx.sealed && !tx.reverting, "Apply after seal/revert");
                    let e = tx.entries.last_mut().context("Apply without prepare")?;
                    ensure!(
                        e.id == id && e.state == State::Pending,
                        "Invalid apply result"
                    );
                    e.state = State::Applied;
                }
                Record::Sealed => {
                    ensure!(
                        !tx.sealed
                            && !tx.reverting
                            && tx.entries.iter().all(|e| e.state == State::Applied),
                        "Invalid seal"
                    );
                    tx.sealed = true;
                }
                Record::Reverting => {
                    ensure!(!tx.reverting, "Duplicate revert start");
                    tx.reverting = true;
                }
                Record::RestorePending { id } => {
                    ensure!(tx.reverting, "Restore before revert start");
                    let e = tx
                        .entries
                        .iter_mut()
                        .find(|e| e.id == id)
                        .context("Restore without before image")?;
                    ensure!(e.state != State::Restored, "Restore after completion");
                    e.state = State::Restoring;
                }
                Record::Restored { id } => {
                    ensure!(tx.reverting, "Restore result before revert start");
                    let e = tx
                        .entries
                        .iter_mut()
                        .find(|e| e.id == id)
                        .context("Result without before image")?;
                    ensure!(e.state == State::Restoring, "Restore result without intent");
                    e.state = State::Restored;
                }
                Record::Reverted => {
                    ensure!(
                        tx.reverting && tx.entries.iter().all(|e| e.state == State::Restored),
                        "Premature revert completion"
                    );
                    tx.reverted = true;
                }
                Record::Header { .. } => bail!("Duplicate journal header"),
            }
        }
        if end != bytes.len() && !self.incomplete_tail(&tx, &bytes[end..], &[])? {
            return Err(JournalRecoveryRequired {
                    transaction: stem.into(), validated_bytes: end,
                    reason: "tail is not a provably incomplete legal append (complete JSON without a newline is ambiguous)",
                }.into());
        }
        Ok(tx)
    }

    fn load(&self) -> Result<Vec<Transaction>> {
        let mut transactions = Vec::new();
        let mut staged = Vec::new();
        let mut evidence_count = 0;
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF8 journal filename"))?;
            if name == LOCK_NAME {
                continue;
            }
            if matches!(name.as_str(), "Updates" | "operations" | "Patching" | "App") {
                // Module-owned protected namespaces, never journal payloads.
                // Production platform validation supplies ACL/owner protection.
                metadata_safe(&fs::symlink_metadata(entry.path())?, true)?;
                continue;
            }
            if LEGACY_UPDATE_FILES.contains(&name.as_str()) {
                validate_update_file(&entry.path())?;
                continue;
            }
            if let Some((stem, digest)) = name.split_once(".evidence-") {
                journal_name(stem)?;
                ensure!(
                    digest.len() == 64
                        && digest
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                    "Invalid evidence filename"
                );
                evidence_count += 1;
                ensure!(
                    evidence_count <= MAX_EVIDENCE,
                    "Too many journal evidence files"
                );
                let file = open_file(&entry.path(), false)?;
                ensure!(
                    file.metadata()?.len() <= MAX_WAL,
                    "Oversized journal evidence"
                );
                // Opaque evidence may itself be a partial failed copy. It is
                // never a source of originals or a replacement journal.
                continue;
            }
            if let Some(stem) = name.strip_suffix(".jsonl.next") {
                journal_name(stem)?;
                ensure!(staged.is_empty(), "Multiple unpublished journal snapshots");
                let mut file = open_file(&entry.path(), false)?;
                let bytes = read_bytes(&mut file)?;
                staged.push((stem.to_owned(), file, bytes));
                continue;
            }
            ensure!(
                transactions.len() < MAX_TRANSACTIONS,
                "Too many journal transactions"
            );
            let stem = name
                .strip_suffix(".jsonl")
                .context("Unexpected journal entry")?;
            journal_name(stem)?;
            let mut file = open_file(&entry.path(), false)?;
            let bytes = read_bytes(&mut file)?;
            transactions.push(self.decode(stem, Some(file), bytes)?);
        }
        transactions.sort_by_key(|t| t.sequence);
        ensure!(
            transactions
                .windows(2)
                .all(|w| w[0].sequence < w[1].sequence),
            "Duplicate transaction sequence"
        );
        // Validate the whole active stack before any caller probes or replays.
        // Only the newest active batch can be incomplete; originals must have
        // exactly one active owner, even when each WAL is valid in isolation.
        let active: Vec<_> = transactions.iter().filter(|t| !t.reverted).collect();
        let mut owners = HashSet::new();
        for (i, tx) in active.iter().enumerate() {
            ensure!(
                i + 1 == active.len() || !tx.incomplete(),
                "Incomplete transaction precedes another active transaction"
            );
            for entry in &tx.entries {
                ensure!(
                    owners.insert(&entry.id),
                    "Duplicate active control owner; journal history is invalid"
                );
            }
        }
        // Do not hide an older torn append behind subsequent history. Validate
        // the entire directory/active stack and all staging before any recovery.
        for tx in &transactions {
            ensure!(
                tx.bytes == tx.disk_bytes
                    || !transactions
                        .iter()
                        .any(|later| later.sequence > tx.sequence && !later.reverted),
                "Incomplete append precedes another active transaction"
            );
            if tx.bytes != tx.disk_bytes {
                ensure!(
                    self.incomplete_tail(tx, &tx.disk_bytes[tx.bytes.len()..], &transactions)?,
                    "Incomplete append conflicts with another active control owner"
                );
            }
        }
        for (stem, _, bytes) in &staged {
            self.validate_staged(stem, bytes, &transactions)?;
        }
        for tx in &transactions {
            if tx.bytes != tx.disk_bytes {
                self.preserve_evidence(&tx.name, &tx.disk_bytes)?;
            }
        }
        for (stem, mut file, bytes) in staged {
            let path = self.dir.join(format!("{stem}.jsonl.next"));
            self.preserve_evidence(&stem, &bytes)?;
            same_file(&file, &path)?;
            ensure!(
                read_bytes(&mut file)? == bytes,
                "Unpublished snapshot changed during recovery"
            );
            drop(file); // Windows handles deny delete until explicitly released.
            io_boundary("retire_stage")?;
            fs::remove_file(path)?;
            sync_directory(&self.dir)?;
        }
        Ok(transactions)
    }

    // Only strict canonical prefixes of legal next records qualify. In
    // particular serde's EOF classification alone is insufficient: unknown
    // controls, invalid domains and invalid transitions must not disappear.
    fn owned_elsewhere(tx: &Transaction, id: &str, transactions: &[Transaction]) -> bool {
        transactions.iter().any(|other| {
            other.name != tx.name && !other.reverted && other.entries.iter().any(|e| e.id == id)
        })
    }

    fn incomplete_tail(
        &self,
        tx: &Transaction,
        tail: &[u8],
        transactions: &[Transaction],
    ) -> Result<bool> {
        if tail.is_empty() || tail.len() > MAX_LINE || tx.reverted {
            return Ok(false);
        }
        if !serde_json::from_slice::<Value>(tail).is_err_and(|e| e.is_eof()) {
            return Ok(false);
        }
        let mut candidates = vec![Record::Sealed, Record::Reverting, Record::Reverted];
        for entry in &tx.entries {
            candidates.push(Record::Applied {
                id: entry.id.clone(),
            });
            candidates.push(Record::RestorePending {
                id: entry.id.clone(),
            });
            candidates.push(Record::Restored {
                id: entry.id.clone(),
            });
        }
        for control in &self.controls {
            if Self::owned_elsewhere(tx, &control.id, transactions) {
                continue;
            }
            // Finite raw preference domains only. An incomplete legacy ACL
            // original cannot be inferred; it requires review. COW staging has
            // an independent committed snapshot and handles ACLs below.
            let values = if permission_control(&control.id)
                || crate::hardening::is_hardening(&control.id)
            {
                Vec::new()
            } else if target(&control.id)?.is_boolean() {
                vec![json!(true), json!(false)]
            } else if target(&control.id)?.is_string() {
                vec![json!("Allow"), json!("Block"), json!("NotConfigured")]
            } else {
                let mut values = vec![json!({"present":false,"value":null})];
                values.extend(
                    (0..=if control.id == "uac.consent" { 5 } else { 1 })
                        .map(|n| json!({"present":true,"value":n})),
                );
                values
            };
            for before in values {
                candidates.push(Record::Prepare {
                    id: control.id.clone(),
                    before,
                });
            }
        }
        for record in candidates {
            let bytes = record_bytes(&record)?;
            if bytes.starts_with(tail) && tail.len() < bytes.len() - 1 {
                let mut complete = tx.bytes.clone();
                complete.extend(bytes);
                if self.decode(&tx.name, None, complete).is_ok() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn validate_staged(
        &self,
        stem: &str,
        bytes: &[u8],
        transactions: &[Transaction],
    ) -> Result<()> {
        let seq = journal_name(stem)?;
        let Some(tx) = transactions.iter().find(|t| t.name == stem) else {
            ensure!(
                transactions.last().is_none_or(|tx| seq > tx.sequence),
                "Staged header is not newest"
            );
            ensure!(
                transactions
                    .iter()
                    .all(|tx| tx.reverted || !tx.incomplete()),
                "Staged header follows incomplete transaction"
            );
            let expected = record_bytes(&Record::Header {
                schema: SCHEMA,
                machine: self.machine.clone(),
                transaction: stem.into(),
                sequence: seq,
            })?;
            ensure!(
                expected.starts_with(bytes),
                "Invalid unpublished journal header or machine mismatch"
            );
            return Ok(());
        };
        ensure!(
            !tx.reverted
                && !transactions
                    .iter()
                    .any(|later| later.sequence > tx.sequence && !later.reverted),
            "Unpublished append is not newest active transaction"
        );
        let prefix_len = bytes.len().min(tx.bytes.len());
        ensure!(
            bytes[..prefix_len] == tx.bytes[..prefix_len],
            "Corrupt committed prefix in unpublished snapshot"
        );
        if bytes.len() <= tx.bytes.len() {
            return Ok(());
        }
        let tail = &bytes[tx.bytes.len()..];
        ensure!(tail.len() <= MAX_LINE + 1, "Invalid journal record size");
        if tail.ends_with(b"\n") || serde_json::from_slice::<Value>(tail).is_ok() {
            ensure!(
                !tail[..tail.len() - usize::from(tail.ends_with(b"\n"))].contains(&b'\n'),
                "Multiple unpublished records"
            );
            let mut complete = bytes.to_vec();
            if !complete.ends_with(b"\n") {
                complete.push(b'\n');
            }
            let next = self.decode(stem, None, complete)?;
            ensure!(
                next.entries.iter().all(|entry| !Self::owned_elsewhere(
                    tx,
                    &entry.id,
                    transactions
                )),
                "Unpublished record duplicates an active control owner"
            );
            return Ok(());
        }
        if self.incomplete_tail(tx, tail, transactions)? {
            return Ok(());
        }
        // An unpublished ACL Prepare has never authorized a backend write. Its
        // already-copied committed prefix must match exactly, and its partial
        // string must have the canonical fixed id/envelope and hex syntax. A
        // completed descriptor is always checked by decode, never discarded as
        // an incomplete string. Legacy ACL tails do not get this exception.
        if !tx.sealed && !tx.reverting && tx.entries.iter().all(|e| e.state == State::Applied) {
            for c in self.controls.iter().filter(|c| {
                permission_control(&c.id)
                    && !tx.entries.iter().any(|e| e.id == c.id)
                    && !Self::owned_elsewhere(tx, &c.id, transactions)
            }) {
                let prefix = format!(
                    "{{\"kind\":\"prepare\",\"id\":\"{}\",\"before\":\"dacl-v1:",
                    c.id
                );
                if prefix.as_bytes().starts_with(tail) {
                    return Ok(());
                }
                if let Some(hex) = tail.strip_prefix(prefix.as_bytes()) {
                    if hex.len() <= 32 * 1024
                        && hex
                            .iter()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
                    {
                        return Ok(());
                    }
                    // Only the closing object brace/newline may still be
                    // missing after a complete descriptor string.
                    if tail.ends_with(b"\"") {
                        let mut complete = bytes.to_vec();
                        complete.extend(b"}\n");
                        self.decode(stem, None, complete)?;
                        return Ok(());
                    }
                }
            }
        }
        bail!("Invalid incomplete unpublished record; manual review required")
    }

    fn preserve_evidence(&self, stem: &str, bytes: &[u8]) -> Result<()> {
        let digest = hex::encode(Sha256::digest(bytes));
        let path = self.dir.join(format!("{stem}.evidence-{digest}"));
        let mut file = match fs::symlink_metadata(&path) {
            Ok(_) => open_file(&path, false)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut count = 0;
                for entry in fs::read_dir(&self.dir)? {
                    if entry?.file_name().to_string_lossy().contains(".evidence-") {
                        count += 1;
                    }
                }
                ensure!(count < MAX_EVIDENCE, "Too many journal evidence files");
                open_file(&path, true)?
            }
            Err(e) => return Err(e.into()),
        };
        // Resume a failed evidence copy only if every existing byte matches.
        // Never overwrite an evidence file, even on an explicit retry.
        let existing = read_bytes(&mut file)?;
        ensure!(
            bytes.starts_with(&existing),
            "Journal recovery evidence mismatch"
        );
        io_boundary("evidence_write")?;
        file.write_all(&bytes[existing.len()..])?;
        file.flush()?;
        io_boundary("evidence_sync")?;
        file.sync_all()?;
        same_file(&file, &path)?;
        io_boundary("evidence_directory")?;
        sync_directory(&self.dir)
    }

    fn append(&mut self, tx: &mut Transaction, record: Record) -> Result<()> {
        let result = (|| {
            let path = self.dir.join(format!("{}.jsonl", tx.name));
            if let Some(file) = tx.file.as_mut() {
                same_file(file, &path)?;
                ensure!(
                    file.metadata()?.len() == tx.length,
                    "Journal length changed since validation"
                );
                ensure!(
                    read_bytes(file)? == tx.disk_bytes,
                    "Journal bytes changed since validation"
                );
            } else {
                ensure!(
                    tx.length == 0 && !path.try_exists()?,
                    "Missing journal handle"
                );
            }
            let mut bytes = tx.bytes.clone();
            bytes.extend(record_bytes(&record)?);
            ensure!(bytes.len() as u64 <= MAX_WAL, "Journal is full");
            if tx.bytes != tx.disk_bytes {
                self.preserve_evidence(&tx.name, &tx.disk_bytes)?;
            }
            let stage_path = self.dir.join(format!("{}.jsonl.next", tx.name));
            io_boundary("snapshot_create")?;
            let mut staged = open_file(&stage_path, true)?;
            io_boundary("snapshot_write")?;
            // Inject an exact short-write prefix using bulk I/O. Flush/replace
            // boundaries and production's single write_all remain unchanged.
            #[cfg(test)]
            write_snapshot_with_fault(&mut staged, &bytes)?;
            #[cfg(not(test))]
            staged.write_all(&bytes)?;
            staged.flush()?;
            io_boundary("snapshot_sync")?;
            staged.sync_all()?;
            same_file(&staged, &stage_path)?;
            if let Some(file) = tx.file.as_mut() {
                same_file(file, &path)?;
                ensure!(
                    read_bytes(file)? == tx.disk_bytes,
                    "Journal changed during snapshot write"
                );
            }
            drop(staged);
            // The shared engine lock and protected directory remain pinned
            // through the narrow Windows close/rename/reopen window.
            drop(tx.file.take());
            io_boundary("snapshot_replace")?;
            publish_snapshot(&stage_path, &path)?;
            io_boundary("snapshot_directory")?;
            sync_directory(&self.dir)?;
            io_boundary("snapshot_reopen")?;
            tx.file = Some(open_file(&path, false)?);
            tx.length = bytes.len() as u64;
            tx.disk_bytes = bytes.clone();
            tx.bytes = bytes;
            Ok(())
        })();
        if result.is_err() {
            self.storage_failed = true;
        }
        result
    }

    fn durable(&mut self, transactions: &[Transaction]) -> Result<()> {
        let result = (|| {
            for tx in transactions {
                tx.file
                    .as_ref()
                    .context("Missing journal handle")?
                    .sync_all()?;
            }
            sync_directory(&self.dir)
        })();
        if result.is_err() {
            self.storage_failed = true;
        }
        result
    }

    fn create(&mut self, sequence: u64) -> Result<Transaction> {
        let name = format!("{sequence:020}-{}", uuid::Uuid::new_v4());
        let mut tx = Transaction {
            name: name.clone(),
            sequence,
            entries: Vec::new(),
            sealed: false,
            reverting: false,
            reverted: false,
            file: None,
            length: 0,
            bytes: Vec::new(),
            disk_bytes: Vec::new(),
        };
        self.append(
            &mut tx,
            Record::Header {
                schema: SCHEMA,
                machine: self.machine.clone(),
                transaction: name,
                sequence,
            },
        )?;
        if let Err(e) = sync_directory(&self.dir) {
            self.storage_failed = true;
            return Err(e);
        }
        Ok(tx)
    }

    fn outcome(c: &Control, status: &str, detail: impl Into<String>) -> Outcome {
        Outcome {
            id: c.id.clone(),
            title: c.title.clone(),
            status: status.into(),
            detail: detail.into(),
            ..Outcome::default()
        }
    }

    fn observed_outcome(
        c: &Control,
        status: &str,
        detail: impl Into<String>,
        observation: &Observation,
    ) -> Outcome {
        Outcome {
            effective: observation.effective,
            authority: observation.authority,
            ..Self::outcome(c, status, detail)
        }
    }

    fn readiness(&mut self, callback: &mut impl FnMut(&str, &str)) -> Readiness {
        callback("readiness", "pending");
        let readiness = self.backend.readiness();
        callback("readiness", "complete");
        readiness
    }

    fn findings(&mut self) -> Vec<Finding> {
        // Findings are assessment, not mutation acknowledgment. A failed final
        // transport/probe must not discard already durable operation outcomes.
        self.backend.findings().unwrap_or_else(|e| {
            vec![Finding {
                title: "Assessment unavailable".into(),
                status: "unknown".into(),
                detail: format!("Findings could not be collected: {e:#}"),
            }]
        })
    }

    fn journal_finding(tx: &Transaction) -> Finding {
        let pending = tx.incomplete();
        Finding {
            title: "Journal recovery".into(),
            status: if pending { "pending" } else { "info" }.into(),
            detail: if pending {
                format!("Transaction {} has incomplete apply or rollback; use revert to resolve its recorded preferences before applying again.", tx.name)
            } else {
                format!("Transaction {} remains unreverted; use revert to restore its recorded preferences.", tx.name)
            },
        }
    }

    pub fn audit(&mut self) -> Result<Report> {
        self.audit_with_progress(|_, _| {})
    }

    /// Immutable compiled/validated catalog; reading it performs no probes.
    pub fn available_controls(&self) -> &[Control] {
        &self.controls
    }

    /// Each observation emits its outcome. The separate readiness and findings
    /// phases each emit (phase, "pending") followed by (phase, "complete").
    pub fn audit_with_progress(&mut self, mut callback: impl FnMut(&str, &str)) -> Result<Report> {
        let _lock = self.lock()?;
        let transactions = self.load()?;
        let active = transactions.iter().rev().find(|t| !t.reverted);
        let mut report = Report {
            transaction: active.map(|t| t.name.clone()),
            results: Vec::new(),
            findings: Vec::new(),
            readiness: None,
        };
        for c in self.controls.clone() {
            let result = match self.observe(&c.id) {
                Ok(o) => match assessment_status(&c.id, &o) {
                    Ok(status) => Self::observed_outcome(&c, status, &o.reason, &o),
                    Err(e) => Self::observed_outcome(&c, "error", format!("{e:#}"), &o),
                },
                Err(e) => Self::outcome(&c, "error", format!("{e:#}")),
            };
            callback(&result.id, &result.status);
            report.results.push(result);
        }
        report.readiness = Some(self.readiness(&mut callback));
        callback("findings", "pending");
        report.findings = self.findings();
        callback("findings", "complete");
        if let Some(tx) = active {
            report.findings.push(Self::journal_finding(tx));
        }
        Ok(report)
    }

    pub fn history(&mut self) -> Result<Vec<String>> {
        let _lock = self.lock()?;
        Ok(self
            .load()?
            .into_iter()
            .rev()
            .map(|tx| {
                format!(
                    "{} {}",
                    tx.name,
                    if tx.reverted {
                        "reverted"
                    } else if tx.reverting {
                        "reverting"
                    } else if !tx.incomplete() {
                        "applied"
                    } else {
                        "pending"
                    }
                )
            })
            .collect())
    }

    pub fn apply(&mut self, callback: impl FnMut(&str, &str)) -> Result<Report> {
        self.apply_impl(None, callback)
    }

    /// IDs only: no audit snapshot or caller-supplied target is trusted.
    /// Invalid selections fail before locking, probing, or modifying the WAL.
    pub fn apply_selected(
        &mut self,
        ids: &[String],
        callback: impl FnMut(&str, &str),
    ) -> Result<Report> {
        ensure!(!ids.is_empty(), "Select at least one control");
        let mut selected = HashSet::new();
        for id in ids {
            ensure!(
                selected.insert(id.as_str()),
                "Duplicate selected control: {id}"
            );
            self.control(id)
                .with_context(|| format!("Unknown selected control: {id}"))?;
        }
        self.apply_impl(Some(&selected), callback)
    }

    fn apply_impl(
        &mut self,
        selected: Option<&HashSet<&str>>,
        mut callback: impl FnMut(&str, &str),
    ) -> Result<Report> {
        let _lock = self.lock()?;
        self.mutation_interlocks(&_lock)?;
        let transactions = self.load()?;
        let controls: Vec<_> = self
            .controls
            .iter()
            .filter(|c| selected.is_none_or(|ids| ids.contains(c.id.as_str())))
            .cloned()
            .collect();
        let mut report = Report {
            transaction: None,
            results: Vec::new(),
            findings: Vec::new(),
            readiness: None,
        };
        if let Some(tx) = transactions
            .iter()
            .rev()
            .find(|t| !t.reverted && (selected.is_none() || t.incomplete()))
        {
            // Never refresh before images, even if an earlier apply only partly
            // completed. Recovery is explicitly revert, not an implicit write.
            report.transaction = Some(tx.name.clone());
            for c in controls {
                let result = if tx.incomplete() {
                    Self::outcome(
                        &c,
                        "pending",
                        "Revert the active transaction before applying again",
                    )
                } else {
                    match self.observe(&c.id).and_then(|o| {
                        let entry = tx.entries.iter().find(|e| e.id == c.id);
                        let expected = if entry.is_none() && firewall_control(&c.id) {
                            firewall_protected(&o)?.then(|| o.value.clone())
                        } else if entry.is_none() && permission_control(&c.id) && !o.eligible {
                            None
                        } else {
                            Some(target_for(&c.id, entry.map_or(&o.value, |e| &e.before))?)
                        };
                        Ok((expected, o))
                    }) {
                        Ok((expected, o))
                            if expected.as_ref()
                                == Some(&scope(
                                    &c.id,
                                    &o.value,
                                    tx.entries
                                        .iter()
                                        .find(|e| e.id == c.id)
                                        .map(|e| &e.before),
                                )) =>
                        {
                            Self::observed_outcome(
                                &c,
                                "unchanged",
                                "Target preference already present; original before image retained",
                                &o,
                            )
                        }
                        Ok((_, o)) if tx.entries.iter().any(|e| e.id == c.id) => {
                            Self::observed_outcome(
                                &c,
                                "conflict",
                                "Preference drifted; original before image retained",
                                &o,
                            )
                        }
                        Ok((_, o)) => Self::observed_outcome(
                            &c,
                            "skipped",
                            "Revert the active transaction before starting another apply",
                            &o,
                        ),
                        Err(e) => Self::outcome(&c, "error", format!("{e:#}")),
                    }
                };
                callback(&result.id, &result.status);
                report.results.push(result);
            }
            report.findings = self.findings();
            report.findings.push(Self::journal_finding(tx));
            return Ok(report);
        }
        // Preflight every selected existing owner before any new intent/write.
        // Compare against the original-derived exact target, including ACLs.
        let mut owned = Vec::new();
        for c in &controls {
            if let Some(entry) = transactions
                .iter()
                .filter(|t| !t.reverted)
                .flat_map(|t| &t.entries)
                .find(|e| e.id == c.id)
            {
                let expected = target_for(&c.id, &entry.before)?;
                let result = match self.observe(&c.id) {
                    Ok(o) if scope(&c.id, &o.value, Some(&entry.before)) == expected => Self::observed_outcome(
                        c,
                        "unchanged",
                        "Target preference already present; original before image retained",
                        &o,
                    ),
                    Ok(o) => Self::observed_outcome(
                        c,
                        "conflict",
                        "Preference drifted; original before image retained",
                        &o,
                    ),
                    Err(e) => Self::outcome(c, "error", format!("{e:#}")),
                };
                owned.push(result);
            }
        }
        if owned.iter().any(|r| r.status != "unchanged") {
            for c in &controls {
                let result = if let Some(i) = owned.iter().position(|r| r.id == c.id) {
                    owned.remove(i)
                } else {
                    Self::outcome(
                        c,
                        "skipped",
                        "Selected batch blocked by an owned control conflict or probe failure",
                    )
                };
                callback(&result.id, &result.status);
                report.results.push(result);
            }
            report.findings = self.findings();
            return Ok(report);
        }
        let readiness = self.readiness(&mut callback);
        let blocked = readiness.blocks_repairs();
        report.readiness = Some(readiness);
        if blocked {
            for c in &controls {
                let result = if let Some(i) = owned.iter().position(|r| r.id == c.id) {
                    owned.remove(i)
                } else {
                    Self::outcome(c, "skipped", "Repair readiness blocks new changes")
                };
                callback(&result.id, &result.status);
                report.results.push(result);
            }
            report.findings = self.findings();
            return Ok(report);
        }
        self.durable(&transactions)?;
        ensure!(
            transactions.len() < MAX_TRANSACTIONS,
            "Too many journal transactions"
        );
        let sequence = transactions
            .last()
            .map_or(Some(1), |t| t.sequence.checked_add(1))
            .context("Transaction sequence exhausted")?;
        let mut tx: Option<Transaction> = None;
        for c in controls {
            if let Some(i) = owned.iter().position(|r| r.id == c.id) {
                let result = owned.remove(i);
                callback(&result.id, &result.status);
                report.results.push(result);
                continue;
            }
            // Before Prepare there is no uncertain write to recover. A runtime
            // unsupported control must not strand earlier selected successes.
            let observed = self.observe(&c.id).and_then(|o| {
                let protected = firewall_control(&c.id) && firewall_protected(&o)?;
                let expected = if permission_control(&c.id) && !o.eligible {
                    None
                } else {
                    Some(target_for(&c.id, &o.value)?)
                };
                Ok((o, expected, protected))
            });
            let (observation, expected, protected) = match observed {
                Ok(value) => value,
                Err(e) if selected.is_some() => {
                    let result = Self::outcome(&c, "error", format!("{e:#}"));
                    callback(&result.id, &result.status);
                    report.results.push(result);
                    continue;
                }
                Err(e) => return Err(e),
            };
            let Some(expected) = expected else {
                let result =
                    Self::observed_outcome(&c, "skipped", &observation.reason, &observation);
                callback(&result.id, &result.status);
                report.results.push(result);
                continue;
            };
            let result = if firewall_control(&c.id) && !observation.eligible {
                Self::observed_outcome(&c, "skipped", &observation.reason, &observation)
            } else if protected || observation.value == expected {
                Self::observed_outcome(
                    &c,
                    "unchanged",
                    "Target preference already present",
                    &observation,
                )
            } else if !apply_eligible(&c.id, &observation) {
                Self::observed_outcome(
                    &c,
                    "skipped",
                    if observation.eligible {
                        "Preserving absent or already-safe machine preference".into()
                    } else {
                        observation.reason.clone()
                    },
                    &observation,
                )
            } else {
                if tx.is_none() {
                    tx = Some(self.create(sequence)?);
                }
                let tx = tx.as_mut().unwrap();
                report.transaction = Some(tx.name.clone());
                // Intent is durable before the final eligibility/read gate. A
                // failure or race at that gate is still safely recoverable.
                self.append(
                    tx,
                    Record::Prepare {
                        id: c.id.clone(),
                        before: observation.value.clone(),
                    },
                )?;
                let fresh = self.observe(&c.id)?;
                ensure!(
                    apply_eligible(&c.id, &fresh) && fresh.value == observation.value,
                    "{} changed or became ineligible after prepare; revert transaction {}",
                    c.id,
                    tx.name
                );
                if let Err(e) = self.backend.write(&c.id, &expected) {
                    callback(&c.id, "error");
                    return Err(e.context(format!(
                        "Apply {} has unknown outcome; pending transaction {} retained",
                        c.id, tx.name
                    )));
                }
                // Verify against the durable original's target, not a target
                // recomputed from readback (which would accept safe ACL drift).
                // Failure leaves Prepare pending even if the backend returned Ok.
                let readback = self.observe(&c.id)?;
                ensure!(
                    readback.value == expected,
                    "Apply {} readback differs from recorded target; pending transaction {} retained",
                    c.id,
                    tx.name
                );
                if firewall_control(&c.id) {
                    ensure!(
                        firewall_protected(&readback)?,
                        "Apply {} effective protection is unverified; pending transaction {} retained",
                        c.id,
                        tx.name
                    );
                }
                self.append(tx, Record::Applied { id: c.id.clone() })?;
                Self::observed_outcome(
                    &c,
                    "applied",
                    if c.reboot {
                        "Preference applied; restart required"
                    } else {
                        "Preference applied"
                    },
                    &readback,
                )
            };
            callback(&result.id, &result.status);
            report.results.push(result);
        }
        if let Some(tx) = tx.as_mut() {
            self.append(tx, Record::Sealed)?;
        }
        report.findings = self.findings();
        Ok(report)
    }

    pub fn revert(&mut self, mut callback: impl FnMut(&str, &str)) -> Result<Report> {
        let _lock = self.lock()?;
        self.mutation_interlocks(&_lock)?;
        let mut transactions = self.load()?;
        self.durable(&transactions)?;
        let mut report = Report {
            transaction: None,
            results: Vec::new(),
            findings: Vec::new(),
            readiness: None,
        };
        if let Some(tx) = transactions.iter_mut().rev().find(|t| !t.reverted) {
            report.transaction = Some(tx.name.clone());
            if !tx.reverting {
                self.append(tx, Record::Reverting)?;
                tx.reverting = true;
            }
            for i in (0..tx.entries.len()).rev() {
                if tx.entries[i].state == State::Restored {
                    continue;
                }
                let id = tx.entries[i].id.clone();
                let before = tx.entries[i].before.clone();
                let c = self.control(&id)?.clone();
                let expected = target_for(&id, &before)?;
                let observation = self.observe(&id)?;
                let before_eff = scope(&id, &before, Some(&observation.value));
                let expected_eff = scope(&id, &expected, Some(&observation.value));
                let seen = scope(&id, &observation.value, Some(&before_eff));
                let result = if seen == before_eff {
                    // Includes a prepared apply that never wrote, and a restore
                    // that crashed between its write and result flush.
                    if tx.entries[i].state != State::Restoring {
                        self.append(tx, Record::RestorePending { id: id.clone() })?;
                    }
                    self.append(tx, Record::Restored { id: id.clone() })?;
                    tx.entries[i].state = State::Restored;
                    Self::observed_outcome(
                        &c,
                        "unchanged",
                        "Original preference already present",
                        &observation,
                    )
                } else if seen != expected_eff {
                    Self::observed_outcome(
                        &c,
                        "conflict",
                        "Preference differs from both target and before image; no write performed",
                        &observation,
                    )
                } else if !restore_eligible(&c, &observation) {
                    Self::observed_outcome(&c, "skipped", &observation.reason, &observation)
                } else {
                    if tx.entries[i].state != State::Restoring {
                        self.append(tx, Record::RestorePending { id: id.clone() })?;
                        tx.entries[i].state = State::Restoring;
                    }
                    let fresh = self.observe(&id)?;
                    if scope(&id, &fresh.value, Some(&before_eff)) != expected_eff {
                        Self::observed_outcome(
                            &c,
                            "conflict",
                            "Preference changed immediately before restore; no write performed",
                            &fresh,
                        )
                    } else if !restore_eligible(&c, &fresh) {
                        Self::observed_outcome(&c, "skipped", &fresh.reason, &fresh)
                    } else {
                        if let Err(e) = self.backend.write(&id, &before_eff) {
                            callback(&id, "error");
                            return Err(e.context(format!(
                                "Restore {id} has unknown outcome; pending transaction {} retained",
                                tx.name
                            )));
                        }
                        let readback = self.observe(&id)?;
                        ensure!(
                            scope(&id, &readback.value, Some(&before_eff)) == before_eff,
                            "Restore {id} readback differs from original; pending transaction {} retained",
                            tx.name
                        );
                        self.append(tx, Record::Restored { id: id.clone() })?;
                        tx.entries[i].state = State::Restored;
                        Self::observed_outcome(
                            &c,
                            "restored",
                            if c.reboot {
                                "Original preference restored; restart required"
                            } else {
                                "Original preference restored"
                            },
                            &readback,
                        )
                    }
                };
                callback(&result.id, &result.status);
                report.results.push(result);
            }
            if tx.entries.iter().all(|e| e.state == State::Restored) {
                self.append(tx, Record::Reverted)?;
                tx.reverted = true;
            }
        }
        report.findings = self.findings();
        if let Some(tx) = transactions.iter().rev().find(|t| !t.reverted) {
            report.findings.push(Self::journal_finding(tx));
        }
        Ok(report)
    }
}

/// Dynamic hardening controls (firewall rules, saved Wi-Fi networks) observe
/// whatever exists now; comparisons against a journaled state must only look at
/// the keys that were journaled. Everything else is returned unchanged.
fn scope(id: &str, observed: &Value, template: Option<&Value>) -> Value {
    match (crate::hardening::spec(id), template) {
        (Some(spec), Some(template)) => spec.view(observed, template),
        _ => observed.clone(),
    }
}

fn firewall_control(id: &str) -> bool {
    matches!(
        id,
        "firewall.domain.enabled"
            | "firewall.private.enabled"
            | "firewall.public.enabled"
            | "firewall.domain.inbound"
            | "firewall.private.inbound"
            | "firewall.public.inbound"
    )
}

/// Assessment only. Never use this predicate to replace a durable raw original
/// or expected raw value during owned-control comparison or rollback.
fn firewall_protected(o: &Observation) -> Result<bool> {
    let authority = o.authority.context("Firewall authority is unavailable")?;
    if authority != Authority::Local || !o.eligible {
        return Ok(false);
    }
    let effective = o
        .effective
        .context("Effective firewall evidence is unavailable")?;
    match effective {
        EffectiveFirewall::Enabled(enabled) => {
            ensure!(
                o.value.as_bool() == Some(enabled),
                "Firewall evidence contradicts the local preference"
            );
            Ok(enabled)
        }
        EffectiveFirewall::Inbound(action) => {
            ensure!(
                o.value == "NotConfigured"
                    || (o.value == "Block" && action == InboundAction::Block)
                    || (o.value == "Allow" && action == InboundAction::Allow),
                "Firewall evidence contradicts the local preference"
            );
            Ok(action == InboundAction::Block)
        }
    }
}

fn assessment_status(id: &str, o: &Observation) -> Result<&'static str> {
    if firewall_control(id) {
        return Ok(if firewall_protected(o)? {
            "compliant"
        } else if apply_eligible(id, o) {
            "attention"
        } else {
            "skipped"
        });
    }
    if permission_control(id) && !o.eligible {
        return Ok("skipped");
    }
    if let Some(spec) = crate::hardening::spec(id) {
        // Safe and absent-default states are protected, even on a managed PC.
        return Ok(if !spec.any_unsafe(&o.value) {
            "compliant"
        } else if apply_eligible(id, o) {
            "attention"
        } else {
            "skipped"
        });
    }
    Ok(if o.value == target_for(id, &o.value)? {
        "compliant"
    } else if apply_eligible(id, o) {
        "attention"
    } else {
        "skipped"
    })
}

fn apply_eligible(id: &str, o: &Observation) -> bool {
    if !o.eligible {
        return false;
    }
    if firewall_control(id) {
        return o.authority == Some(Authority::Local) && matches!(firewall_protected(o), Ok(false));
    }
    // Extended controls: management/capability evidence (`eligible`) plus
    // something that is genuinely unsafe. Safe or absent-default state is
    // never a repair request.
    if let Some(spec) = crate::hardening::spec(id) {
        return spec.any_unsafe(&o.value);
    }
    // Registry absence and nonzero UAC modes are not repair requests, even if a
    // backend accidentally advertises them as eligible. Binary originals must
    // be the explicit unsafe value, never an invented default for absence.
    if machine_registry_control(id) {
        let unsafe_value = if matches!(
            id,
            "installer.always_install_elevated" | "wdigest.use_logon_credential"
        ) {
            1
        } else {
            0
        };
        o.value == json!({"present":true,"value":unsafe_value})
    } else if matches!(id, "uac.enabled" | "uac.consent") {
        o.value == json!({"present":true,"value":0})
    } else {
        true
    }
}

fn restore_eligible(c: &Control, o: &Observation) -> bool {
    if firewall_control(&c.id) {
        return o.eligible && o.authority == Some(Authority::Local);
    }
    o.eligible
        || (matches!(c.id.as_str(), "uac.enabled" | "uac.consent")
        && o.value == c.target
        // This exact platform reason is emitted only after Gate succeeds.
        // A management/capability error replaces it and must never be bypassed.
        && o.reason == "Preserving absent or nonzero UAC preference")
        || (machine_registry_control(&c.id)
            && o.value == c.target
            && o.reason == "Preserving absent or already-safe machine preference")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Probe;
    use std::{cell::RefCell, collections::HashMap, rc::Rc};
    use tempfile::TempDir;

    const FIREWALL: &str = "firewall.public.inbound";
    const DEFENDER: &str = "defender.realtime";

    #[test]
    fn all_default_block_profiles_are_protected_without_preference_or_wal_changes() {
        let ids = [
            "firewall.domain.inbound",
            "firewall.private.inbound",
            FIREWALL,
        ];
        let (dir, state, e) = fixture(ids[0], json!("NotConfigured"));
        drop(e);
        for id in ids {
            state
                .borrow_mut()
                .values
                .insert(id.into(), json!("NotConfigured"));
            state.borrow_mut().evidence.insert(
                id.into(),
                (
                    Some(EffectiveFirewall::Inbound(InboundAction::Block)),
                    Some(Authority::Local),
                ),
            );
        }
        let mut e = reopen(&dir, &state, &ids);
        assert!(e
            .audit()
            .unwrap()
            .results
            .iter()
            .all(|r| r.status == "compliant"
                && r.authority == Some(Authority::Local)
                && r.effective == Some(EffectiveFirewall::Inbound(InboundAction::Block))));
        let selected = ids.map(str::to_owned);
        for report in [
            e.apply(|_, _| {}).unwrap(),
            e.apply_selected(&selected, |_, _| {}).unwrap(),
        ] {
            assert!(report.transaction.is_none());
            assert!(report.results.iter().all(|r| r.status == "unchanged"));
        }
        assert!(e.history().unwrap().is_empty());
        assert!(state.borrow().writes.is_empty());
        assert!(state.borrow().values.values().all(|v| v == "NotConfigured"));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1); // base lock only
    }

    #[test]
    fn genuine_firewall_gaps_repair_and_undo_exact_raw_schema_one_originals() {
        for raw in [json!("Allow"), json!("NotConfigured")] {
            let (dir, state, mut e) = fixture(FIREWALL, raw.clone());
            assert_eq!(e.audit().unwrap().results[0].status, "attention");
            let report = e.apply_selected(&[FIREWALL.into()], |_, _| {}).unwrap();
            assert_eq!(report.results[0].status, "applied");
            assert_eq!(
                report.results[0].effective,
                Some(EffectiveFirewall::Inbound(InboundAction::Block))
            );
            let tx = e.load().unwrap().pop().unwrap();
            assert_eq!(tx.entries[0].before, raw);
            let text = fs::read_to_string(dir.path().join(format!("{}.jsonl", tx.name))).unwrap();
            assert!(text.contains("\"schema\":1"));
            assert!(!text.contains("effective") && !text.contains("authority"));
            drop(tx);
            drop(e);
            let mut e = reopen(&dir, &state, &[FIREWALL]);
            e.revert(|_, _| {}).unwrap();
            assert_eq!(state.borrow().values[FIREWALL], raw);
            assert_eq!(state.borrow().writes.len(), 2);
        }
    }

    #[test]
    fn missing_contradictory_and_managed_firewall_evidence_never_writes_or_claims_protection() {
        for (id, raw, effective, authority, managed, status) in [
            (
                FIREWALL,
                json!("NotConfigured"),
                None,
                Some(Authority::Local),
                false,
                "error",
            ),
            (
                FIREWALL,
                json!("Block"),
                Some(EffectiveFirewall::Inbound(InboundAction::Block)),
                None,
                false,
                "error",
            ),
            (
                FIREWALL,
                json!("Allow"),
                Some(EffectiveFirewall::Inbound(InboundAction::Block)),
                Some(Authority::Local),
                false,
                "error",
            ),
            (
                FIREWALL,
                json!("Allow"),
                Some(EffectiveFirewall::Inbound(InboundAction::Block)),
                Some(Authority::Managed),
                true,
                "skipped",
            ),
            (
                FIREWALL,
                json!("Block"),
                None,
                Some(Authority::Unknown),
                true,
                "skipped",
            ),
            (
                "firewall.public.enabled",
                json!(true),
                None,
                Some(Authority::Local),
                false,
                "error",
            ),
            (
                "firewall.public.enabled",
                json!(true),
                Some(EffectiveFirewall::Enabled(false)),
                Some(Authority::Local),
                false,
                "error",
            ),
        ] {
            let (_dir, state, mut e) = fixture(id, raw);
            state.borrow_mut().blocked = managed;
            state
                .borrow_mut()
                .evidence
                .insert(id.into(), (effective, authority));
            assert_eq!(e.audit().unwrap().results[0].status, status);
            assert_eq!(
                e.apply_selected(&[id.into()], |_, _| {}).unwrap().results[0].status,
                status
            );
            assert!(state.borrow().writes.is_empty());
            assert!(e.history().unwrap().is_empty());
        }
    }

    #[test]
    fn owned_firewall_uses_raw_drift_and_final_gate_rechecks_evidence() {
        let (_dir, state, mut e) = fixture(FIREWALL, json!("Allow"));
        // Inject evidence loss at the fresh probe after Prepare, not a new raw target.
        state.borrow_mut().evidence_at = Some((2, None, Some(Authority::Local)));
        assert!(e.apply_selected(&[FIREWALL.into()], |_, _| {}).is_err());
        assert!(state.borrow().writes.is_empty());
        assert_eq!(e.load().unwrap()[0].entries[0].before, json!("Allow"));
        e.revert(|_, _| {}).unwrap(); // original already present, no write needed
        state.borrow_mut().evidence.clear();
        e.apply_selected(&[FIREWALL.into()], |_, _| {}).unwrap();
        state
            .borrow_mut()
            .values
            .insert(FIREWALL.into(), json!("NotConfigured"));
        state.borrow_mut().evidence.insert(
            FIREWALL.into(),
            (
                Some(EffectiveFirewall::Inbound(InboundAction::Block)),
                Some(Authority::Local),
            ),
        );
        assert_eq!(e.audit().unwrap().results[0].status, "compliant");
        assert_eq!(
            e.apply_selected(&[FIREWALL.into()], |_, _| {})
                .unwrap()
                .results[0]
                .status,
            "conflict"
        );
        assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "conflict");
        state
            .borrow_mut()
            .values
            .insert(FIREWALL.into(), json!("Block"));
        state.borrow_mut().evidence.clear();
        e.revert(|_, _| {}).unwrap();
        assert_eq!(state.borrow().values[FIREWALL], json!("Allow"));
    }

    #[test]
    fn default_becoming_protected_after_prepare_requires_explicit_recovery() {
        let (_dir, state, mut e) = fixture(FIREWALL, json!("NotConfigured"));
        state.borrow_mut().evidence_at = Some((
            2,
            Some(EffectiveFirewall::Inbound(InboundAction::Block)),
            Some(Authority::Local),
        ));
        assert!(e.apply_selected(&[FIREWALL.into()], |_, _| {}).is_err());
        let tx = e.load().unwrap().pop().unwrap();
        assert!(!tx.sealed);
        assert_eq!(tx.entries[0].state, State::Pending);
        assert_eq!(tx.entries[0].before, json!("NotConfigured"));
        assert!(state.borrow().writes.is_empty());
        drop(tx);
        assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "unchanged");
        assert!(e.load().unwrap()[0].reverted);
        assert!(state.borrow().writes.is_empty());
    }

    #[test]
    fn firewall_readback_requires_effective_protection_before_sealing() {
        for (id, before, evidence, authority, blocked) in [
            (
                FIREWALL,
                json!("Allow"),
                None,
                Some(Authority::Local),
                false,
            ),
            (
                FIREWALL,
                json!("Allow"),
                Some(EffectiveFirewall::Inbound(InboundAction::Allow)),
                Some(Authority::Local),
                false,
            ),
            (
                "firewall.public.enabled",
                json!(false),
                Some(EffectiveFirewall::Enabled(false)),
                Some(Authority::Local),
                false,
            ),
            (
                FIREWALL,
                json!("Allow"),
                Some(EffectiveFirewall::Inbound(InboundAction::Block)),
                None,
                false,
            ),
            (
                FIREWALL,
                json!("Allow"),
                Some(EffectiveFirewall::Inbound(InboundAction::Block)),
                Some(Authority::Managed),
                true,
            ),
            (
                FIREWALL,
                json!("Allow"),
                Some(EffectiveFirewall::Inbound(InboundAction::Block)),
                Some(Authority::Unknown),
                true,
            ),
            (
                FIREWALL,
                json!("Allow"),
                Some(EffectiveFirewall::Inbound(InboundAction::Block)),
                Some(Authority::Local),
                true,
            ),
        ] {
            for selected in [false, true] {
                let (dir, state, mut e) = fixture(id, before.clone());
                state.borrow_mut().evidence_at = Some((3, evidence, authority));
                if blocked {
                    state.borrow_mut().block_at = Some(3);
                }
                let result = if selected {
                    e.apply_selected(&[id.into()], |_, _| {})
                } else {
                    e.apply(|_, _| {})
                };
                assert!(
                    result.is_err(),
                    "unverified readback must retain pending intent"
                );
                assert_eq!(state.borrow().writes.len(), 1);
                let tx = e.load().unwrap().pop().unwrap();
                assert!(!tx.sealed);
                assert_eq!(tx.entries[0].state, State::Pending);
                assert_eq!(tx.entries[0].before, before);
                drop(tx);
                drop(e);
                state.borrow_mut().evidence.clear();
                state.borrow_mut().blocked = false;
                let mut e = reopen(&dir, &state, &[id]);
                e.revert(|_, _| {}).unwrap();
                assert_eq!(state.borrow().values[id], before);
            }
        }
    }

    #[test]
    fn blocked_readiness_preserves_owned_noops_and_existing_wal_bytes() {
        let (dir, state, e) = fixture(DEFENDER, json!(true));
        drop(e);
        state
            .borrow_mut()
            .values
            .insert(FIREWALL.into(), json!("Allow"));
        let mut e = reopen(&dir, &state, &[DEFENDER, FIREWALL]);
        let name = e
            .apply_selected(&[DEFENDER.into()], |_, _| {})
            .unwrap()
            .transaction
            .unwrap();
        let path = dir.path().join(format!("{name}.jsonl"));
        let original = fs::read(&path).unwrap();
        state.borrow_mut().readiness.journal_volume = Probe::Known(crate::model::VolumeReadiness {
            available_bytes: 0,
            read_only: true,
        });
        for ids in [
            vec![DEFENDER.into()],
            vec![DEFENDER.into(), FIREWALL.into()],
        ] {
            let report = e.apply_selected(&ids, |_, _| {}).unwrap();
            assert_eq!(report.results[0].status, "unchanged");
            if ids.len() == 2 {
                assert_eq!(report.results[1].status, "skipped");
            }
            assert!(report.readiness.is_some());
            assert_eq!(fs::read(&path).unwrap(), original);
            assert_eq!(e.load().unwrap().len(), 1);
            assert_eq!(state.borrow().writes.len(), 1);
        }
        // The existing-active legacy path performs no intent/write either.
        e.apply(|_, _| {}).unwrap();
        assert_eq!(fs::read(path).unwrap(), original);
        assert_eq!(state.borrow().writes.len(), 1);
    }

    #[test]
    fn readiness_refresh_blocks_only_confirmed_storage_conditions_and_never_undo() {
        use crate::model::{PowerReadiness, VolumeReadiness};
        let volume = |bytes, read_only| {
            Probe::Known(VolumeReadiness {
                available_bytes: bytes,
                read_only,
            })
        };
        for readiness in [
            Readiness {
                system_volume: volume(100, true),
                ..Default::default()
            },
            Readiness {
                journal_volume: volume(100, true),
                ..Default::default()
            },
            Readiness {
                journal_volume: volume(0, false),
                ..Default::default()
            },
        ] {
            let (_dir, state, mut e) = fixture(DEFENDER, json!(true));
            let audit = e.audit().unwrap();
            assert_eq!(audit.readiness, Some(Readiness::default()));
            assert_eq!(state.borrow().readiness_count, 1);
            state.borrow_mut().readiness = readiness.clone();
            let report = e.apply_selected(&[DEFENDER.into()], |_, _| {}).unwrap();
            assert_eq!(report.readiness, Some(readiness.clone()));
            assert_eq!(report.results[0].status, "skipped");
            assert_eq!(state.borrow().readiness_count, 2);
            assert!(state.borrow().writes.is_empty());
            assert!(e.history().unwrap().is_empty());
            state.borrow_mut().readiness = Readiness::default();
            e.apply_selected(&[DEFENDER.into()], |_, _| {}).unwrap();
            state.borrow_mut().readiness = readiness;
            let calls = state.borrow().readiness_count;
            e.revert(|_, _| {}).unwrap();
            assert_eq!(state.borrow().readiness_count, calls);
            assert_eq!(state.borrow().values[DEFENDER], json!(true));
        }
        for readiness in [
            Readiness::default(),
            Readiness {
                system_volume: volume(0, false),
                journal_volume: volume(1u64 << 40, false),
                power: Probe::Known(PowerReadiness {
                    ac_connected: Some(false),
                    battery_percent: Some(1),
                    battery_present: Some(true),
                }),
                windows_update_reboot: Probe::Known(true),
            },
        ] {
            let (_dir, state, mut e) = fixture(DEFENDER, json!(true));
            state.borrow_mut().readiness = readiness.clone();
            let report = e.apply(|_, _| {}).unwrap();
            assert_eq!(report.readiness, Some(readiness));
            assert_eq!(report.results[0].status, "applied");
        }
    }

    #[test]
    fn updater_reserved_entries_coexist_with_exact_journal_roundtrip() {
        let (dir, state, e) = fixture(DEFENDER, json!(true));
        drop(e);
        let mut held = Vec::new();
        for name in LEGACY_UPDATE_FILES {
            let path = dir.path().join(name);
            // Deliberately not JSON, including the manifest/status files.
            fs::write(&path, b"updater data\0not a journal").unwrap();
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                // Model the published worker/installer allowing only readers.
                options.share_mode(0x1);
            }
            held.push(options.open(path).unwrap());
        }
        fs::create_dir(dir.path().join("Updates")).unwrap();
        fs::write(dir.path().join("Updates/staged-data.bin"), b"staged").unwrap();
        let mut e = reopen(&dir, &state, &[DEFENDER]);
        assert_eq!(e.audit().unwrap().results[0].status, "attention");
        assert!(e.history().unwrap().is_empty());
        let applied = e.apply_selected(&[DEFENDER.into()], |_, _| {}).unwrap();
        let name = applied.transaction.unwrap();
        assert_eq!(e.history().unwrap(), vec![format!("{name} applied")]);
        assert_eq!(e.load().unwrap().len(), 1);
        assert_eq!(e.audit().unwrap().results[0].status, "compliant");
        let reverted = e.revert(|_, _| {}).unwrap();
        assert_eq!(reverted.transaction.as_deref(), Some(name.as_str()));
        assert_eq!(reverted.results[0].status, "restored");
        assert_eq!(e.history().unwrap(), vec![format!("{name} reverted")]);
        assert_eq!(state.borrow().values[DEFENDER], json!(true));
        for file in LEGACY_UPDATE_FILES {
            assert_eq!(
                fs::read(dir.path().join(file)).unwrap(),
                b"updater data\0not a journal"
            );
        }
        assert_eq!(
            fs::read(dir.path().join("Updates/staged-data.bin")).unwrap(),
            b"staged"
        );
        drop(held);
    }

    fn assert_reserved_history_rejected(e: &mut Engine, state: &Rc<RefCell<FakeState>>) {
        assert!(e.audit().is_err());
        assert!(e.apply_selected(&[DEFENDER.into()], |_, _| {}).is_err());
        assert!(e.revert(|_, _| {}).is_err());
        assert!(e.history().is_err());
        assert!(state.borrow().events.is_empty());
    }

    #[test]
    fn updater_exceptions_reject_wrong_types_unknown_names_and_corrupt_wal() {
        for name in LEGACY_UPDATE_FILES {
            let (dir, state, mut e) = fixture(DEFENDER, json!(true));
            fs::create_dir(dir.path().join(name)).unwrap();
            assert_reserved_history_rejected(&mut e, &state);
        }
        for name in [
            "Updates", // reserved directory cannot be a file
            "updates",
            "Updates.jsonl",
            "update-extra.exe",
            "update-worker.exe.jsonl",
            "update-status.json.jsonl",
            "update.lock.backup",
            "junk",
        ] {
            let (dir, state, mut e) = fixture(DEFENDER, json!(true));
            fs::write(dir.path().join(name), b"{}").unwrap();
            assert_reserved_history_rejected(&mut e, &state);
        }
        let (dir, state, mut e) = fixture(DEFENDER, json!(true));
        for name in LEGACY_UPDATE_FILES {
            fs::write(dir.path().join(name), b"reserved").unwrap();
        }
        fs::create_dir(dir.path().join("Updates")).unwrap();
        let tx = prepare(&mut e, 1, DEFENDER, json!(true));
        let path = dir.path().join(format!("{}.jsonl", tx.name));
        drop(tx);
        fs::write(&path, b"{corrupt WAL}\n").unwrap();
        assert_reserved_history_rejected(&mut e, &state);
        assert_eq!(fs::read(path).unwrap(), b"{corrupt WAL}\n");
    }

    #[test]
    fn updater_reserved_files_reject_hardlinks() {
        for name in LEGACY_UPDATE_FILES {
            let (dir, state, mut e) = fixture(DEFENDER, json!(true));
            let outside = tempfile::tempdir().unwrap();
            let source = outside.path().join("source");
            fs::write(&source, b"data").unwrap();
            fs::hard_link(source, dir.path().join(name)).unwrap();
            assert_reserved_history_rejected(&mut e, &state);
        }
    }

    #[cfg(unix)]
    #[test]
    fn updater_reserved_entries_reject_symlinks() {
        use std::os::unix::fs::symlink;
        for name in LEGACY_UPDATE_FILES.into_iter().chain(["Updates"]) {
            let (dir, state, mut e) = fixture(DEFENDER, json!(true));
            let outside = tempfile::tempdir().unwrap();
            let source = outside.path().join("source");
            if name == "Updates" {
                fs::create_dir(&source).unwrap();
            } else {
                fs::write(&source, b"data").unwrap();
            }
            symlink(source, dir.path().join(name)).unwrap();
            assert_reserved_history_rejected(&mut e, &state);
        }
    }

    #[test]
    fn selected_validation_and_probe_isolation() {
        let (dir, state, mut e) = fixture(DEFENDER, json!(true));
        // Advertised but unsupported at runtime: probing this missing value panics.
        e.controls.push(Control {
            id: FIREWALL.into(),
            title: FIREWALL.into(),
            description: String::new(),
            target: target(FIREWALL).unwrap(),
            reboot: false,
        });
        for ids in [
            vec![],
            vec!["ALL".into()],
            vec!["".into()],
            vec![DEFENDER.into(), DEFENDER.into()],
            vec![DEFENDER.into(), "unknown".into()],
        ] {
            assert!(e
                .apply_selected(&ids, |_, _| panic!("invalid callback"))
                .is_err());
            assert!(e.load().unwrap().is_empty());
            assert!(state.borrow().events.is_empty());
            assert_eq!(state.borrow().readiness_count, 0);
        }
        let mut callbacks = Vec::new();
        let r = e
            .apply_selected(&[DEFENDER.into()], |id, status| {
                callbacks.push((id.to_owned(), status.to_owned()))
            })
            .unwrap();
        assert_eq!(r.results.len(), 1);
        assert_eq!(
            callbacks,
            vec![
                ("readiness".into(), "pending".into()),
                ("readiness".into(), "complete".into()),
                (DEFENDER.into(), "applied".into()),
            ]
        );
        assert!(state.borrow().events.iter().all(|s| s.ends_with(DEFENDER)));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn selected_disjoint_batches_reopen_and_reverse_undo() {
        let (dir, state, e) = fixture(DEFENDER, json!(true));
        drop(e);
        state
            .borrow_mut()
            .values
            .insert(FIREWALL.into(), json!("Allow"));
        let mut e = reopen(&dir, &state, &[DEFENDER, FIREWALL]);
        e.apply_selected(&[DEFENDER.into()], |_, _| {}).unwrap();
        let original = e.load().unwrap()[0].length;
        e.apply_selected(&[DEFENDER.into(), FIREWALL.into()], |_, _| {})
            .unwrap();
        assert_eq!(e.load().unwrap()[0].length, original);
        assert_eq!(e.load().unwrap().len(), 2);
        drop(e);
        let mut e = reopen(&dir, &state, &[DEFENDER, FIREWALL]);
        assert_eq!(e.revert(|_, _| {}).unwrap().results[0].id, FIREWALL);
        assert_eq!(state.borrow().values[DEFENDER], json!(false));
        assert_eq!(e.revert(|_, _| {}).unwrap().results[0].id, DEFENDER);
        assert_eq!(state.borrow().values[DEFENDER], json!(true));
        assert_eq!(state.borrow().values[FIREWALL], json!("Allow"));
    }

    #[test]
    fn selected_exact_acl_conflict_blocks_entire_mixed_batch() {
        let id = "permissions.service.bits";
        let before = acl_snapshot(0x0002_0012, 1);
        let (dir, state, e) = fixture(id, before.clone());
        drop(e);
        state
            .borrow_mut()
            .values
            .insert(DEFENDER.into(), json!(true));
        let mut e = reopen(&dir, &state, &[DEFENDER, id]);
        e.apply_selected(&[id.into()], |_, _| {}).unwrap();
        state
            .borrow_mut()
            .values
            .insert(id.into(), acl_snapshot(0x0002_0030, 1));
        let r = e
            .apply_selected(&[DEFENDER.into(), id.into()], |_, _| {})
            .unwrap();
        assert_eq!(r.results[0].status, "skipped");
        assert_eq!(r.results[1].status, "conflict");
        assert_eq!(state.borrow().writes.len(), 1);
        assert_eq!(e.load().unwrap().len(), 1);
        assert_eq!(e.load().unwrap()[0].entries[0].before, before);
    }

    #[test]
    fn selected_pending_and_reverting_block_new_batches_and_earlier_undo() {
        let (dir, state, e) = fixture(DEFENDER, json!(true));
        drop(e);
        state
            .borrow_mut()
            .values
            .insert(FIREWALL.into(), json!("Allow"));
        let mut e = reopen(&dir, &state, &[DEFENDER, FIREWALL]);
        e.apply_selected(&[DEFENDER.into()], |_, _| {}).unwrap();
        state.borrow_mut().fail_write = true;
        assert!(e.apply_selected(&[FIREWALL.into()], |_, _| {}).is_err());
        state.borrow_mut().fail_write = false;
        let n = state.borrow().observe_count;
        assert_eq!(
            e.apply_selected(&[DEFENDER.into()], |_, _| {})
                .unwrap()
                .results[0]
                .status,
            "pending"
        );
        assert_eq!(state.borrow().observe_count, n);
        state
            .borrow_mut()
            .values
            .insert(FIREWALL.into(), json!("NotConfigured"));
        for _ in 0..2 {
            let r = e.revert(|_, _| {}).unwrap();
            assert_eq!(r.results.len(), 1);
            assert_eq!(r.results[0].status, "conflict");
            assert_eq!(state.borrow().values[DEFENDER], json!(false));
            assert_eq!(
                e.apply_selected(&[DEFENDER.into()], |_, _| {})
                    .unwrap()
                    .results[0]
                    .status,
                "pending"
            );
        }
    }

    #[test]
    fn selected_final_gate_rejects_race_and_audit_reports_progress() {
        let (_dir, state, mut e) = fixture(DEFENDER, json!(true));
        let mut progress = Vec::new();
        assert_eq!(
            e.audit_with_progress(|id, s| progress.push((id.to_owned(), s.to_owned())))
                .unwrap()
                .results[0]
                .status,
            "attention"
        );
        assert_eq!(
            progress,
            vec![
                (DEFENDER.into(), "attention".into()),
                ("readiness".into(), "pending".into()),
                ("readiness".into(), "complete".into()),
                ("findings".into(), "pending".into()),
                ("findings".into(), "complete".into())
            ]
        );
        state.borrow_mut().block_at = Some(3);
        assert!(e.apply_selected(&[DEFENDER.into()], |_, _| {}).is_err());
        assert!(state.borrow().writes.is_empty());
        assert!(!e.load().unwrap()[0].sealed);
    }

    #[test]
    fn selected_initial_probe_errors_preserve_success_and_allow_later_batches() {
        for invalid_value in [false, true] {
            let (dir, state, e) = fixture(DEFENDER, json!(true));
            drop(e);
            state.borrow_mut().values.insert(
                FIREWALL.into(),
                if invalid_value {
                    json!("invalid")
                } else {
                    json!("Allow")
                },
            );
            if !invalid_value {
                state.borrow_mut().fail_observe = Some(FIREWALL.into());
            }
            let mut e = reopen(&dir, &state, &[DEFENDER, FIREWALL]);
            let mut statuses = Vec::new();
            let report = e
                .apply_selected(&[DEFENDER.into(), FIREWALL.into()], |_, s| {
                    statuses.push(s.to_owned());
                })
                .unwrap();
            assert_eq!(statuses, ["pending", "complete", "applied", "error"]);
            assert_eq!(report.results.len(), 2);
            let tx = e.load().unwrap().pop().unwrap();
            assert!(tx.sealed);
            assert_eq!(tx.entries.len(), 1);
            assert_eq!(tx.entries[0].before, json!(true));
            drop(tx);
            drop(e);
            state.borrow_mut().fail_observe = None;
            state
                .borrow_mut()
                .values
                .insert(FIREWALL.into(), json!("Allow"));
            let mut e = reopen(&dir, &state, &[DEFENDER, FIREWALL]);
            e.apply_selected(&[FIREWALL.into()], |_, _| {}).unwrap();
            assert_eq!(e.revert(|_, _| {}).unwrap().results[0].id, FIREWALL);
            assert_eq!(e.revert(|_, _| {}).unwrap().results[0].id, DEFENDER);
            assert_eq!(state.borrow().values[DEFENDER], json!(true));
        }
    }

    #[test]
    fn incomplete_older_batch_rejects_disjoint_active_history_without_changes() {
        for reverting in [false, true] {
            let (dir, state, e) = fixture(DEFENDER, json!(true));
            drop(e);
            let mut e = reopen(&dir, &state, &[DEFENDER, FIREWALL]);
            let mut older = prepare(&mut e, 1, DEFENDER, json!(true));
            if reverting {
                e.append(&mut older, Record::Reverting).unwrap();
            }
            let newer = prepare(&mut e, 2, FIREWALL, json!("Allow"));
            let paths: Vec<_> = [&older, &newer]
                .iter()
                .map(|tx| {
                    let path = dir.path().join(format!("{}.jsonl", tx.name));
                    let bytes = fs::read(&path).unwrap();
                    (path, bytes)
                })
                .collect();
            drop((older, newer));
            assert!(e.audit().is_err());
            assert!(e.history().is_err());
            assert!(e.apply_selected(&[DEFENDER.into()], |_, _| {}).is_err());
            assert!(e.revert(|_, _| {}).is_err());
            assert!(Engine::open(
                dir.path().into(),
                backend(&state, &[DEFENDER, FIREWALL], "machine-a")
            )
            .is_err());
            assert!(state.borrow().events.is_empty());
            for (path, bytes) in paths {
                assert_eq!(fs::read(path).unwrap(), bytes);
            }
        }
    }

    #[derive(Default)]
    struct FakeState {
        values: HashMap<String, Value>,
        writes: Vec<(String, Value)>,
        events: Vec<String>,
        blocked: bool,
        fail_write: bool,
        fail_before_write: bool,
        fail_findings: bool,
        fail_machine: bool,
        catalog_target: Option<Value>,
        observe_count: usize,
        drift_at: Option<(usize, Value)>,
        block_at: Option<usize>,
        fail_observe: Option<String>,
        evidence: HashMap<String, (Option<EffectiveFirewall>, Option<Authority>)>,
        evidence_at: Option<(usize, Option<EffectiveFirewall>, Option<Authority>)>,
        readiness: Readiness,
        readiness_count: usize,
    }
    struct Fake {
        state: Rc<RefCell<FakeState>>,
        ids: Vec<String>,
        machine: String,
    }
    impl Backend for Fake {
        fn machine_id(&mut self) -> Result<String> {
            if self.state.borrow().fail_machine {
                bail!("Simulated machine identity transport failure");
            }
            Ok(self.machine.clone())
        }
        fn controls(&self) -> Vec<Control> {
            self.ids
                .iter()
                .map(|id| Control {
                    id: id.clone(),
                    title: id.clone(),
                    description: String::new(),
                    target: self
                        .state
                        .borrow()
                        .catalog_target
                        .clone()
                        .unwrap_or_else(|| target(id).unwrap()),
                    reboot: false,
                })
                .collect()
        }
        fn observe(&mut self, id: &str) -> Result<Observation> {
            let mut s = self.state.borrow_mut();
            s.events.push(format!("observe:{id}"));
            s.observe_count += 1;
            if let Some((n, effective, authority)) = s.evidence_at {
                if n == s.observe_count {
                    s.evidence.insert(id.into(), (effective, authority));
                }
            }
            if s.fail_observe.as_deref() == Some(id) {
                bail!("Simulated unsupported observation");
            }
            if s.block_at == Some(s.observe_count) {
                s.blocked = true;
            }
            if let Some((n, value)) = s.drift_at.clone() {
                if s.observe_count == n {
                    s.values.insert(id.into(), value);
                }
            }
            let value = s.values[id].clone();
            let (eligible, reason) = if s.blocked {
                (false, "Managed device")
            } else if id.starts_with("uac.") && value != json!({"present":true,"value":0}) {
                (false, "Preserving absent or nonzero UAC preference")
            } else if machine_registry_control(id)
                && (value["present"] == false || value == target(id)?)
            {
                (
                    false,
                    "Preserving absent or already-safe machine preference",
                )
            } else {
                (true, "Eligible")
            };
            let (effective, authority) = if firewall_control(id) {
                let effective = if id.ends_with(".enabled") {
                    EffectiveFirewall::Enabled(value.as_bool().unwrap())
                } else {
                    // Legacy fixtures intentionally treat NotConfigured as an
                    // effective gap; explicit default-proof tests override it.
                    EffectiveFirewall::Inbound(if value == "Block" {
                        InboundAction::Block
                    } else {
                        InboundAction::Allow
                    })
                };
                s.evidence.get(id).copied().unwrap_or((
                    Some(effective),
                    Some(if s.blocked {
                        Authority::Managed
                    } else {
                        Authority::Local
                    }),
                ))
            } else {
                (None, None)
            };
            Ok(Observation {
                value,
                eligible,
                reason: reason.into(),
                effective,
                authority,
            })
        }
        fn write(&mut self, id: &str, value: &Value) -> Result<()> {
            validate_value(id, value)?;
            let mut s = self.state.borrow_mut();
            assert_eq!(
                s.events.last(),
                Some(&format!("observe:{id}")),
                "write must immediately follow a fresh probe"
            );
            s.events.push(format!("write:{id}"));
            if s.fail_before_write {
                bail!("Simulated failure before mutation");
            }
            s.values.insert(id.into(), value.clone());
            s.writes.push((id.into(), value.clone()));
            if s.fail_write {
                bail!("Simulated crash after mutation");
            }
            Ok(())
        }
        fn findings(&mut self) -> Result<Vec<Finding>> {
            if self.state.borrow().fail_findings {
                bail!("Simulated findings transport failure");
            }
            Ok(Vec::new())
        }
        fn readiness(&mut self) -> Readiness {
            let mut state = self.state.borrow_mut();
            state.readiness_count += 1;
            state.readiness.clone()
        }
    }

    fn backend(state: &Rc<RefCell<FakeState>>, ids: &[&str], machine: &str) -> Box<dyn Backend> {
        Box::new(Fake {
            state: state.clone(),
            ids: ids.iter().map(|s| (*s).into()).collect(),
            machine: machine.into(),
        })
    }
    fn fixture(id: &str, before: Value) -> (TempDir, Rc<RefCell<FakeState>>, Engine) {
        let dir = tempfile::tempdir().unwrap();
        let state = Rc::new(RefCell::new(FakeState::default()));
        state.borrow_mut().values.insert(id.into(), before);
        let engine = Engine::open(dir.path().into(), backend(&state, &[id], "machine-a")).unwrap();
        (dir, state, engine)
    }
    fn prepare(engine: &mut Engine, sequence: u64, id: &str, before: Value) -> Transaction {
        let mut tx = engine.create(sequence).unwrap();
        engine
            .append(
                &mut tx,
                Record::Prepare {
                    id: id.into(),
                    before,
                },
            )
            .unwrap();
        tx
    }
    fn reopen(dir: &TempDir, state: &Rc<RefCell<FakeState>>, ids: &[&str]) -> Engine {
        Engine::open(dir.path().into(), backend(state, ids, "machine-a")).unwrap()
    }

    const MACHINE_REGISTRY: [(&str, u32); 4] = [
        ("installer.always_install_elevated", 0),
        ("lsa.restrict_anonymous_sam", 1),
        ("lsa.limit_blank_password_use", 1),
        ("wdigest.use_logon_credential", 0),
    ];

    // Canonical self-relative descriptor: SYSTEM owner, Administrators group,
    // explicit Authenticated Users ACE(s), then an untouched administrator ACE.
    // The fixture builds actual bytes independently of repair_target.
    fn acl_snapshot(mask: u32, count: usize) -> Value {
        acl_snapshot_principals(mask, count, &[18], &[32, 544])
    }

    fn acl_snapshot_principals(mask: u32, count: usize, owner: &[u32], group: &[u32]) -> Value {
        fn sid(subs: &[u32]) -> Vec<u8> {
            let mut bytes = vec![1, subs.len() as u8, 0, 0, 0, 0, 0, 5];
            for sub in subs {
                bytes.extend(sub.to_le_bytes());
            }
            bytes
        }
        let mut sd = vec![0u8; 20];
        sd[0] = 1;
        sd[2..4].copy_from_slice(&0x8004u16.to_le_bytes());
        sd[4..8].copy_from_slice(&20u32.to_le_bytes());
        sd.extend(sid(owner));
        let group_offset = sd.len() as u32;
        sd[8..12].copy_from_slice(&group_offset.to_le_bytes());
        sd.extend(sid(group));
        let offset = sd.len();
        sd[16..20].copy_from_slice(&(offset as u32).to_le_bytes());
        let mut acl = vec![2, 0, 0, 0, 0, 0, 0, 0];
        acl[4..6].copy_from_slice(&((count + 1) as u16).to_le_bytes());
        for (rights, subs) in std::iter::repeat_n((mask, &[11][..]), count)
            .chain(std::iter::once((0x000f_01ff, &[32, 544][..])))
        {
            let principal = sid(subs);
            acl.extend([0, 0]);
            acl.extend(((8 + principal.len()) as u16).to_le_bytes());
            acl.extend(rights.to_le_bytes());
            acl.extend(principal);
        }
        let size = acl.len() as u16;
        acl[2..4].copy_from_slice(&size.to_le_bytes());
        sd.extend(acl);
        let mut text = String::from("dacl-v1:");
        for byte in sd {
            text.push_str(&format!("{byte:02x}"));
        }
        json!(text)
    }

    #[test]
    fn exact_readback_failure_keeps_apply_and_restore_recoverable() {
        for id in ["permissions.service.bits", "permissions.service.wuauserv"] {
            let before = acl_snapshot(0x0002_0012, 1);
            let after = acl_snapshot(0x0002_0010, 1);
            let safe_drift = acl_snapshot(0x0002_0030, 1);
            let (dir, state, mut e) = fixture(id, before.clone());
            // A successful write acknowledgment followed by a different, still
            // compliant ACL must not seal the transaction.
            state.borrow_mut().drift_at = Some((3, safe_drift.clone()));
            assert!(e.apply(|_, _| {}).is_err());
            let tx = e.load().unwrap().pop().unwrap();
            assert_eq!(tx.entries[0].state, State::Pending);
            assert_eq!(tx.entries[0].before, before);
            assert!(!tx.sealed);
            drop(tx);
            drop(e);
            let mut e = reopen(&dir, &state, &[id]);
            assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "conflict");
            assert_eq!(state.borrow().writes.len(), 1);

            state.borrow_mut().values.insert(id.into(), after.clone());
            let n = state.borrow().observe_count;
            state.borrow_mut().drift_at = Some((n + 3, safe_drift));
            assert!(e.revert(|_, _| {}).is_err());
            assert_eq!(e.load().unwrap()[0].entries[0].state, State::Restoring);
            assert!(!e.load().unwrap()[0].reverted);
            drop(e);
            let mut e = reopen(&dir, &state, &[id]);
            assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "conflict");
            assert_eq!(state.borrow().writes.len(), 2);
            state.borrow_mut().values.insert(id.into(), after);
            assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "restored");
            assert_eq!(state.borrow().values[id], before);
        }
    }

    #[test]
    fn owner_group_and_protection_are_exact_engine_drift_fingerprints() {
        let mut protected = acl_snapshot(0x0002_0010, 1).as_str().unwrap().to_owned();
        let prefix = "dacl-v1:".len();
        protected.replace_range(prefix + 2 * 2..prefix + 4 * 2, "0490");
        for id in ["permissions.service.bits", "permissions.service.wuauserv"] {
            for drift in [
                acl_snapshot_principals(0x0002_0010, 1, &[32, 544], &[32, 544]),
                acl_snapshot_principals(0x0002_0010, 1, &[18], &[32, 545]),
                json!(protected),
            ] {
                // These are valid and repair-safe states, not parser failures.
                validate_value(id, &drift).unwrap();
                assert_eq!(target_for(id, &drift).unwrap(), drift);
                let before = acl_snapshot(0x0002_0012, 1);
                let (dir, state, mut e) = fixture(id, before.clone());
                e.apply(|_, _| {}).unwrap();
                state.borrow_mut().values.insert(id.into(), drift.clone());
                drop(e);
                let mut e = reopen(&dir, &state, &[id]);
                assert_eq!(e.apply(|_, _| {}).unwrap().results[0].status, "conflict");
                assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "conflict");
                assert_eq!(state.borrow().values[id], drift);
                assert_eq!(state.borrow().writes.len(), 1);
                assert_eq!(e.load().unwrap()[0].entries[0].before, before);
            }
        }
    }

    #[test]
    fn restore_reason_exceptions_are_exact_and_control_scoped() {
        let uac_reason = "Preserving absent or nonzero UAC preference";
        let registry_reason = "Preserving absent or already-safe machine preference";
        for id in MACHINE_REGISTRY.iter().map(|(id, _)| *id).chain([
            "uac.enabled",
            "uac.consent",
            DEFENDER,
            "permissions.service.bits",
        ]) {
            let current = if permission_control(id) {
                acl_snapshot(0x0002_0010, 1)
            } else {
                target(id).unwrap()
            };
            let (_dir, _state, e) = fixture(id, current.clone());
            for reason in [
                uac_reason,
                registry_reason,
                "Managed device",
                "Eligible",
                "Preserving absent or already-safe machine preference ",
            ] {
                let o = Observation {
                    value: current.clone(),
                    eligible: false,
                    reason: reason.into(),
                    ..Observation::default()
                };
                assert_eq!(
                    restore_eligible(e.control(id).unwrap(), &o),
                    (id.starts_with("uac.") && reason == uac_reason)
                        || (machine_registry_control(id) && reason == registry_reason),
                    "{id}: {reason}"
                );
                let absent = Observation {
                    value: json!({"present":false,"value":null}),
                    ..o
                };
                assert!(!restore_eligible(e.control(id).unwrap(), &absent));
            }
        }
    }

    #[test]
    fn wal_and_line_bounds_reject_before_observation_or_mutation() {
        let id = "permissions.service.bits";
        let (dir, state, mut e) = fixture(id, acl_snapshot(0x0002_0010, 1));
        let tx = prepare(&mut e, 1, id, acl_snapshot(0x0002_0012, 1));
        let path = dir.path().join(format!("{}.jsonl", tx.name));
        drop(tx);
        let prefix = fs::read(&path).unwrap();
        // Valid JSON plus whitespace proves the line-size guard, rather than
        // an incidental syntax/truncation error, rejects the oversized record.
        let mut oversized_line = prefix.clone();
        oversized_line
            .extend_from_slice(b"{\"kind\":\"applied\",\"id\":\"permissions.service.bits\"}");
        oversized_line.extend(vec![b' '; MAX_LINE]);
        oversized_line.push(b'\n');
        let mut oversized_wal = prefix;
        oversized_wal.resize(MAX_WAL as usize, b' ');
        oversized_wal.push(b'\n');
        for (bytes, message) in [
            (oversized_line, "Invalid journal record size"),
            (oversized_wal, "Journal exceeds size limit"),
        ] {
            fs::write(&path, &bytes).unwrap();
            assert!(format!("{:#}", e.revert(|_, _| {}).unwrap_err()).contains(message));
            assert!(state.borrow().events.is_empty());
            assert!(state.borrow().writes.is_empty());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn service_acl_targets_are_exact_deterministic_and_not_catalog_sentinels() {
        for id in ["permissions.service.bits", "permissions.service.wuauserv"] {
            let before = acl_snapshot(0x0002_0012, 1);
            let after = acl_snapshot(0x0002_0010, 1);
            assert_eq!(target_for(id, &before).unwrap(), after);
            assert_eq!(target_for(id, &after).unwrap(), after);
            assert!(validate_value(id, &target(id).unwrap()).is_err());
            let (_dir, state, mut e) = fixture(id, before.clone());
            assert_eq!(e.audit().unwrap().results[0].status, "attention");
            assert_eq!(e.apply(|_, _| {}).unwrap().results[0].status, "applied");
            assert_eq!(state.borrow().writes, vec![(id.into(), after.clone())]);
            assert_eq!(e.load().unwrap()[0].entries[0].before, before);
            assert_eq!(e.audit().unwrap().results[0].status, "compliant");
            assert_eq!(e.apply(|_, _| {}).unwrap().results[0].status, "unchanged");
            state.borrow_mut().blocked = true;
            assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "skipped");
            state.borrow_mut().blocked = false;
            assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "restored");
            assert_eq!(state.borrow().values[id], before);

            let (_dir, state, mut e) = fixture(id, after);
            assert!(e.apply(|_, _| {}).unwrap().transaction.is_none());
            assert!(state.borrow().writes.is_empty());
        }
    }

    #[test]
    fn service_catalog_accepts_only_the_compiled_marker_and_fixed_ids() {
        let id = "permissions.service.bits";
        let before = acl_snapshot(0x0002_0012, 1);
        let (dir, state, e) = fixture(id, before.clone());
        drop(e);
        for wrong in [before.clone(), json!(true), json!("service-dacl-repair-v2")] {
            state.borrow_mut().catalog_target = Some(wrong);
            assert!(Engine::open(dir.path().into(), backend(&state, &[id], "machine-a")).is_err());
        }
        state.borrow_mut().catalog_target = Some(json!("service-dacl-repair-v1"));
        assert!(reopen(&dir, &state, &[id]).history().unwrap().is_empty());
        for unknown in [
            "permissions.service.anything",
            "permissions.service.BITS",
            "permissions.file.bits",
        ] {
            assert!(target(unknown).is_err());
            assert!(validate_value(unknown, &before).is_err());
            assert!(target_for(unknown, &before).is_err());
        }
        assert!(state.borrow().writes.is_empty());
    }

    #[test]
    fn service_acl_unknown_apply_and_restore_outcomes_recover_exactly() {
        for id in ["permissions.service.bits", "permissions.service.wuauserv"] {
            for mutated in [false, true] {
                let before = acl_snapshot(0x0002_0012, 1);
                let (dir, state, mut e) = fixture(id, before.clone());
                state.borrow_mut().fail_before_write = !mutated;
                state.borrow_mut().fail_write = mutated;
                assert!(e.apply(|_, _| {}).is_err());
                assert_eq!(e.load().unwrap()[0].entries[0].state, State::Pending);
                drop(e);
                state.borrow_mut().fail_before_write = false;
                let mut e = reopen(&dir, &state, &[id]);
                assert_eq!(e.apply(|_, _| {}).unwrap().results[0].status, "pending");
                if mutated {
                    assert!(e.revert(|_, _| {}).is_err());
                    assert_eq!(e.load().unwrap()[0].entries[0].state, State::Restoring);
                    drop(e);
                    e = reopen(&dir, &state, &[id]);
                }
                state.borrow_mut().fail_write = false;
                e.revert(|_, _| {}).unwrap();
                assert_eq!(state.borrow().values[id], before);
                assert_eq!(state.borrow().writes.len(), if mutated { 2 } else { 0 });
                assert!(e.load().unwrap()[0].reverted);
            }
        }
    }

    #[test]
    fn intervening_safe_acl_changes_conflict_with_exact_recorded_after_image() {
        let id = "permissions.service.bits";
        let before = acl_snapshot(0x0002_0012, 1);
        let (dir, state, mut e) = fixture(id, before.clone());
        e.apply(|_, _| {}).unwrap();
        let safe_drift = acl_snapshot(0x0002_0030, 1); // additional safe STOP grant
        assert_eq!(target_for(id, &safe_drift).unwrap(), safe_drift);
        state
            .borrow_mut()
            .values
            .insert(id.into(), safe_drift.clone());
        drop(e);
        let mut e = reopen(&dir, &state, &[id]);
        assert_eq!(e.apply(|_, _| {}).unwrap().results[0].status, "conflict");
        assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "conflict");
        assert_eq!(state.borrow().values[id], safe_drift);
        assert_eq!(state.borrow().writes.len(), 1);
        assert_eq!(e.load().unwrap()[0].entries[0].before, before);
        assert!(!e.load().unwrap()[0].reverted);
    }

    #[test]
    fn malformed_impossible_and_already_safe_acl_before_images_are_rejected() {
        let id = "permissions.service.bits";
        let valid = acl_snapshot(0x0002_0012, 1);
        let mut unsupported = valid.as_str().unwrap().to_owned();
        // The first ACE begins at byte 56: DENY is structurally valid but cannot
        // be emitted by this repair algorithm as a repairable original.
        let prefix = "dacl-v1:".len();
        unsupported.replace_range(prefix + 56 * 2..prefix + 56 * 2 + 2, "01");
        validate_value(id, &json!(unsupported)).unwrap();
        assert!(target_for(id, &json!(unsupported)).is_err());
        for bad in [
            json!(null),
            json!(true),
            target(id).unwrap(),
            json!("dacl-v1:00"),
            json!("dacl-v1:GG"),
            json!(format!("dacl-v1:{}", "00".repeat(16 * 1024 + 1))),
            json!("powershell.exe -Command Write-Output untrusted"),
            json!("D:(A;;GA;;;WD)"),
            json!("Block"),
            json!(unsupported),
            acl_snapshot(0x0002_0010, 1),
        ] {
            let (_dir, state, mut e) = fixture(id, valid.clone());
            drop(prepare(&mut e, 1, id, bad));
            assert!(e.revert(|_, _| {}).is_err());
            assert!(e.apply(|_, _| {}).is_err());
            assert!(state.borrow().writes.is_empty());
        }
    }

    #[test]
    fn ineligible_complex_acl_is_skipped_without_stranding_prior_registry_repairs() {
        let registry = "lsa.limit_blank_password_use";
        let id = "permissions.service.bits";
        let mut complex = acl_snapshot(0x0002_0012, 1).as_str().unwrap().to_owned();
        let prefix = "dacl-v1:".len();
        complex.replace_range(prefix + 56 * 2..prefix + 56 * 2 + 2, "01");
        let complex = json!(complex);
        crate::permissions::validate_value(id, &complex).unwrap();
        assert!(target_for(id, &complex).is_err());
        let (dir, state, e) = fixture(registry, json!({"present":true,"value":0}));
        drop(e);
        state.borrow_mut().values.insert(id.into(), complex.clone());
        // Three registry probes precede the ACL observation. The native backend
        // advertises valid-but-unsupported descriptors as ineligible.
        state.borrow_mut().block_at = Some(4);
        let mut e = reopen(&dir, &state, &[registry, id]);
        let report = e.apply(|_, _| {}).unwrap();
        assert_eq!(report.results[0].status, "applied");
        assert_eq!(report.results[1].status, "skipped");
        assert!(e.load().unwrap()[0].sealed);
        assert_eq!(e.audit().unwrap().results[1].status, "skipped");
        assert_eq!(e.apply(|_, _| {}).unwrap().results[1].status, "skipped");
        assert_eq!(state.borrow().values[id], complex);
        assert_eq!(state.borrow().writes.len(), 1);
    }

    #[test]
    fn bounded_large_acl_before_image_survives_wal_roundtrip() {
        let id = "permissions.service.wuauserv";
        let before = acl_snapshot(0x0002_0012, 400);
        assert!(before.as_str().unwrap().len() > 4096);
        let (dir, state, mut e) = fixture(id, before.clone());
        e.apply(|_, _| {}).unwrap();
        drop(e);
        let mut e = reopen(&dir, &state, &[id]);
        assert_eq!(e.load().unwrap()[0].entries[0].before, before);
        e.revert(|_, _| {}).unwrap();
        assert_eq!(state.borrow().values[id], before);
    }

    #[test]
    fn binary_registry_controls_preserve_absence_and_restore_exact_originals() {
        for (id, bit) in MACHINE_REGISTRY {
            let safe = json!({"present":true,"value":bit});
            let unsafe_value = json!({"present":true,"value":1-bit});
            let absent = json!({"present":false,"value":null});
            assert_eq!(target(id).unwrap(), safe);
            for before in [&absent, &safe] {
                let (_dir, state, mut e) = fixture(id, before.clone());
                let report = e.apply(|_, _| {}).unwrap();
                assert!(report.transaction.is_none());
                assert!(e.history().unwrap().is_empty());
                assert!(state.borrow().writes.is_empty());
                // Defense in depth even if a backend mistakes absence for eligibility.
                assert!(!apply_eligible(
                    id,
                    &Observation {
                        value: before.clone(),
                        eligible: true,
                        reason: String::new(),
                        ..Observation::default()
                    }
                ));
            }
            let (dir, state, mut e) = fixture(id, unsafe_value.clone());
            e.apply(|_, _| {}).unwrap();
            assert_eq!(state.borrow().values[id], safe);
            assert_eq!(e.load().unwrap()[0].entries[0].before, unsafe_value);
            assert_eq!(e.apply(|_, _| {}).unwrap().results[0].status, "unchanged");
            state.borrow_mut().blocked = true;
            assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "skipped");
            assert_eq!(state.borrow().writes.len(), 1);
            drop(e);
            state.borrow_mut().blocked = false;
            let mut e = reopen(&dir, &state, &[id]);
            assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "restored");
            assert_eq!(state.borrow().values[id], unsafe_value);
            assert_eq!(state.borrow().writes.len(), 2);

            // Absence remains a supported journal original, although automatic
            // apply never invents an explicit value for it.
            let (_dir, state, mut e) = fixture(id, safe);
            drop(prepare(&mut e, 1, id, absent.clone()));
            assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "restored");
            assert_eq!(state.borrow().values[id], absent);
        }
    }

    #[test]
    fn binary_registry_malformed_values_fail_before_replay() {
        for (id, _) in MACHINE_REGISTRY {
            for bad in [
                json!(true),
                json!(1),
                json!("1"),
                json!({"present":true,"value":2}),
                json!({"present":true,"value":5}),
                json!({"present":true,"value":-1}),
                json!({"present":true,"value":1.0}),
                json!({"present":true,"value":null}),
                json!({"present":false,"value":0}),
                json!({"present":false}),
                json!({"present":true,"value":1,"path":"HKCU"}),
            ] {
                assert!(validate_value(id, &bad).is_err(), "{id}: {bad}");
                let (_dir, state, mut e) = fixture(id, target(id).unwrap());
                drop(prepare(&mut e, 1, id, bad));
                assert!(e.revert(|_, _| {}).is_err());
                assert!(e.apply(|_, _| {}).is_err());
                assert!(state.borrow().writes.is_empty());
            }
            let (_dir, state, mut e) = fixture(id, target(id).unwrap());
            drop(prepare(&mut e, 1, id, target(id).unwrap()));
            assert!(e.revert(|_, _| {}).is_err()); // already-compliant before-image
            assert!(state.borrow().writes.is_empty());
        }
    }

    #[test]
    fn legacy_twelve_control_schema_one_wal_replays_with_extended_catalog() {
        let originals = [
            ("defender.realtime", json!(true)),
            ("defender.behavior", json!(true)),
            ("defender.ioav", json!(true)),
            ("defender.archive", json!(true)),
            ("firewall.domain.enabled", json!(false)),
            ("firewall.private.enabled", json!(false)),
            ("firewall.public.enabled", json!(false)),
            ("firewall.domain.inbound", json!("Allow")),
            ("firewall.private.inbound", json!("Allow")),
            ("firewall.public.inbound", json!("NotConfigured")),
            ("uac.enabled", json!({"present":true,"value":0})),
            ("uac.consent", json!({"present":true,"value":0})),
        ];
        let dir = tempfile::tempdir().unwrap();
        let name = "00000000000000000001-00000000-0000-4000-8000-000000000001";
        let mut wal = format!("{{\"kind\":\"header\",\"schema\":1,\"machine\":\"machine-a\",\"transaction\":\"{name}\",\"sequence\":1}}\n");
        let state = Rc::new(RefCell::new(FakeState::default()));
        for (id, before) in &originals {
            wal.push_str(&format!("{{\"kind\":\"prepare\",\"id\":\"{id}\",\"before\":{before}}}\n{{\"kind\":\"applied\",\"id\":\"{id}\"}}\n"));
            state
                .borrow_mut()
                .values
                .insert((*id).into(), target(id).unwrap());
        }
        wal.push_str("{\"kind\":\"sealed\"}\n");
        fs::write(dir.path().join(format!("{name}.jsonl")), wal).unwrap();
        let mut ids: Vec<&str> = originals.iter().map(|(id, _)| *id).collect();
        ids.extend(MACHINE_REGISTRY.iter().map(|(id, _)| *id));
        ids.extend(["permissions.service.bits", "permissions.service.wuauserv"]);
        let mut e = reopen(&dir, &state, &ids);
        // Old raw NotConfigured originals must still restore even when the
        // current inherited default is effectively Block after that restore.
        state.borrow_mut().evidence.insert(
            FIREWALL.into(),
            (
                Some(EffectiveFirewall::Inbound(InboundAction::Block)),
                Some(Authority::Local),
            ),
        );
        assert_eq!(e.revert(|_, _| {}).unwrap().results.len(), 12);
        for (id, before) in originals {
            assert_eq!(state.borrow().values[id], before);
        }
        assert_eq!(state.borrow().writes.len(), 12);
        assert!(e.load().unwrap()[0].reverted);
    }

    #[test]
    fn apply_is_idempotent_and_audit_never_mutates_preferences() {
        let (_dir, state, mut e) = fixture(DEFENDER, json!(true));
        assert_eq!(e.audit().unwrap().results[0].status, "attention");
        assert!(state.borrow().writes.is_empty());
        assert!(e.history().unwrap().is_empty());
        let mut statuses = Vec::new();
        let first = e
            .apply(|id, status| statuses.push((id.to_string(), status.to_string())))
            .unwrap();
        assert_eq!(
            statuses,
            vec![
                ("readiness".into(), "pending".into()),
                ("readiness".into(), "complete".into()),
                (DEFENDER.into(), "applied".into()),
            ]
        );
        let second = e.apply(|_, status| assert!(status.is_ascii())).unwrap();
        assert_eq!(first.transaction, second.transaction);
        assert_eq!(state.borrow().writes.len(), 1);
        assert_eq!(e.load().unwrap()[0].entries[0].before, json!(true));
        assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "restored");
        assert_eq!(state.borrow().values[DEFENDER], json!(true));
        assert!(e.revert(|_, _| {}).unwrap().transaction.is_none());
        assert_eq!(state.borrow().writes.len(), 2);
    }

    #[test]
    fn prepared_apply_recovery_handles_both_sides_of_write() {
        for written in [false, true] {
            let (dir, state, mut e) = fixture(DEFENDER, json!(true));
            drop(prepare(&mut e, 1, DEFENDER, json!(true)));
            if written {
                state
                    .borrow_mut()
                    .values
                    .insert(DEFENDER.into(), json!(false));
            }
            drop(e);
            let mut e = reopen(&dir, &state, &[DEFENDER]);
            assert_eq!(e.apply(|_, _| {}).unwrap().results[0].status, "pending");
            e.audit().unwrap();
            assert!(state.borrow().writes.is_empty());
            e.revert(|_, _| {}).unwrap();
            assert_eq!(state.borrow().values[DEFENDER], json!(true));
            assert_eq!(state.borrow().writes.len(), usize::from(written));
            assert!(e.history().unwrap()[0].ends_with(" reverted"));
        }
    }

    #[test]
    fn write_error_preserves_pending_and_restore_error_is_retryable() {
        for before_write in [false, true] {
            let (dir, state, mut e) = fixture(DEFENDER, json!(true));
            state.borrow_mut().fail_write = !before_write;
            state.borrow_mut().fail_before_write = before_write;
            assert!(e.apply(|_, _| {}).is_err());
            assert_eq!(e.load().unwrap()[0].entries[0].state, State::Pending);
            drop(e);
            state.borrow_mut().fail_before_write = false;
            let mut e = reopen(&dir, &state, &[DEFENDER]);
            if !before_write {
                // Restore mutates, then reports failure. Retrying recognizes the
                // before image rather than performing a second restore write.
                assert!(e.revert(|_, _| {}).is_err());
                assert_eq!(e.load().unwrap()[0].entries[0].state, State::Restoring);
            }
            let writes = state.borrow().writes.len();
            state.borrow_mut().fail_write = false;
            e.revert(|_, _| {}).unwrap();
            assert_eq!(state.borrow().writes.len(), writes);
            assert_eq!(state.borrow().values[DEFENDER], json!(true));
        }
    }

    #[test]
    fn overlapping_active_owners_fail_closed_before_replay() {
        let (_dir, state, mut e) = fixture(FIREWALL, json!("Allow"));
        let mut older = prepare(&mut e, 1, FIREWALL, json!("Allow"));
        e.append(
            &mut older,
            Record::Applied {
                id: FIREWALL.into(),
            },
        )
        .unwrap();
        e.append(&mut older, Record::Sealed).unwrap();
        drop(older);
        let newer = prepare(&mut e, 2, FIREWALL, json!("NotConfigured"));
        drop(newer);
        assert!(e.revert(|_, _| {}).is_err());
        assert!(e.apply(|_, _| {}).is_err());
        assert!(e.apply_selected(&[FIREWALL.into()], |_, _| {}).is_err());
        assert!(e.audit().is_err());
        assert!(e.history().is_err());
        assert!(state.borrow().events.is_empty());
        assert!(state.borrow().writes.is_empty());
    }

    #[test]
    fn final_probe_detects_apply_and_restore_races() {
        let (_dir, state, mut e) = fixture(FIREWALL, json!("Allow"));
        state.borrow_mut().drift_at = Some((2, json!("NotConfigured")));
        assert!(e.apply(|_, _| {}).is_err());
        assert!(state.borrow().writes.is_empty());
        assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "conflict");
        state
            .borrow_mut()
            .values
            .insert(FIREWALL.into(), json!("Block"));
        let count = state.borrow().observe_count;
        state.borrow_mut().drift_at = Some((count + 2, json!("NotConfigured")));
        assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "conflict");
        assert!(state.borrow().writes.is_empty());
        assert_eq!(e.load().unwrap()[0].entries[0].state, State::Restoring);
    }

    #[test]
    fn eligibility_blocks_changes_but_allows_uac_restore_after_gate() {
        let (_dir, state, mut e) = fixture("uac.consent", json!({"present":true,"value":0}));
        state.borrow_mut().blocked = true;
        assert_eq!(e.apply(|_, _| {}).unwrap().results[0].status, "skipped");
        assert!(e.history().unwrap().is_empty());
        state.borrow_mut().blocked = false;
        e.apply(|_, _| {}).unwrap();
        state.borrow_mut().blocked = true;
        assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "skipped");
        assert_eq!(state.borrow().writes.len(), 1);
        state.borrow_mut().blocked = false;
        assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "restored");
        assert_eq!(
            state.borrow().values["uac.consent"],
            json!({"present":true,"value":0})
        );
    }

    #[test]
    fn before_images_reject_duplicate_missing_unknown_and_executable_fields() {
        for before in [
            r#"{"present":true,"value":0,"value":1}"#,
            r#"{"present":true,"present":false,"value":null}"#,
            r#"{"present":false}"#,
            r#"{"present":true,"value":0,"command":"execute"}"#,
            r#"{"present":true,"value":"execute"}"#,
        ] {
            let line = format!(r#"{{"kind":"prepare","id":"uac.enabled","before":{before}}}"#);
            assert!(
                serde_json::from_str::<Record>(&line).is_err(),
                "accepted {before}"
            );
        }
        // Explicit absence is a real before image, not a default value.
        let line =
            r#"{"kind":"prepare","id":"uac.enabled","before":{"present":false,"value":null}}"#;
        assert!(serde_json::from_str::<Record>(line).is_ok());
    }

    #[test]
    fn all_transactions_are_validated_before_any_replay() {
        let (dir, state, mut e) = fixture(DEFENDER, json!(false));
        let older = prepare(&mut e, 1, DEFENDER, json!(true));
        let path = dir.path().join(format!("{}.jsonl", older.name));
        drop(older);
        drop(prepare(&mut e, 2, DEFENDER, json!(true)));
        let original = fs::read(&path).unwrap();
        for bad in [
            b"{not json}\n".to_vec(),
            b"{\"kind\":\"prepare\",\"id\":\"arbitrary.command\",\"before\":true}\n".to_vec(),
            b"{\"kind\":\"prepare\",\"id\":\"defender.realtime\",\"before\":\"execute\"}\n"
                .to_vec(),
            b"{\"kind\":\"applied\",\"id\":\"defender.realtime\",\"extra\":1}\n".to_vec(),
            b"{\"kind\":\"reverted\"}\n".to_vec(),
            vec![b'x'; MAX_LINE + 1],
            b"{}".to_vec(),
        ] {
            let mut corrupt = original.clone();
            corrupt.extend(bad);
            fs::write(&path, &corrupt).unwrap();
            assert!(e.revert(|_, _| {}).is_err());
            assert!(e.apply(|_, _| {}).is_err());
            assert!(e.audit().is_err());
            assert!(e.history().is_err());
            assert!(state.borrow().writes.is_empty());
            assert_eq!(fs::read(&path).unwrap(), corrupt);
        }
        fs::write(&path, &original).unwrap();
        assert!(
            Engine::open(dir.path().into(), backend(&state, &[DEFENDER], "machine-b")).is_err()
        );
        let mut lines: Vec<Value> = String::from_utf8(original)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        lines[0]["schema"] = json!(999);
        fs::write(
            &path,
            lines.iter().map(|l| format!("{l}\n")).collect::<String>(),
        )
        .unwrap();
        assert!(e.revert(|_, _| {}).is_err());
        assert!(state.borrow().writes.is_empty());
    }

    #[test]
    fn result_storage_failure_retains_recoverable_prepare() {
        let (dir, state, mut e) = fixture(DEFENDER, json!(true));
        let mut tx = prepare(&mut e, 1, DEFENDER, json!(true));
        // COW failure after the mutation but before the result is published.
        e.observe(DEFENDER).unwrap();
        e.backend.write(DEFENDER, &json!(false)).unwrap();
        IO_FAULT.with(|f| *f.borrow_mut() = Some(("snapshot_sync", 0)));
        assert!(e
            .append(
                &mut tx,
                Record::Applied {
                    id: DEFENDER.into()
                }
            )
            .is_err());
        assert!(e.history().is_err()); // poisoned instance cannot continue
        drop(tx);
        drop(e);
        let mut e = reopen(&dir, &state, &[DEFENDER]);
        assert_eq!(e.load().unwrap()[0].entries[0].state, State::Pending);
        e.revert(|_, _| {}).unwrap();
        assert_eq!(state.borrow().values[DEFENDER], json!(true));
    }

    #[test]
    fn journal_creation_failure_prevents_mutation_and_lock_is_nonblocking() {
        let (dir, state, mut e) = fixture(DEFENDER, json!(true));
        let held = e.lock().unwrap();
        assert!(e.audit().is_err());
        assert!(e.apply(|_, _| {}).is_err());
        assert!(e.revert(|_, _| {}).is_err());
        assert!(e.history().is_err());
        assert!(
            Engine::open(dir.path().into(), backend(&state, &[DEFENDER], "machine-a")).is_err()
        );
        drop(held);
        fs::create_dir(dir.path().join("invalid.jsonl")).unwrap();
        assert!(e.apply(|_, _| {}).is_err());
        assert!(state.borrow().writes.is_empty());
    }

    #[test]
    fn append_rejects_changed_length_without_repairing_the_tail() {
        for truncate in [false, true] {
            let (dir, state, mut e) = fixture(DEFENDER, json!(true));
            let mut tx = prepare(&mut e, 1, DEFENDER, json!(true));
            let path = dir.path().join(format!("{}.jsonl", tx.name));
            if truncate {
                // Truncate precisely at a valid record boundary, not only in
                // malformed JSON: appending Applied would lose its before image.
                let bytes = fs::read(&path).unwrap();
                let header_end = bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
                OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_len(header_end as u64)
                    .unwrap();
            } else {
                OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .unwrap()
                    .write_all(b"{\"kind\":")
                    .unwrap();
            }
            let damaged = fs::read(&path).unwrap();
            assert!(e
                .append(
                    &mut tx,
                    Record::Applied {
                        id: DEFENDER.into()
                    }
                )
                .is_err());
            assert_eq!(fs::read(&path).unwrap(), damaged);
            assert!(e.storage_failed);
            assert!(e.apply(|_, _| {}).is_err());
            assert!(state.borrow().writes.is_empty());
        }
    }

    #[test]
    fn legacy_partial_result_preserves_evidence_but_complete_unterminated_json_requires_review() {
        let (dir, state, mut e) = fixture(DEFENDER, json!(false));
        let tx = prepare(&mut e, 1, DEFENDER, json!(true));
        let path = dir.path().join(format!("{}.jsonl", tx.name));
        drop(tx);
        let prefix = fs::read(&path).unwrap();
        let result = b"{\"kind\":\"applied\",\"id\":\"defender.realtime\"}\n";
        for end in 1..result.len() {
            let mut torn = prefix.clone();
            torn.extend_from_slice(&result[..end]);
            fs::write(&path, &torn).unwrap();
            if end == result.len() - 1 {
                let error =
                    Engine::open(dir.path().into(), backend(&state, &[DEFENDER], "machine-a"))
                        .err()
                        .unwrap();
                assert!(error.downcast_ref::<JournalRecoveryRequired>().is_some());
                assert!(e.revert(|_, _| {}).is_err());
            } else {
                let recovered = reopen(&dir, &state, &[DEFENDER]);
                assert_eq!(recovered.load().unwrap()[0].entries[0].before, json!(true));
                let evidence = dir.path().join(format!(
                    "{}.evidence-{}",
                    path.file_stem().unwrap().to_str().unwrap(),
                    hex::encode(Sha256::digest(&torn))
                ));
                assert_eq!(fs::read(evidence).unwrap(), torn);
            }
            assert_eq!(fs::read(&path).unwrap(), torn);
            assert!(state.borrow().writes.is_empty());
        }
        // No result bytes at all is the legitimate unknown-write window.
        fs::write(&path, prefix).unwrap();
        e.revert(|_, _| {}).unwrap();
        assert_eq!(state.borrow().values[DEFENDER], json!(true));
    }

    #[test]
    fn legacy_torn_restore_records_recover_without_repeating_a_completed_write() {
        for record in [
            Record::RestorePending {
                id: DEFENDER.into(),
            },
            Record::Restored {
                id: DEFENDER.into(),
            },
            Record::Reverted,
        ] {
            let (dir, state, mut e) = fixture(DEFENDER, json!(true));
            e.apply(|_, _| {}).unwrap();
            let mut tx = e.load().unwrap().pop().unwrap();
            e.append(&mut tx, Record::Reverting).unwrap();
            let restore_happened = !matches!(&record, Record::RestorePending { .. });
            if restore_happened {
                e.append(
                    &mut tx,
                    Record::RestorePending {
                        id: DEFENDER.into(),
                    },
                )
                .unwrap();
                e.observe(DEFENDER).unwrap();
                e.backend.write(DEFENDER, &json!(true)).unwrap();
            }
            if matches!(&record, Record::Reverted) {
                e.append(
                    &mut tx,
                    Record::Restored {
                        id: DEFENDER.into(),
                    },
                )
                .unwrap();
            }
            let path = dir.path().join(format!("{}.jsonl", tx.name));
            drop(tx);
            drop(e);
            let prefix = fs::read(&path).unwrap();
            let mut bytes = serde_json::to_vec(&record).unwrap();
            bytes.push(b'\n');
            let writes = state.borrow().writes.len();
            for end in 1..bytes.len() {
                let mut torn = prefix.clone();
                torn.extend_from_slice(&bytes[..end]);
                fs::write(&path, &torn).unwrap();
                if end == bytes.len() - 1 {
                    assert!(Engine::open(
                        dir.path().into(),
                        backend(&state, &[DEFENDER], "machine-a")
                    )
                    .is_err());
                } else {
                    let mut recovered = reopen(&dir, &state, &[DEFENDER]);
                    assert!(recovered.history().unwrap()[0].ends_with(" reverting"));
                }
                assert_eq!(fs::read(&path).unwrap(), torn);
                assert_eq!(state.borrow().writes.len(), writes);
            }
            // Zero result bytes is also recoverable. A completed restore must
            // not be repeated, with or without an incomplete result append.
            fs::write(&path, &prefix).unwrap();
            let mut e = reopen(&dir, &state, &[DEFENDER]);
            e.revert(|_, _| {}).unwrap();
            assert!(e.load().unwrap()[0].reverted);
            assert_eq!(state.borrow().values[DEFENDER], json!(true));
            assert_eq!(
                state.borrow().writes.len(),
                writes + usize::from(!restore_happened)
            );
        }
    }

    #[test]
    fn repeated_failed_restores_reuse_durable_intent() {
        let (dir, state, mut e) = fixture(DEFENDER, json!(true));
        e.apply(|_, _| {}).unwrap();
        state.borrow_mut().fail_before_write = true;
        assert!(e.revert(|_, _| {}).is_err());
        let tx = e.load().unwrap().pop().unwrap();
        let path = dir.path().join(format!("{}.jsonl", tx.name));
        let intent = fs::read(&path).unwrap();
        drop(tx);
        for _ in 0..4 {
            drop(e);
            e = reopen(&dir, &state, &[DEFENDER]);
            assert!(e.revert(|_, _| {}).is_err());
            assert_eq!(fs::read(&path).unwrap(), intent);
            assert_eq!(e.apply(|_, _| {}).unwrap().results[0].status, "pending");
        }
        state.borrow_mut().fail_before_write = false;
        e.revert(|_, _| {}).unwrap();
        assert_eq!(state.borrow().writes.len(), 2);
        assert!(e.load().unwrap()[0].reverted);
    }

    #[test]
    fn noop_apply_and_empty_crash_transaction_do_not_hide_before_images() {
        let (_dir, state, mut e) = fixture(DEFENDER, json!(false));
        for _ in 0..2 {
            assert!(e.apply(|_, _| {}).unwrap().transaction.is_none());
            assert!(e.history().unwrap().is_empty());
        }
        drop(e.create(1).unwrap()); // crash after header, before first Prepare
        assert_eq!(e.apply(|_, _| {}).unwrap().results[0].status, "pending");
        assert!(state.borrow().writes.is_empty());
        e.revert(|_, _| {}).unwrap();
        assert!(e.load().unwrap()[0].reverted);
        assert!(e.apply(|_, _| {}).unwrap().transaction.is_none());
    }

    #[test]
    fn audit_marks_unknown_apply_and_interrupted_rollback_as_pending() {
        let (dir, state, mut e) = fixture(DEFENDER, json!(true));
        state.borrow_mut().fail_write = true;
        assert!(e.apply(|_, _| {}).is_err()); // mutation happened, acknowledgment failed
        drop(e);
        let mut e = reopen(&dir, &state, &[DEFENDER]);
        let report = e.audit().unwrap();
        assert_eq!(report.results[0].status, "compliant");
        assert!(report.findings.iter().any(|f| f.status == "pending"));
        assert!(e.revert(|_, _| {}).is_err()); // restore happened, acknowledgment failed
        let report = e.audit().unwrap();
        assert!(report.findings.iter().any(|f| f.status == "pending"));
        assert_eq!(state.borrow().writes.len(), 2);
        state.borrow_mut().fail_write = false;
        e.revert(|_, _| {}).unwrap();
        assert!(!e
            .audit()
            .unwrap()
            .findings
            .iter()
            .any(|f| f.title == "Journal recovery"));
        assert_eq!(state.borrow().writes.len(), 2);
    }

    #[test]
    fn findings_failure_preserves_reports_and_journal_outcomes() {
        let (dir, state, mut e) = fixture(DEFENDER, json!(true));
        state.borrow_mut().fail_findings = true;
        let audit = e
            .audit()
            .expect("Assessment transport failure must be an unknown finding");
        assert_eq!(audit.results[0].status, "attention");
        assert!(audit.findings.iter().any(|f| f.status == "unknown"));
        let applied = e
            .apply(|_, _| {})
            .expect("Completed mutation report must survive findings failure");
        assert_eq!(applied.results[0].status, "applied");
        assert!(applied.transaction.is_some());
        assert!(applied.findings.iter().any(|f| f.status == "unknown"));
        assert!(e.load().unwrap()[0].sealed);
        drop(e);
        let mut e = reopen(&dir, &state, &[DEFENDER]);
        let repeated = e.apply(|_, _| {}).unwrap();
        assert_eq!(repeated.transaction, applied.transaction);
        assert_eq!(repeated.results[0].status, "unchanged");
        let restored = e.revert(|_, _| {}).unwrap();
        assert_eq!(restored.results[0].status, "restored");
        assert!(restored.findings.iter().any(|f| f.status == "unknown"));
        assert!(e.load().unwrap()[0].reverted);
        assert_eq!(state.borrow().writes.len(), 2);
    }

    #[test]
    fn gate_rejection_does_not_poison_journal_and_recovery_never_bypasses_it() {
        let (dir, state, mut e) = fixture(FIREWALL, json!("Allow"));
        state.borrow_mut().blocked = true;
        let skipped = e.apply(|_, _| {}).unwrap();
        assert_eq!(skipped.results[0].status, "skipped");
        assert!(skipped.transaction.is_none());
        assert!(e.history().unwrap().is_empty());
        assert!(state.borrow().writes.is_empty());

        state.borrow_mut().blocked = false; // backend corrected its false positive
        let applied = e.apply(|_, _| {}).unwrap();
        assert_eq!(applied.results[0].status, "applied");
        state.borrow_mut().blocked = true;
        let skipped = e.revert(|_, _| {}).unwrap();
        assert_eq!(skipped.results[0].status, "skipped");
        assert!(skipped.findings.iter().any(|f| f.status == "pending"));
        assert_eq!(state.borrow().writes.len(), 1);
        drop(e);
        let mut e = reopen(&dir, &state, &[FIREWALL]);
        assert_eq!(e.apply(|_, _| {}).unwrap().results[0].status, "pending");
        assert_eq!(e.load().unwrap()[0].entries[0].before, json!("Allow"));
        state.borrow_mut().blocked = false;
        let restored = e.revert(|_, _| {}).unwrap();
        assert_eq!(restored.transaction, applied.transaction);
        assert_eq!(restored.results[0].status, "restored");
        assert_eq!(state.borrow().values[FIREWALL], json!("Allow"));
        assert!(e.load().unwrap()[0].reverted);
        assert_eq!(state.borrow().writes.len(), 2);
    }

    #[test]
    fn rejection_after_prepare_can_be_closed_without_a_managed_write() {
        let (dir, state, mut e) = fixture(DEFENDER, json!(true));
        state.borrow_mut().block_at = Some(2);
        assert!(e.apply(|_, _| {}).is_err());
        assert_eq!(e.load().unwrap()[0].entries[0].state, State::Pending);
        assert!(state.borrow().writes.is_empty());
        drop(e);
        let mut e = reopen(&dir, &state, &[DEFENDER]);
        assert!(e
            .audit()
            .unwrap()
            .findings
            .iter()
            .any(|f| f.status == "pending"));
        assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "unchanged");
        assert!(state.borrow().writes.is_empty());
        assert!(e.load().unwrap()[0].reverted);
        state.borrow_mut().blocked = false;
        assert_eq!(e.apply(|_, _| {}).unwrap().results[0].status, "applied");
    }

    #[test]
    fn machine_probe_failure_releases_lock_and_creates_no_transaction() {
        let (dir, state, e) = fixture(DEFENDER, json!(true));
        drop(e);
        state.borrow_mut().fail_machine = true;
        assert!(
            Engine::open(dir.path().into(), backend(&state, &[DEFENDER], "machine-a")).is_err()
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1); // lock only
        state.borrow_mut().fail_machine = false;
        let mut e = reopen(&dir, &state, &[DEFENDER]);
        assert!(e.history().unwrap().is_empty());
        assert_eq!(e.apply(|_, _| {}).unwrap().results[0].status, "applied");
    }

    #[test]
    fn sealed_success_is_informational_but_header_only_crash_requires_review() {
        let (_dir, _state, mut e) = fixture(DEFENDER, json!(true));
        e.apply(|_, _| {}).unwrap();
        let report = e.audit().unwrap();
        assert_eq!(report.results[0].status, "compliant");
        assert_eq!(
            report
                .findings
                .iter()
                .find(|f| f.title == "Journal recovery")
                .unwrap()
                .status,
            "info"
        );
        e.revert(|_, _| {}).unwrap();
        drop(e.create(2).unwrap());
        let report = e.audit().unwrap();
        assert_eq!(
            report
                .findings
                .iter()
                .find(|f| f.title == "Journal recovery")
                .unwrap()
                .status,
            "pending"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_handles_deny_delete_and_release_locks_on_drop() {
        let (dir, state, mut e) = fixture(DEFENDER, json!(true));
        let held = e.lock().unwrap();
        let lock_path = dir.path().join(LOCK_NAME);
        let other = open_file(&lock_path, false).unwrap();
        assert!(fs2::FileExt::try_lock_exclusive(&other).is_err());
        assert!(fs::remove_file(&lock_path).is_err());
        drop(held);
        fs2::FileExt::try_lock_exclusive(&other).unwrap();
        drop(other);
        e.audit().unwrap();

        let tx = prepare(&mut e, 1, DEFENDER, json!(true));
        let path = dir.path().join(format!("{}.jsonl", tx.name));
        let renamed = dir.path().join("moved");
        same_file(tx.file.as_ref().unwrap(), &path).unwrap();
        assert!(fs::rename(&path, &renamed).is_err());
        assert!(fs::remove_file(&path).is_err());
        drop(tx);
        fs::rename(&path, &renamed).unwrap();
        fs::rename(&renamed, &path).unwrap();
        assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "unchanged");
        assert!(state.borrow().writes.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn windows_native_link_count_rejects_lock_and_wal_hardlinks() {
        for lock in [false, true] {
            let (dir, state, mut e) = fixture(DEFENDER, json!(false));
            let tx = prepare(&mut e, 1, DEFENDER, json!(true));
            let path = if lock {
                dir.path().join(LOCK_NAME)
            } else {
                dir.path().join(format!("{}.jsonl", tx.name))
            };
            drop(tx);
            let outside = tempfile::tempdir().unwrap();
            let alias = outside.path().join("alias");
            fs::hard_link(&path, &alias).unwrap();
            assert!(e.revert(|_, _| {}).is_err());
            assert!(state.borrow().writes.is_empty());
            fs::remove_file(&alias).unwrap();
            assert_eq!(e.revert(|_, _| {}).unwrap().results[0].status, "restored");
        }
    }

    #[cfg(unix)]
    #[test]
    fn append_rejects_replacement_file_even_with_matching_length() {
        let (dir, _state, mut e) = fixture(DEFENDER, json!(true));
        let mut tx = prepare(&mut e, 1, DEFENDER, json!(true));
        let path = dir.path().join(format!("{}.jsonl", tx.name));
        let bytes = fs::read(&path).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let moved = outside.path().join("original");
        fs::rename(&path, &moved).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(e
            .append(
                &mut tx,
                Record::Applied {
                    id: DEFENDER.into()
                }
            )
            .is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read(&moved).unwrap(), bytes);
        assert!(e.storage_failed);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_and_hardlinks_are_rejected_for_lock_and_journals() {
        use std::os::unix::fs::symlink;
        let (dir, state, mut e) = fixture(DEFENDER, json!(true));
        let tx = prepare(&mut e, 1, DEFENDER, json!(true));
        let path = dir.path().join(format!("{}.jsonl", tx.name));
        drop(tx);
        let outside = tempfile::tempdir().unwrap();
        let linked = outside.path().join("linked");
        fs::hard_link(&path, &linked).unwrap();
        assert!(e.revert(|_, _| {}).is_err());
        fs::remove_file(&linked).unwrap();
        fs::rename(&path, &linked).unwrap();
        symlink(&linked, &path).unwrap();
        assert!(e.revert(|_, _| {}).is_err());
        assert!(state.borrow().writes.is_empty());
        fs::remove_file(dir.path().join(LOCK_NAME)).unwrap();
        symlink(&linked, dir.path().join(LOCK_NAME)).unwrap();
        assert!(e.history().is_err());
    }

    include!("engine_recovery_tests.rs");
    include!("engine_hardening_tests.rs");
}
