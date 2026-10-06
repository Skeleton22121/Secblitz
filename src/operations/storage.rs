//! Handle-pinned private state. Privileged administrators are outside this trust
//! boundary; unprivileged users cannot supply journal contents or replace pins.
use super::*;
use std::{
    ffi::c_void,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Component, Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
};

pub(super) struct Local(pub *mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
pub(super) fn wide(text: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}
fn sid(text: &str) -> Result<Local> {
    let mut value = null_mut();
    ensure!(
        unsafe { ConvertStringSidToSidW(wide(text).as_ptr(), &mut value) } != 0,
        "SID conversion failed"
    );
    Ok(Local(value))
}
fn descriptor(directory: bool) -> Result<Local> {
    let text = if directory {
        "O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"
    } else {
        "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)"
    };
    let mut value = null_mut();
    ensure!(
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide(text).as_ptr(),
                1,
                &mut value,
                null_mut(),
            )
        } != 0,
        "Security descriptor construction failed"
    );
    Ok(Local(value))
}

fn inspect(file: &File, directory: bool, strict: bool, ancestor: bool) -> Result<()> {
    unsafe {
        let mut info: BY_HANDLE_FILE_INFORMATION = zeroed();
        ensure!(
            GetFileInformationByHandle(file.as_raw_handle(), &mut info) != 0,
            "File information unavailable"
        );
        ensure!(
            info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0
                && (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) == directory,
            "Reparse point or wrong object type"
        );
        ensure!(
            !strict || directory || info.nNumberOfLinks == 1,
            "Hardlinked operation state rejected"
        );
        let (mut owner, mut acl, mut sd) = (null_mut(), null_mut(), null_mut());
        ensure!(
            GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                null_mut(),
                &mut acl,
                null_mut(),
                &mut sd
            ) == 0,
            "Security inspection failed"
        );
        let _sd = Local(sd);
        let system = sid("S-1-5-18")?;
        let admins = sid("S-1-5-32-544")?;
        let installer = sid("S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464")?;
        let trusted = |s: PSID| {
            EqualSid(s, system.0) != 0
                || EqualSid(s, admins.0) != 0
                || (!strict && EqualSid(s, installer.0) != 0)
        };
        ensure!(
            !owner.is_null() && IsValidSid(owner) != 0 && trusted(owner),
            "Untrusted object owner"
        );
        ensure!(
            !acl.is_null() && IsValidAcl(acl) != 0,
            "Missing or invalid DACL"
        );
        let (mut control, mut revision) = (0, 0);
        ensure!(
            GetSecurityDescriptorControl(sd, &mut control, &mut revision) != 0
                && control & SE_DACL_PRESENT != 0,
            "Missing DACL"
        );
        if strict && directory {
            ensure!(
                control & SE_DACL_PROTECTED != 0,
                "Private directory permits inheritance"
            );
        }
        let (mut seen_system, mut seen_admins) = (false, false);
        for n in 0..(*acl).AceCount as u32 {
            let mut pointer = null_mut();
            ensure!(GetAce(acl, n, &mut pointer) != 0, "ACE query failed");
            let header = &*(pointer as *const ACE_HEADER);
            ensure!(
                header.AceType <= 1 && header.AceSize as usize >= size_of::<ACCESS_ALLOWED_ACE>(),
                "Unsupported ACE"
            );
            if !strict && (header.AceType == 1 || header.AceFlags & INHERIT_ONLY_ACE as u8 != 0) {
                continue;
            }
            let ace = &*(pointer as *const ACCESS_ALLOWED_ACE);
            let trustee = &ace.SidStart as *const u32 as PSID;
            let length = header.AceSize as usize - 8;
            ensure!(
                length >= 8
                    && 8 + *trustee.cast::<u8>().add(1) as usize * 4 <= length
                    && IsValidSid(trustee) != 0,
                "Malformed ACE SID"
            );
            if strict {
                ensure!(
                    header.AceType == 0
                        && header.AceFlags
                            & !(OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | INHERITED_ACE) as u8
                            == 0
                        && ace.Mask == FILE_ALL_ACCESS
                        && trusted(trustee),
                    "Private state has foreign permissions"
                );
                if directory {
                    ensure!(
                        header.AceFlags & (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE) as u8
                            == (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE) as u8,
                        "Private directory does not propagate ACL"
                    );
                }
                seen_system |= EqualSid(trustee, system.0) != 0;
                seen_admins |= EqualSid(trustee, admins.0) != 0;
            } else if !trusted(trustee) {
                let benign = FILE_GENERIC_READ
                    | FILE_GENERIC_EXECUTE
                    | GENERIC_READ
                    | GENERIC_EXECUTE
                    | if directory && ancestor {
                        FILE_ADD_FILE
                            | FILE_ADD_SUBDIRECTORY
                            | FILE_WRITE_EA
                            | FILE_WRITE_ATTRIBUTES
                    } else {
                        0
                    };
                ensure!(ace.Mask & !benign == 0, "Untrusted object write rights");
            }
        }
        ensure!(
            !strict || seen_system && seen_admins,
            "Missing private state trustees"
        );
    }
    Ok(())
}

