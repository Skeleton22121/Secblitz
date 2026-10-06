//! Privileged file work for saved copies. Paths are opened without following
//! links and each handle is checked to still be inside its folder.
use super::backup::{hex, valid_relative, FileEntry, MAX_BYTES, MAX_FILES};
use super::vault::{Item, Sink, Source};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::ffi::{c_void, OsString};
use std::fs::File;
use std::io::{Read, Write};
use std::mem::size_of;
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
        // SAFETY: the pointer came from a Windows allocation owned by this wrapper and is freed once.
        unsafe { LocalFree(self.0) };
    }
}

pub fn enable_privileges() -> Result<()> {
    // SAFETY: all calls use valid out-pointers and handles that this block owns.
    unsafe {
        let mut token: HANDLE = null_mut();
        ensure!(
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
                &mut token
            ) != 0,
            "Couldn't get permission to save app files"
        );
        let token = OwnedHandle(token);
        for name in ["SeBackupPrivilege", "SeRestorePrivilege"] {
            let mut luid = LUID {
                LowPart: 0,
                HighPart: 0,
            };
            ensure!(
                LookupPrivilegeValueW(null(), wide(name).as_ptr(), &mut luid) != 0,
                "Unknown privilege"
            );
            let tp = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES {
                    Luid: luid,
                    Attributes: SE_PRIVILEGE_ENABLED,
                }],
            };
            ensure!(
                AdjustTokenPrivileges(token.0, 0, &tp, 0, null_mut(), null_mut()) != 0,
                "Couldn't get permission to save app files"
            );
            ensure!(
                GetLastError() != ERROR_NOT_ALL_ASSIGNED,
                "Couldn't get permission to save app files"
            );
        }
    }
    Ok(())
}

struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle is owned by this wrapper and closed once.
        unsafe { CloseHandle(self.0) };
    }
}

pub fn windows_apps() -> Result<PathBuf> {
    let mut raw: windows_sys::core::PWSTR = null_mut();
    // SAFETY: `raw` is a valid out-pointer.
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFiles, 0, null_mut(), &mut raw) };
    ensure!(hr == 0 && !raw.is_null(), "Program Files not found");
    // SAFETY: `raw` is non-null and NUL-terminated, as returned by SHGetKnownFolderPath.
    let len = (0..).take_while(|&i| unsafe { *raw.add(i) } != 0).count();
    // SAFETY: the slice covers exactly the `len` units counted above.
    let path = PathBuf::from(OsString::from_wide(unsafe {
        std::slice::from_raw_parts(raw, len)
    }));
    // SAFETY: `raw` was allocated by the shell and is not used afterwards.
    unsafe { windows_sys::Win32::System::Com::CoTaskMemFree(raw.cast()) };
    ensure!(path.is_absolute(), "Program Files not found");
    Ok(path.join("WindowsApps"))
}

pub fn free_bytes(path: &Path) -> Result<u64> {
    let mut free = 0u64;
    ensure!(
        // SAFETY: the path buffer is NUL-terminated and the out-pointer is valid.
        unsafe { GetDiskFreeSpaceExW(wide(path).as_ptr(), &mut free, null_mut(), null_mut()) } != 0,
        "Couldn't read free space"
    );
    Ok(free)
}

fn open_raw(
    path: &Path,
    access: u32,
    share: u32,
    disposition: u32,
    sa: *const SECURITY_ATTRIBUTES,
    extra: u32,
) -> Result<File> {
    open_io(path, access, share, disposition, sa, extra)
        .map_err(|e| anyhow::anyhow!("{}: {}", path.display(), e))
}

fn open_io(
    path: &Path,
    access: u32,
    share: u32,
    disposition: u32,
    sa: *const SECURITY_ATTRIBUTES,
    extra: u32,
) -> std::io::Result<File> {
    // SAFETY: the path buffer is NUL-terminated and `sa` is null or points at a live SECURITY_ATTRIBUTES.
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
    if h == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `h` is a valid handle that nothing else owns.
    Ok(unsafe { File::from_raw_handle(h) })
}

