//! Same handle-pinning/strict-DACL trust boundary as operations/storage.rs.
//! Kept local because that implementation is private. No ACL adoption/repair.
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
pub(super) fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}
fn sid(s: &str) -> Result<Local> {
    let mut p = null_mut();
    ensure!(
        unsafe { ConvertStringSidToSidW(wide(s).as_ptr(), &mut p) } != 0,
        "SID conversion failed"
    );
    Ok(Local(p))
}
fn descriptor(dir: bool) -> Result<Local> {
    let mut p = null_mut();
    let text = if dir {
        "O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"
    } else {
        "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)"
    };
    ensure!(
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide(text).as_ptr(),
                1,
                &mut p,
                null_mut(),
            )
        } != 0,
        "Descriptor construction failed"
    );
    Ok(Local(p))
}
fn inspect(f: &File, dir: bool, private: bool, ancestor: bool) -> Result<()> {
    unsafe {
        let mut i: BY_HANDLE_FILE_INFORMATION = zeroed();
        ensure!(
            GetFileInformationByHandle(f.as_raw_handle(), &mut i) != 0,
            "File information unavailable"
        );
        ensure!(
            i.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0
                && (i.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) == dir
                && (!private || dir || i.nNumberOfLinks == 1),
            "Reparse point, hardlink or wrong object type"
        );
        let (mut owner, mut acl, mut sd) = (null_mut(), null_mut(), null_mut());
        ensure!(
            GetSecurityInfo(
                f.as_raw_handle(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                null_mut(),
                &mut acl,
                null_mut(),
                &mut sd
            ) == 0,
            "Cannot inspect object security"
        );
        let _sd = Local(sd);
        let sy = sid("S-1-5-18")?;
        let ba = sid("S-1-5-32-544")?;
        let ti = sid("S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464")?;
        let trusted = |s| {
            EqualSid(s, sy.0) != 0 || EqualSid(s, ba.0) != 0 || (!private && EqualSid(s, ti.0) != 0)
        };
        ensure!(
            !owner.is_null()
                && IsValidSid(owner) != 0
                && trusted(owner)
                && !acl.is_null()
                && IsValidAcl(acl) != 0,
            "Untrusted owner/missing DACL"
        );
        let (mut control, mut rev) = (0, 0);
        ensure!(
            GetSecurityDescriptorControl(sd, &mut control, &mut rev) != 0
                && control & SE_DACL_PRESENT != 0
                && (!private || !dir || control & SE_DACL_PROTECTED != 0),
            "Private directory inheritance/missing DACL"
        );
        let (mut system, mut admins) = (false, false);
        for n in 0..(*acl).AceCount as u32 {
            let mut p = null_mut();
            ensure!(GetAce(acl, n, &mut p) != 0, "Invalid ACE");
            let h = &*(p as *const ACE_HEADER);
            ensure!(
                h.AceType <= 1 && h.AceSize as usize >= size_of::<ACCESS_ALLOWED_ACE>(),
                "Unsupported ACE"
            );
            if !private && (h.AceType == 1 || h.AceFlags & INHERIT_ONLY_ACE as u8 != 0) {
                continue;
            }
            let a = &*(p as *const ACCESS_ALLOWED_ACE);
            let s = &a.SidStart as *const u32 as PSID;
            let len = h.AceSize as usize - 8;
            ensure!(
                len >= 8 && 8 + *s.cast::<u8>().add(1) as usize * 4 <= len && IsValidSid(s) != 0,
                "Malformed ACE SID"
            );
            if private {
                ensure!(
                    h.AceType == 0
                        && h.AceFlags
                            & !(OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | INHERITED_ACE) as u8
                            == 0
                        && a.Mask == FILE_ALL_ACCESS
                        && trusted(s),
                    "Foreign private-state permissions"
                );
                ensure!(
                    !dir || h.AceFlags & (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE) as u8
                        == (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE) as u8,
                    "Nonpropagating private ACL"
                );
                system |= EqualSid(s, sy.0) != 0;
                admins |= EqualSid(s, ba.0) != 0;
            } else if !trusted(s) {
                let benign = FILE_GENERIC_READ
                    | FILE_GENERIC_EXECUTE
                    | GENERIC_READ
                    | GENERIC_EXECUTE
                    | if dir && ancestor {
                        FILE_ADD_FILE
                            | FILE_ADD_SUBDIRECTORY
                            | FILE_WRITE_EA
                            | FILE_WRITE_ATTRIBUTES
                    } else {
                        0
                    };
                ensure!(a.Mask & !benign == 0, "Untrusted object write rights");
            }
        }
        ensure!(
            !private || system && admins,
            "Missing private-state trustees"
        );
    }
    Ok(())
}
pub(super) fn pin(path: &Path, dir: bool, private: bool, ancestor: bool) -> Result<File> {
    let h = unsafe {
        CreateFileW(
            wide(path).as_ptr(),
            READ_CONTROL
                | if dir {
                    FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES
                } else {
                    GENERIC_READ
                },
            FILE_SHARE_READ | if dir { FILE_SHARE_WRITE } else { 0 },
            null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            null_mut(),
        )
    };
    ensure!(
        h != INVALID_HANDLE_VALUE,
        "Cannot pin trusted object: {}",
        std::io::Error::last_os_error()
    );
    let f = unsafe { File::from_raw_handle(h) };
    inspect(&f, dir, private, ancestor)?;
    Ok(f)
}
pub(super) fn pin_executable(path: &Path) -> Result<Vec<File>> {
    let mut parts = path.components();
    ensure!(
        matches!(parts.next(), Some(Component::Prefix(p)) if matches!(p.kind(), std::path::Prefix::Disk(_)))
            && matches!(parts.next(), Some(Component::RootDir))
            && parts.all(|p| matches!(p, Component::Normal(_))),
        "Not a local absolute executable path"
    );
    let mut prefix = PathBuf::new();
    let mut pins = Vec::new();
    for part in path.components() {
        prefix.push(part.as_os_str());
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        if matches!(part, Component::RootDir) {
            ensure!(
                unsafe { GetDriveTypeW(wide(&prefix).as_ptr()) } == 3,
                "Nonfixed system drive"
            );
        }
        pins.push(pin(
            &prefix,
            prefix != path,
            false,
            prefix != path && Some(prefix.as_path()) != path.parent(),
        )?);
    }
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    {
        let mut name = path.as_os_str().to_owned();
        name.push(".config");
        let config = PathBuf::from(name);
        if exists(&config)? {
            pins.push(pin(&config, false, false, false)?);
        }
    }
    Ok(pins)
}
pub(super) fn pin_module(manifest: &Path) -> Result<Vec<File>> {
    let mut pins = pin_executable(manifest)?;
    fn walk(root: &Path, depth: usize, pins: &mut Vec<File>) -> Result<()> {
        ensure!(depth <= 16, "System module depth cap exceeded");
        for entry in fs::read_dir(root)? {
            ensure!(pins.len() < 4096, "System module entry cap exceeded");
            let path = entry?.path();
            let directory = fs::symlink_metadata(&path)?.is_dir();
            pins.push(pin(&path, directory, false, false)?);
            if directory {
                walk(&path, depth + 1, pins)?;
            }
        }
        Ok(())
    }
    walk(
        manifest.parent().context("Missing module directory")?,
        0,
        &mut pins,
    )?;
    Ok(pins)
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
    let f = unsafe { File::from_raw_handle(h) };
    inspect(&f, false, true, false)?;
    Ok(f)
}
fn directory(path: &Path) -> Result<File> {
    let sd = descriptor(true)?;
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: 0,
    };
    if unsafe { CreateDirectoryW(wide(path).as_ptr(), &sa) } == 0 {
        ensure!(
            unsafe { GetLastError() } == ERROR_ALREADY_EXISTS,
            "Cannot create protected Patching directory"
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
fn open_lock(base: &Path) -> Result<File> {
    let path = base.join("engine.lock");
    if !exists(&path)? {
        match create(&path) {
            Ok(f) => drop(f),
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::AlreadyExists) => {}
            Err(e) => return Err(e),
        }
    }
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    inspect(&f, false, true, false)?;
    Ok(f)
}
fn tree(root: &Path, count: &mut usize, depth: usize) -> Result<()> {
    ensure!(depth <= 4, "Patching directory depth cap exceeded");
    for e in fs::read_dir(root)? {
        *count += 1;
        ensure!(*count <= 256, "Patching directory entry cap exceeded");
        let p = e?.path();
        let m = fs::symlink_metadata(&p)?;
        let _pin = pin(&p, m.is_dir(), true, false)?;
        if m.is_dir() {
            tree(&p, count, depth + 1)?;
        } else {
            ensure!(
                m.len() <= MAX_STATE_BYTES as u64,
                "Patching file cap exceeded"
            );
        }
    }
    Ok(())
}
fn load(root: &Path) -> Result<Option<Vec<u8>>> {
    let p = root.join("state.json");
    if !exists(&p)? {
        ensure!(
            fs::read_dir(root)?.all(|e| e.is_ok_and(|e| e.file_name() == "scratch")),
            "Missing state with orphan records; review required"
        );
        return Ok(None);
    }
    let mut bytes = Vec::new();
    pin(&p, false, true, false)?
        .take(MAX_STATE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_STATE_BYTES,
        "Patching state cap exceeded"
    );
    Ok(Some(bytes))
}
pub(super) struct Store {
    root: PathBuf,
    _pins: Vec<File>,
    lock: File,
}
impl Store {
    pub fn open() -> Result<Self> {
        ensure!(cfg!(target_arch = "x86_64"), "Patching requires elevated Windows x64");
        crate::platform::require_admin("Patching requires elevated Windows x64")?;
        let base = crate::platform::state_dir()?;
        let mut prefix = PathBuf::new();
        let mut pins = Vec::new();
        for part in base.components() {
            prefix.push(part.as_os_str());
            if matches!(part, Component::Prefix(_)) {
                continue;
            }
            pins.push(pin(&prefix, true, prefix == base, prefix != base)?);
        }
        let lock = open_lock(&base)?;
        fs2::FileExt::try_lock_exclusive(&lock).context("Deferred: shared engine.lock is busy")?;
        // Never inspect our own pending records here: verify() must be able to
        // resolve them. Other subsystems still veto even read-only WUA work.
        crate::updater::interlock::ensure_others_idle(
            crate::updater::interlock::Activity::Patching,
            &lock,
        )?;
        let root = base.join("Patching");
        pins.push(directory(&root)?);
        pins.push(directory(&root.join("scratch"))?);
        tree(&root, &mut 0, 0)?;
        Ok(Self {
            root,
            _pins: pins,
            lock,
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
    fn other_operations_idle(&self) -> Result<()> {
        crate::updater::interlock::ensure_others_idle(
            crate::updater::interlock::Activity::Patching,
            &self.lock,
        )
    }
    fn save(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(
            bytes.len() <= MAX_STATE_BYTES,
            "Patching state cap exceeded"
        );
        let target = self.root.join("state.json");
        if exists(&target)? {
            drop(pin(&target, false, true, false)?);
        }
        let tmp = self.root.join(format!("state-{}.tmp", Uuid::new_v4()));
        let mut f = create(&tmp)?;
        let written = f.write_all(bytes).and_then(|_| f.sync_all());
        drop(f);
        let result = written.map_err(anyhow::Error::from).and_then(|_| {
            ensure!(
                unsafe {
                    MoveFileExW(
                        wide(&tmp).as_ptr(),
                        wide(&target).as_ptr(),
                        MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                    )
                } != 0,
                "Atomic Patching publication failed"
            );
            Ok(())
        });
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }
}
pub(super) fn ensure_idle(held: &File) -> Result<()> {
    // Leaf inspector, not Store::open: never re-enter another subsystem's gate.
    let base = crate::updater::inspect_engine_lock(held)?;
    let root = base.base.join("Patching");
    if exists(&root)? {
        let _root = pin(&root, true, true, false)?;
        tree(&root, &mut 0, 0)?;
        if let Some(bytes) = load(&root)? {
            core::idle(&bytes, &windows::machine()?)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "elevated Windows synthetic module ACL test; no WUA/scripts executed"]
    fn module_payload_and_executable_config_are_independently_pinned_and_checked() {
        let namespace = crate::platform::state_dir().unwrap().join("Patching");
        let namespace_pin = directory(&namespace).unwrap();
        let scratch = namespace.join("scratch");
        let scratch_pin = directory(&scratch).unwrap();
        let root = scratch.join(format!("pin-test-{}", Uuid::new_v4()));
        let root_pin = directory(&root).unwrap();
        let manifest = root.join("sample.psd1");
        let payload = root.join("sample.psm1");
        let exe = root.join("sample.exe");
        let config = root.join("sample.exe.config");
        for path in [&manifest, &payload, &exe, &config] {
            drop(create(path).unwrap());
        }
        let pins = pin_module(&manifest).unwrap();
        assert!(OpenOptions::new().write(true).open(&payload).is_err());
        assert!(fs::remove_file(&payload).is_err());
        drop(pins);
        let pins = pin_executable(&exe).unwrap();
        assert!(OpenOptions::new().write(true).open(&config).is_err());
        drop(pins);
        for path in [&payload, &config] {
            fs::remove_file(path).unwrap();
            let mut sd = null_mut();
            assert_ne!(
                unsafe {
                    ConvertStringSecurityDescriptorToSecurityDescriptorW(
                        wide("O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;BU)").as_ptr(),
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
            let h = unsafe {
                CreateFileW(
                    wide(path).as_ptr(),
                    GENERIC_WRITE,
                    0,
                    &sa,
                    CREATE_NEW,
                    0,
                    null_mut(),
                )
            };
            assert_ne!(h, INVALID_HANDLE_VALUE);
            drop(unsafe { File::from_raw_handle(h) });
            if path == &payload {
                assert!(pin_module(&manifest).is_err());
            } else {
                assert!(pin_executable(&exe).is_err());
            }
            fs::remove_file(path).unwrap();
        }
        fs::remove_file(manifest).unwrap();
        fs::remove_file(exe).unwrap();
        drop(root_pin);
        fs::remove_dir(root).unwrap();
        drop((scratch_pin, namespace_pin));
    }
}