pub(super) fn pin(path: &Path, directory: bool, strict: bool, ancestor: bool) -> Result<File> {
    let access = READ_CONTROL
        | if directory {
            FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES
        } else {
            GENERIC_READ
        };
    let sharing = FILE_SHARE_READ | if directory { FILE_SHARE_WRITE } else { 0 };
    let handle = unsafe {
        CreateFileW(
            wide(path).as_ptr(),
            access,
            sharing,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            null_mut(),
        )
    };
    ensure!(
        handle != INVALID_HANDLE_VALUE,
        "Cannot pin trusted object: {}",
        std::io::Error::last_os_error()
    );
    let file = unsafe { File::from_raw_handle(handle) };
    inspect(&file, directory, strict, ancestor)?;
    Ok(file)
}

pub(crate) fn pin_system_executable(path: &Path) -> Result<Vec<File>> {
    let mut parts = path.components();
    ensure!(
        matches!(parts.next(), Some(Component::Prefix(p)) if matches!(p.kind(), std::path::Prefix::Disk(_)))
            && matches!(parts.next(), Some(Component::RootDir))
            && parts.all(|p| matches!(p, Component::Normal(_))),
        "Executable is not a local absolute path"
    );
    let mut prefix = PathBuf::new();
    let mut handles = Vec::new();
    for part in path.components() {
        prefix.push(part.as_os_str());
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        if matches!(part, Component::RootDir) {
            ensure!(
                unsafe { GetDriveTypeW(wide(&prefix).as_ptr()) } == 3,
                "System executable requires a fixed local drive"
            );
        }
        handles.push(pin(
            &prefix,
            prefix != path,
            false,
            prefix != path && Some(prefix.as_path()) != path.parent(),
        )?);
    }
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    {
        let mut config = path.as_os_str().to_os_string();
        config.push(".config");
        let config = PathBuf::from(config);
        if exists(&config)? {
            handles.push(pin(&config, false, false, false)?);
        }
    }
    Ok(handles)
}

/// Pin every module payload, not just the manifest that imports it. Directory
/// ACLs alone do not establish the ACL of an already existing psm1/DLL file.
pub(crate) fn pin_system_module(path: &Path) -> Result<Vec<File>> {
    let mut handles = pin_system_executable(path)?;
    fn walk(root: &Path, depth: usize, handles: &mut Vec<File>) -> Result<()> {
        ensure!(depth <= 16, "System module depth cap exceeded");
        for entry in fs::read_dir(root)? {
            ensure!(handles.len() < 4096, "System module entry cap exceeded");
            let path = entry?.path();
            let directory = fs::symlink_metadata(&path)?.is_dir();
            handles.push(pin(&path, directory, false, false)?);
            if directory {
                walk(&path, depth + 1, handles)?;
            }
        }
        Ok(())
    }
    walk(
        path.parent().context("Missing module directory")?,
        0,
        &mut handles,
    )?;
    Ok(handles)
}

fn create(path: &Path) -> Result<File> {
    let sd = descriptor(false)?;
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: 0,
    };
    let h = unsafe {
        CreateFileW(
            wide(path).as_ptr(),
            GENERIC_READ | GENERIC_WRITE | READ_CONTROL,
            0,
            &sa,
            CREATE_NEW,
            FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error().into());
    }
    let file = unsafe { File::from_raw_handle(h) };
    inspect(&file, false, true, false)?;
    Ok(file)
}
pub(super) fn directory(path: &Path) -> Result<File> {
    let sd = descriptor(true)?;
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: 0,
    };
    if unsafe { CreateDirectoryW(wide(path).as_ptr(), &sa) } == 0 {
        ensure!(
            unsafe { GetLastError() } == ERROR_ALREADY_EXISTS,
            "Cannot create protected operations directory"
        );
    }
    pin(path, true, true, false)
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
fn lock(base: &Path) -> Result<File> {
    let path = base.join("engine.lock");
    if !exists(&path)? {
        match create(&path) {
            Ok(file) => drop(file),
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::AlreadyExists) => {}
            Err(e) => return Err(e),
        }
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    inspect(&file, false, true, false)?;
    fs2::FileExt::try_lock_exclusive(&file).context("Deferred: shared engine.lock is busy")?;
    Ok(file)
}
fn inspect_tree(root: &Path, count: &mut usize, depth: usize) -> Result<()> {
    ensure!(depth <= 4, "Operations directory depth cap exceeded");
    for entry in fs::read_dir(root)? {
        *count += 1;
        ensure!(*count <= 256, "Operations directory entry cap exceeded");
        let path = entry?.path();
        let metadata = fs::symlink_metadata(&path)?;
        let directory = metadata.is_dir();
        let _pin = pin(&path, directory, true, false)?;
        if directory {
            inspect_tree(&path, count, depth + 1)?;
        } else {
            ensure!(
                metadata.len() <= 16 * 1024 * 1024,
                "Operations file cap exceeded"
            );
        }
    }
    Ok(())
}