fn mark_delete(f: &File) -> Result<()> {
    let d = FILE_DISPOSITION_INFO { DeleteFile: 1 };
    ensure!(
        // SAFETY: the handle is live and the struct size matches the information class.
        unsafe {
            SetFileInformationByHandle(
                f.as_raw_handle(),
                FileDispositionInfo,
                &d as *const _ as *const c_void,
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        } != 0,
        "Couldn't remove a file: {}",
        std::io::Error::last_os_error()
    );
    Ok(())
}

fn info(f: &File) -> Result<BY_HANDLE_FILE_INFORMATION> {
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut i = BY_HANDLE_FILE_INFORMATION {
        dwFileAttributes: 0,
        ftCreationTime: zero,
        ftLastAccessTime: zero,
        ftLastWriteTime: zero,
        dwVolumeSerialNumber: 0,
        nFileSizeHigh: 0,
        nFileSizeLow: 0,
        nNumberOfLinks: 0,
        nFileIndexHigh: 0,
        nFileIndexLow: 0,
    };
    ensure!(
        // SAFETY: the handle is live and `i` is a valid out-structure.
        unsafe { GetFileInformationByHandle(f.as_raw_handle(), &mut i) } != 0,
        "File information unavailable"
    );
    Ok(i)
}

fn is_link(i: &BY_HANDLE_FILE_INFORMATION) -> bool {
    i.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn is_dir(i: &BY_HANDLE_FILE_INFORMATION) -> bool {
    i.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0
}

fn final_path(f: &File) -> Result<String> {
    let mut buf = vec![0u16; 32768];
    // SAFETY: `buf` is writable for its full length.
    let n = unsafe {
        GetFinalPathNameByHandleW(f.as_raw_handle(), buf.as_mut_ptr(), buf.len() as u32, 0)
    } as usize;
    ensure!(n > 0 && n < buf.len(), "Couldn't resolve a path");
    Ok(String::from_utf16_lossy(&buf[..n]).to_lowercase())
}

fn inside(child: &File, root: &str) -> Result<()> {
    let p = final_path(child)?;
    ensure!(
        p.starts_with(root) && p[root.len()..].starts_with('\\'),
        "A link points outside the app's folder"
    );
    Ok(())
}

fn rel_path(root: &Path, rel: &str) -> Result<PathBuf> {
    ensure!(valid_relative(rel), "Unexpected name");
    Ok(rel
        .split('/')
        .fold(root.to_path_buf(), |p, part| p.join(part)))
}

fn children(dir: &Path) -> Result<Vec<(String, bool, u64, bool)>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let name = e
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Non-text file name"))?;
        let meta = std::fs::symlink_metadata(e.path())?;
        use std::os::windows::fs::MetadataExt;
        let link = meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || meta.file_type().is_symlink();
        out.push((name, meta.is_dir(), meta.len(), link));
    }
    out.sort();
    Ok(out)
}

pub fn copy_out(src: &Path, dst: &Path) -> Result<Vec<FileEntry>> {
    let root = open_raw(
        src,
        FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        OPEN_EXISTING,
        null(),
        0,
    )?;
    let ri = info(&root)?;
    ensure!(is_dir(&ri) && !is_link(&ri), "Unexpected app folder");
    let root_final = final_path(&root)?;
    std::fs::create_dir_all(dst)?;
    let mut files = Vec::new();
    let mut bytes = 0u64;
    fn walk(
        src: &Path,
        dst: &Path,
        rel: &str,
        root_final: &str,
        files: &mut Vec<FileEntry>,
        bytes: &mut u64,
        depth: usize,
    ) -> Result<()> {
        ensure!(depth <= 32, "Folder too deep");
        for (name, dir, _len, link) in children(src)? {
            ensure!(!link, "Link inside an app folder");
            let child_rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            ensure!(valid_relative(&child_rel), "Unexpected file name");
            let s = src.join(&name);
            let d = dst.join(&name);
            if dir {
                std::fs::create_dir(&d)?;
                walk(&s, &d, &child_rel, root_final, files, bytes, depth + 1)?;
            } else {
                let mut input = open_raw(
                    &s,
                    GENERIC_READ,
                    FILE_SHARE_READ,
                    OPEN_EXISTING,
                    null(),
                    FILE_FLAG_SEQUENTIAL_SCAN,
                )?;
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
                files.push(FileEntry {
                    path: child_rel,
                    size,
                    sha256: hex(&hasher.finalize()),
                });
                ensure!(
                    files.len() <= MAX_FILES && *bytes <= MAX_BYTES,
                    "App too large to save"
                );
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
        // SAFETY: the SDDL string is NUL-terminated and `p` is a valid out-pointer.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide(sddl).as_ptr(),
                1,
                &mut p,
                null_mut(),
            )
        } != 0,
        "Unexpected folder permissions"
    );
    Ok(Local(p))
}

