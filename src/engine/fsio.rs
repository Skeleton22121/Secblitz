//! Safe file primitives for the journal directory: link and reparse-point checks, identity pinning, durable publish.

use super::MAX_WAL;
use anyhow::{ensure, Context, Result};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::Path,
};
pub(super) fn metadata_safe(m: &Metadata, directory: bool) -> Result<()> {
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

#[cfg(windows)]
fn handle_information(
    file: &File,
    what: &str,
) -> Result<windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    // SAFETY: the struct is plain integers, so all-zero is valid; the call below fills it.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `file` owns a live handle for the whole call and `info` is a valid out pointer.
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) };
    ensure!(
        ok != 0,
        "Cannot inspect journal {what}: {}",
        std::io::Error::last_os_error()
    );
    Ok(info)
}

pub(super) fn file_safe(file: &File) -> Result<()> {
    metadata_safe(&file.metadata()?, false)?;
    #[cfg(windows)]
    {
        // std's by-handle link-count metadata is not stable on all supported
        // toolchains. Use the native query for this one additional check.
        let info = handle_information(file, "handle")?;
        ensure!(info.nNumberOfLinks == 1, "Journal hard links are forbidden");
    }
    Ok(())
}

pub(super) fn open_file(path: &Path, create: bool) -> Result<File> {
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
pub(super) fn validate_update_file(path: &Path) -> Result<()> {
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

pub(super) fn same_file(file: &File, path: &Path) -> Result<()> {
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
        let current = open_file(path, false)?;
        let identity = |f: &File| -> Result<(u32, u32, u32)> {
            let info = handle_information(f, "identity")?;
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

pub(super) fn sync_directory(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(dir)?.sync_all()?;
    // Windows files use WRITE_THROUGH and FlushFileBuffers via sync_all;
    // snapshot publication also uses MOVEFILE_WRITE_THROUGH. Windows does not
    // support the Unix directory-fsync contract.
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

pub(super) fn read_bytes(file: &mut File) -> Result<Vec<u8>> {
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

pub(super) fn publish_snapshot(from: &Path, to: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: both buffers are NUL-terminated UTF-16 that outlive the call.
        let moved = unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        ensure!(
            moved != 0,
            "Publish journal snapshot: {}",
            std::io::Error::last_os_error()
        );
    }
    #[cfg(not(windows))]
    fs::rename(from, to)?;
    Ok(())
}

#[cfg(test)]
use anyhow::bail;
#[cfg(test)]
use std::io::Write;

#[cfg(test)]
thread_local! {
    pub(super) static IO_FAULT: std::cell::RefCell<Option<(&'static str, usize)>> = const { std::cell::RefCell::new(None) };
}

pub(super) fn io_boundary(point: &'static str) -> Result<()> {
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
pub(super) fn write_snapshot_with_fault(writer: &mut impl Write, bytes: &[u8]) -> Result<()> {
    // A short write then failure must leave the same prefix as a byte-at-a-time injector. One WRITE_THROUGH syscall per byte would make even unarmed tests perform tens of thousands of disk flushes.
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