pub(super) struct Store {
    root: PathBuf,
    _pins: Vec<File>,
    _lock: File,
}
impl Store {
    pub fn open() -> Result<Self> {
        ensure!(cfg!(target_arch = "x86_64"), "Operations require elevated Windows x64");
        crate::platform::require_admin("Operations require elevated Windows x64")?;
        let base = crate::platform::state_dir()?;
        let mut pins = Vec::new();
        let mut prefix = PathBuf::new();
        for part in base.components() {
            prefix.push(part.as_os_str());
            if matches!(part, Component::Prefix(_)) {
                continue;
            }
            pins.push(pin(&prefix, true, prefix == base, prefix != base)?);
        }
        let lock = lock(&base)?;
        // Inspect only OTHER subsystems here. Opening our own durable state must
        // remain possible for resume/verification of a lost supervisor's intent.
        crate::updater::interlock::ensure_others_idle(
            crate::updater::interlock::Activity::Operations,
            &lock,
        )?;
        let root = base.join("operations");
        pins.push(directory(&root)?);
        pins.push(directory(&root.join("scratch"))?);
        inspect_tree(&root, &mut 0, 0)?;
        Ok(Self {
            root,
            _pins: pins,
            _lock: lock,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl core::Storage for Store {
    fn load(&mut self) -> Result<Option<Vec<u8>>> {
        load(&self.root)
    }
    fn save(&mut self, bytes: &[u8]) -> Result<()> {
        publish(&self.root, bytes, |_| Ok(()))
    }
}

fn load(root: &Path) -> Result<Option<Vec<u8>>> {
    let path = root.join("state.json");
    if !exists(&path)? {
        ensure!(
            fs::read_dir(root)?.all(|e| e.is_ok_and(|e| e.file_name() == "scratch")),
            "Missing state with orphan records; review required"
        );
        return Ok(None);
    }
    let mut bytes = Vec::new();
    pin(&path, false, true, false)?
        .take(MAX_STATE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_STATE_BYTES,
        "Operations state exceeds cap"
    );
    Ok(Some(bytes))
}

pub(super) fn ensure_update_idle(shared_engine_lock: &File) -> Result<()> {
    let base = crate::updater::inspect_engine_lock(shared_engine_lock)?;
    let root = base.base.join("operations");
    if exists(&root)? {
        let _root = pin(&root, true, true, false)?;
        inspect_tree(&root, &mut 0, 0)?;
        if let Some(bytes) = load(&root)? {
            let backend = windows::Backend::new(&root)?;
            core::update_idle(&bytes, &core::Backend::machine(&backend)?)?;
        }
    }
    windows::ensure_no_servicing_processes()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Publication {
    Created,
    Written,
    Flushed,
    BeforeRename,
    Published,
}

fn publish(
    root: &Path,
    bytes: &[u8],
    mut boundary: impl FnMut(Publication) -> Result<()>,
) -> Result<()> {
    ensure!(
        bytes.len() <= MAX_STATE_BYTES,
        "Operations state exceeds cap"
    );
    let target = root.join("state.json");
    if exists(&target)? {
        drop(pin(&target, false, true, false)?);
    }
    let temp = root.join(format!("state-{}.tmp", Uuid::new_v4()));
    // Own only our exclusively created object; never remove an existing name
    // when CREATE_NEW failed, even in the improbable event of UUID collision.
    let mut file = create(&temp)?;
    let result = (|| {
        boundary(Publication::Created)?;
        file.write_all(bytes)?;
        boundary(Publication::Written)?;
        file.sync_all()?;
        boundary(Publication::Flushed)?;
        Ok(())
    })();
    drop(file);
    let result = result.and_then(|_| {
        boundary(Publication::BeforeRename)?;
        ensure!(
            unsafe {
                MoveFileExW(
                    wide(&temp).as_ptr(),
                    wide(&target).as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } != 0,
            "Atomic operations publication failed"
        );
        boundary(Publication::Published)
    });
    // A crash can leave a capped, strictly protected temp. Never parse it.
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestRoot {
        root: PathBuf,
        pin: Option<File>,
    }
    impl TestRoot {
        fn new() -> Self {
            assert!(
                crate::platform::is_elevated().unwrap(),
                "Run these storage-only tests elevated in the coordinator's Windows VM"
            );
            let root = crate::platform::state_dir()
                .unwrap()
                .join(format!("operations-test-{}", Uuid::new_v4()));
            let pin = Some(directory(&root).unwrap());
            Self { root, pin }
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            self.pin.take();
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    #[ignore = "coordinator Windows VM: elevated native storage fault test; no maintenance commands"]
    fn atomic_publication_failure_at_every_native_boundary() {
        let root = TestRoot::new();
        for boundary in [
            Publication::Created,
            Publication::Written,
            Publication::Flushed,
            Publication::BeforeRename,
            Publication::Published,
        ] {
            publish(&root.root, b"old", |_| Ok(())).unwrap();
            assert!(publish(&root.root, b"new", |at| {
                ensure!(at != boundary, "injected I/O publication fault");
                Ok(())
            })
            .is_err());
            let mut bytes = Vec::new();
            pin(&root.root.join("state.json"), false, true, false)
                .unwrap()
                .read_to_end(&mut bytes)
                .unwrap();
            assert_eq!(
                bytes,
                if boundary == Publication::Published {
                    b"new"
                } else {
                    b"old"
                }
            );
            assert_eq!(fs::read_dir(&root.root).unwrap().count(), 1);
        }
    }
    #[test]
    #[ignore = "coordinator Windows VM: elevated native link/lock test; no maintenance commands"]
    fn shared_fs2_lock_and_links_reject_without_repair() {
        let root = TestRoot::new();
        let first = lock(&root.root).unwrap();
        assert!(lock(&root.root).is_err());
        drop(first);
        let second = lock(&root.root).unwrap();
        drop(second);
        let target = root.root.join("target");
        drop(create(&target).unwrap());
        let alias = root.root.join("alias");
        fs::hard_link(&target, &alias).unwrap();
        assert!(pin(&target, false, true, false).is_err());
        assert!(pin(&alias, false, true, false).is_err());
        fs::remove_file(&alias).unwrap();
        let link = root.root.join("link");
        std::os::windows::fs::symlink_file(&target, &link).unwrap();
        assert!(pin(&link, false, true, false).is_err());
        fs::remove_file(&link).unwrap();
        let dir_link = root.root.join("dir-link");
        std::os::windows::fs::symlink_dir(&root.root, &dir_link).unwrap();
        assert!(pin(&dir_link, true, true, false).is_err());
        fs::remove_dir(dir_link).unwrap();
    }
    #[test]
    #[ignore = "coordinator Windows VM: elevated native hostile ACL test; no maintenance commands"]
    fn foreign_dacl_is_rejected_without_adoption() {
        let root = TestRoot::new();
        let path = root.root.join("untrusted");
        let mut sd = null_mut();
        assert_ne!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    wide("O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;BU)").as_ptr(),
                    1,
                    &mut sd,
                    null_mut(),
                )
            },
            0
        );
        let sd = Local(sd);
        let sa = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: 0,
        };
        assert_ne!(unsafe { CreateDirectoryW(wide(&path).as_ptr(), &sa) }, 0);
        assert!(directory(&path).is_err());
        assert!(pin(&path, true, true, false).is_err());
    }

    #[test]
    #[ignore = "native elevated Windows module ACL/pinning test; no scripts executed"]
    fn module_payload_is_checked_and_pinned_not_just_its_manifest() {
        let root = TestRoot::new();
        let manifest = root.root.join("sample.psd1");
        let payload = root.root.join("sample.psm1");
        drop(create(&manifest).unwrap());
        drop(create(&payload).unwrap());
        let pins = pin_system_module(&manifest).unwrap();
        assert!(OpenOptions::new().write(true).open(&payload).is_err());
        assert!(fs::remove_file(&payload).is_err());
        drop(pins);
        fs::remove_file(&payload).unwrap();
        let mut raw = null_mut();
        assert_ne!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    wide("O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;BU)").as_ptr(),
                    1,
                    &mut raw,
                    null_mut(),
                )
            },
            0
        );
        let sd = Local(raw);
        let sa = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: 0,
        };
        let handle = unsafe {
            CreateFileW(
                wide(&payload).as_ptr(),
                GENERIC_WRITE,
                0,
                &sa,
                CREATE_NEW,
                0,
                null_mut(),
            )
        };
        assert_ne!(handle, INVALID_HANDLE_VALUE);
        drop(unsafe { File::from_raw_handle(handle) });
        assert!(pin_system_module(&manifest).is_err());
    }
}