pub fn copy_in(
    src: &Path,
    files: &[FileEntry],
    dst: &Path,
    dir_sddl: &str,
    file_sddl: &str,
) -> Result<()> {
    ensure!(!dst.exists(), "The app's folder is already there");
    let mut created = false;
    let result = (|| -> Result<()> {
        let dsd = descriptor(dir_sddl)?;
        let fsd = descriptor(file_sddl)?;
        let dsa = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: dsd.0,
            bInheritHandle: 0,
        };
        let fsa = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: fsd.0,
            bInheritHandle: 0,
        };
        let mkdir = |p: &Path, strict: bool| -> Result<()> {
            // SAFETY: the path is NUL-terminated and `dsa` outlives the call.
            if unsafe { CreateDirectoryW(wide(p).as_ptr(), &dsa) } == 0 {
                ensure!(
                    // SAFETY: GetLastError has no preconditions.
                    !strict && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS,
                    "Couldn't create {}",
                    p.display()
                );
                let d = open_raw(
                    p,
                    FILE_READ_ATTRIBUTES,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    OPEN_EXISTING,
                    null(),
                    0,
                )?;
                let i = info(&d)?;
                ensure!(is_dir(&i) && !is_link(&i), "Unexpected folder");
            }
            Ok(())
        };
        mkdir(dst, true)?; // must be brand new
        created = true;
        for f in files {
            let target = rel_path(dst, &f.path)?;
            let mut parent = dst.to_path_buf();
            let parts: Vec<&str> = f.path.split('/').collect();
            for part in &parts[..parts.len() - 1] {
                parent = parent.join(part);
                mkdir(&parent, false)?;
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
            ensure!(
                size == f.size && hex(&hasher.finalize()) == f.sha256,
                "Saved copy changed"
            );
        }
        Ok(())
    })();
    if result.is_err() && created {
        let _ = remove_tree(dst);
    }
    result
}

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
        delete_entry(path)?;
    } else {
        delete_entry(path)?; // a link is removed itself, a file is a file
    }
    Ok(())
}

fn delete_entry(path: &Path) -> Result<()> {
    let f = open_raw(
        path,
        DELETE,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        OPEN_EXISTING,
        null(),
        0,
    )?;
    mark_delete(&f)
}

pub fn security_sddl(path: &Path) -> Result<String> {
    let (mut sd, mut out) = (null_mut(), null_mut());
    let what = OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    ensure!(
        // SAFETY: the path is NUL-terminated and every out-pointer is valid.
        unsafe {
            GetNamedSecurityInfoW(
                wide(path).as_ptr(),
                SE_FILE_OBJECT,
                what,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut sd,
            )
        } == 0,
        "Couldn't read folder permissions"
    );
    let _sd = Local(sd);
    let mut len = 0u32;
    ensure!(
        // SAFETY: `sd` came from GetNamedSecurityInfoW and the out-pointers are valid.
        unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(sd, 1, what, &mut out, &mut len)
        } != 0,
        "Couldn't read folder permissions"
    );
    let _out = Local(out.cast());
    ensure!(!out.is_null(), "Couldn't read folder permissions");
    // SAFETY: `out` is non-null and holds `len` UTF-16 units from the conversion call.
    let text = unsafe { std::slice::from_raw_parts(out, len as usize) };
    Ok(String::from_utf16_lossy(text)
        .trim_end_matches('\0')
        .to_owned())
}

pub fn current_sid() -> Result<String> {
    // SAFETY: every call uses valid out-pointers, and the token and strings are freed by their guards.
    unsafe {
        let mut token: HANDLE = null_mut();
        ensure!(
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) != 0,
            "Couldn't read the current account"
        );
        let token = OwnedHandle(token);
        let mut needed = 0u32;
        GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut needed);
        ensure!(
            needed > 0 && needed < 4096,
            "Couldn't read the current account"
        );
        // u64 storage keeps the TOKEN_USER view aligned.
        let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
        ensure!(
            GetTokenInformation(
                token.0,
                TokenUser,
                buf.as_mut_ptr().cast(),
                needed,
                &mut needed
            ) != 0,
            "Couldn't read the current account"
        );
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut raw: *mut u16 = null_mut();
        ensure!(
            ConvertSidToStringSidW(user.User.Sid, &mut raw) != 0 && !raw.is_null(),
            "Couldn't read the current account"
        );
        let _free = Local(raw.cast());
        let len = (0..).take_while(|&i| *raw.add(i) != 0).count();
        Ok(String::from_utf16_lossy(std::slice::from_raw_parts(
            raw, len,
        )))
    }
}

pub fn profile_dir(sid: &str) -> Result<PathBuf> {
    ensure!(super::backup::valid_sid(sid), "Unexpected account");
    let key = format!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\ProfileList\\{sid}");
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: both strings are NUL-terminated and `buf` and `size` describe the same writable buffer.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            wide(&key).as_ptr(),
            wide("ProfileImagePath").as_ptr(),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    ensure!(status == 0, "Account folder not found");
    let len = (size as usize / 2).saturating_sub(1);
    let path = PathBuf::from(OsString::from_wide(&buf[..len]));
    ensure!(path.is_absolute(), "Account folder not found");
    Ok(path)
}

pub struct TreeSource {
    root: PathBuf,
    root_final: String,
    _pins: Vec<File>,
}

fn pin_chain(profile: &Path, family: &str) -> Result<(PathBuf, Vec<File>)> {
    let mut pins = Vec::new();
    let mut path = profile.to_path_buf();
    let p = open_raw(
        &path,
        FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | READ_CONTROL,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        OPEN_EXISTING,
        null(),
        0,
    )?;
    let i = info(&p)?;
    ensure!(is_dir(&i) && !is_link(&i), "Unexpected account folder");
    pins.push(p);
    for part in ["AppData", "Local", "Packages", family] {
        path = path.join(part);
        let p = open_raw(
            &path,
            FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | READ_CONTROL,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            OPEN_EXISTING,
            null(),
            0,
        )?;
        let i = info(&p)?;
        ensure!(
            is_dir(&i) && !is_link(&i),
            "A link was found in the app's data folder"
        );
        pins.push(p);
    }
    Ok((path, pins))
}

impl TreeSource {
    pub fn open(sid: &str, family: &str) -> Result<TreeSource> {
        let profile = profile_dir(sid)?;
        let (root, pins) = pin_chain(&profile, family)?;
        let root_final = final_path(pins.last().expect("pinned"))?;
        Ok(TreeSource {
            root,
            root_final,
            _pins: pins,
        })
    }
}

impl Source for TreeSource {
    fn items(&mut self) -> Result<Vec<Item>> {
        fn walk(dir: &Path, rel: &str, out: &mut Vec<Item>, depth: usize) -> Result<()> {
            ensure!(depth <= 32 && out.len() <= MAX_FILES, "Too much app data");
            for (name, is_dir, len, link) in children(dir)? {
                if link {
                    continue; // never follow or save links
                }
                let child = if rel.is_empty() {
                    name.clone()
                } else {
                    format!("{rel}/{name}")
                };
                if !valid_relative(&child) {
                    continue;
                }
                if is_dir {
                    out.push(Item::Dir(child.clone()));
                    walk(&dir.join(&name), &child, out, depth + 1)?;
                } else {
                    out.push(Item::File(child, len));
                }
            }
            Ok(())
        }
        let mut out = Vec::new();
        walk(&self.root, "", &mut out, 0)?;
        Ok(out)
    }

    fn read(&mut self, rel: &str, out: &mut dyn Write) -> Result<u64> {
        let path = rel_path(&self.root, rel)?;
        let mut f = open_raw(
            &path,
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            OPEN_EXISTING,
            null(),
            FILE_FLAG_SEQUENTIAL_SCAN,
        )?;
        let i = info(&f)?;
        ensure!(
            !is_link(&i) && !is_dir(&i) && i.nNumberOfLinks == 1,
            "Unexpected file in app data"
        );
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
        ensure!(
            // SAFETY: the SID string is NUL-terminated and `owner` is a valid out-pointer.
            unsafe { ConvertStringSidToSidW(wide(sid).as_ptr(), &mut owner) } != 0,
            "Unexpected account"
        );
        Ok(DataSink {
            root,
            root_final,
            owner: Local(owner),
            _pins: pins,
        })
    }

    /// Open the folder that will hold `path`, proving it is inside the app's
    /// data folder. The handle denies delete/rename sharing, so the folder
    /// cannot be replaced by a junction while the caller holds it.
    fn pin_parent(&self, path: &Path) -> Result<File> {
        let parent = path.parent().context("Unexpected name")?;
        let d = open_raw(
            parent,
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            OPEN_EXISTING,
            null(),
            0,
        )?;
        let i = info(&d)?;
        ensure!(
            is_dir(&i) && !is_link(&i),
            "A link was found in the app's data folder"
        );
        if parent != self.root {
            inside(&d, &self.root_final)?;
        }
        Ok(d)
    }

    fn give_to_owner(&self, f: &File) -> Result<()> {
        // SAFETY: the handle is live and `owner` is a valid SID kept by `self`.
        let status = unsafe {
            SetSecurityInfo(
                f.as_raw_handle(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION,
                self.owner.0,
                null_mut(),
                null(),
                null(),
            )
        };
        ensure!(status == 0, "Couldn't hand the file back to its account");
        Ok(())
    }
}

impl Sink for DataSink {
    fn dir(&mut self, rel: &str) -> Result<()> {
        let path = rel_path(&self.root, rel)?;
        // Hold the parent (no delete/rename sharing, so it cannot be swapped
        // for a junction) and prove it is inside before creating anything.
        let _parent = self.pin_parent(&path)?;
        // SAFETY: the path is NUL-terminated and a null descriptor is allowed.
        if unsafe { CreateDirectoryW(wide(&path).as_ptr(), null()) } == 0 {
            ensure!(
                // SAFETY: GetLastError has no preconditions.
                unsafe { GetLastError() } == ERROR_ALREADY_EXISTS,
                "Couldn't create a folder"
            );
        }
        let d = open_raw(
            &path,
            FILE_READ_ATTRIBUTES | WRITE_OWNER | READ_CONTROL,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            OPEN_EXISTING,
            null(),
            0,
        )?;
        let i = info(&d)?;
        ensure!(
            is_dir(&i) && !is_link(&i),
            "A link was found in the app's data folder"
        );
        inside(&d, &self.root_final)?;
        self.give_to_owner(&d)
    }

    fn file(&mut self, rel: &str, size: u64, data: &mut dyn Read) -> Result<()> {
        let path = rel_path(&self.root, rel)?;
        let _parent = self.pin_parent(&path)?;
        let access = GENERIC_WRITE | WRITE_OWNER | READ_CONTROL;
        // Replace an existing plain file (the app may have created defaults),
        // never write through a link or a hard link. Everything is checked
        // before anything is changed.
        let mut f = match open_io(&path, access, 0, OPEN_EXISTING, null(), 0) {
            Ok(f) => {
                let i = info(&f)?;
                ensure!(
                    !is_link(&i) && !is_dir(&i) && i.nNumberOfLinks == 1,
                    "A link was found in the app's data folder"
                );
                inside(&f, &self.root_final)?;
                f.set_len(0)?;
                f
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let f = open_raw(&path, access | DELETE, 0, CREATE_NEW, null(), 0)?;
                if let Err(e) = inside(&f, &self.root_final) {
                    let _ = mark_delete(&f);
                    return Err(e);
                }
                f
            }
            Err(e) => return Err(anyhow::anyhow!("{}: {}", path.display(), e)),
        };
        let written = std::io::copy(data, &mut f)?;
        ensure!(written == size, "Damaged saved data");
        f.sync_all()?;
        self.give_to_owner(&f)
    }
}
