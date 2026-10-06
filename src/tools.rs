//! Optional, explicitly requested desktop software installation.

pub fn install_bitwarden() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        windows::install()
    }
    #[cfg(not(windows))]
    anyhow::bail!("Bitwarden installation is supported only on Windows")
}

pub fn bitwarden_installed() -> anyhow::Result<bool> {
    #[cfg(windows)]
    {
        windows::known_install()
    }
    #[cfg(not(windows))]
    Ok(false)
}

pub fn bitwarden_installable() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        windows::install_check()
    }
    #[cfg(not(windows))]
    Err(NotHere.into())
}

#[cfg(windows)]
pub fn winget_path() -> anyhow::Result<std::path::PathBuf> {
    windows::winget()
}

#[derive(Debug)]
pub struct Offline;

impl std::fmt::Display for Offline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("network_unreachable")
    }
}

impl std::error::Error for Offline {}

#[derive(Debug)]
pub struct NotHere;

impl std::fmt::Display for NotHere {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("not_available_for_this_account")
    }
}

impl std::error::Error for NotHere {}

pub fn is_not_here_error(error: &anyhow::Error) -> bool {
    error.downcast_ref::<NotHere>().is_some()
}

pub fn is_offline_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| cause.is::<Offline>())
}

pub fn is_offline_code(code: u32) -> bool {
    matches!(
        code,
        0x8007_2EE7 | 0x8007_2EE2 | 0x8007_2EFD | 0x8007_2EFE | 0x8007_2EFF | 0x8007_2EE9
    )
}

pub fn dns_offline() -> bool {
    use std::net::ToSocketAddrs;
    match ("cdn.winget.microsoft.com", 443).to_socket_addrs() {
        Ok(mut addresses) => addresses.next().is_none(),
        Err(_) => true,
    }
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug)]
enum Operation {
    Source,
    List,
    Install,
}

#[cfg(any(windows, test))]
impl Operation {
    fn args(self) -> &'static [&'static str] {
        match self {
            Self::Source => &[
                "source",
                "export",
                "--name",
                "winget",
                "--disable-interactivity",
            ],
            Self::List => &[
                "list",
                "--id",
                "Bitwarden.Bitwarden",
                "--exact",
                "--source",
                "winget",
                "--accept-source-agreements",
                "--disable-interactivity",
            ],
            Self::Install => &[
                "install",
                "--id",
                "Bitwarden.Bitwarden",
                "--exact",
                "--source",
                "winget",
                "--scope",
                "user",
                "--silent",
                "--accept-package-agreements",
                "--accept-source-agreements",
                "--disable-interactivity",
                "--no-upgrade",
            ],
        }
    }
}

#[cfg(any(windows, test))]
fn list_found(code: u32) -> anyhow::Result<bool> {
    match code {
        0 => Ok(true),
        0x8A15_0014 => Ok(false),
        _ => anyhow::bail!("WinGet Bitwarden detection failed (exit 0x{code:08X})"),
    }
}

#[cfg(any(windows, test))]
fn verify_source(output: &[u8]) -> anyhow::Result<()> {
    use anyhow::{ensure, Context};
    let output = output.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(output);
    let source: serde_json::Value = serde_json::from_slice(output)
        .context("WinGet source export did not return a single JSON source")?;
    for (key, expected) in [
        ("Name", "winget"),
        ("Arg", "https://cdn.winget.microsoft.com/cache"),
        ("Type", "Microsoft.PreIndexed.Package"),
        ("Identifier", "Microsoft.Winget.Source_8wekyb3d8bbwe"),
    ] {
        ensure!(
            source[key].as_str() == Some(expected),
            "WinGet repository verification failed: unexpected {key}: {}",
            source[key]
        );
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn package_executable(root: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
    use anyhow::{ensure, Context};
    ensure!(
        root.is_absolute(),
        "App Installer package path is not absolute"
    );
    let root = root
        .canonicalize()
        .context("Resolve App Installer package directory")?;
    let exe = root
        .join("winget.exe")
        .canonicalize()
        .context("Resolve packaged winget.exe")?;
    ensure!(
        exe.parent() == Some(root.as_path()) && exe.is_file(),
        "Packaged winget.exe escapes its registered package directory or is not a file"
    );
    Ok(exe)
}

#[cfg(windows)]
mod windows {
    use super::{list_found, package_executable, verify_source, Operation};
    use anyhow::{bail, ensure, Context, Result};
    use std::{
        ffi::{c_void, OsString},
        fs::File,
        io::Read,
        mem::{size_of, zeroed},
        os::windows::{
            ffi::{OsStrExt, OsStringExt},
            io::{AsRawHandle, FromRawHandle},
        },
        path::{Path, PathBuf},
        ptr::{null, null_mut},
        time::{Duration, Instant},
    };
    use windows_sys::{
        core::GUID,
        Win32::{
            Foundation::{
                CloseHandle, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, WAIT_OBJECT_0,
                WAIT_TIMEOUT,
            },
            Security::{
                EqualSid, GetTokenInformation, TokenElevation, TokenUser, SECURITY_ATTRIBUTES,
                TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER,
            },
            System::Threading::{
                CreateProcessW, GetCurrentProcess, GetExitCodeProcess, OpenProcess,
                OpenProcessToken, ResumeThread, TerminateProcess, WaitForSingleObject,
                CREATE_NO_WINDOW, CREATE_SUSPENDED, IO_COUNTERS, PROCESS_INFORMATION,
                PROCESS_QUERY_LIMITED_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOW,
            },
            UI::{
                Shell::{
                    FOLDERID_LocalAppData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86,
                    SHGetKnownFolderPath,
                },
                WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId},
            },
        },
    };

    #[link(name = "ole32")]
    extern "system" {
        fn CoTaskMemFree(memory: *const c_void);
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetPackagesByPackageFamily(
            family: *const u16,
            count: *mut u32,
            names: *mut *mut u16,
            length: *mut u32,
            buffer: *mut u16,
        ) -> i32;
        fn GetPackagePathByFullName(name: *const u16, length: *mut u32, path: *mut u16) -> i32;
        fn OpenPackageInfoByFullName(
            name: *const u16,
            reserved: u32,
            reference: *mut *mut c_void,
        ) -> u32;
        fn GetPackageInfo(
            reference: *const c_void,
            flags: u32,
            length: *mut u32,
            buffer: *mut u8,
            count: *mut u32,
        ) -> u32;
        fn ClosePackageInfo(reference: *const c_void) -> u32;
        fn CreateJobObjectW(attributes: *const SECURITY_ATTRIBUTES, name: *const u16) -> HANDLE;
        fn SetInformationJobObject(
            job: HANDLE,
            class: i32,
            info: *const c_void,
            length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: HANDLE, process: HANDLE) -> i32;
        fn CreatePipe(
            read: *mut HANDLE,
            write: *mut HANDLE,
            attributes: *const SECURITY_ATTRIBUTES,
            size: u32,
        ) -> i32;
        fn PeekNamedPipe(
            pipe: HANDLE,
            buffer: *mut c_void,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            remaining: *mut u32,
        ) -> i32;
    }

    #[repr(C)]
    struct BasicLimits {
        process_time: i64,
        job_time: i64,
        flags: u32,
        min_working_set: usize,
        max_working_set: usize,
        active_processes: u32,
        affinity: usize,
        priority: u32,
        scheduling: u32,
    }
    #[repr(C)]
    struct ExtendedLimits {
        basic: BasicLimits,
        io: IO_COUNTERS,
        process_memory: usize,
        job_memory: usize,
        peak_process_memory: usize,
        peak_job_memory: usize,
    }

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    fn check(ok: i32, operation: &str) -> Result<()> {
        if ok == 0 {
            return Err(std::io::Error::last_os_error()).context(operation.to_owned());
        }
        Ok(())
    }

    fn token(process: HANDLE) -> Result<Handle> {
        let mut value = null_mut();
        check(
            unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut value) },
            "OpenProcessToken",
        )?;
        Ok(Handle(value))
    }

    fn token_user(token: &Handle) -> Result<Vec<usize>> {
        let mut bytes = 0;
        unsafe {
            GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut bytes);
        }
        ensure!(
            bytes >= size_of::<TOKEN_USER>() as u32 && bytes <= 65536,
            "Invalid TokenUser buffer size: {bytes}"
        );
        let mut buffer = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
        check(
            unsafe {
                GetTokenInformation(
                    token.0,
                    TokenUser,
                    buffer.as_mut_ptr().cast(),
                    bytes,
                    &mut bytes,
                )
            },
            "GetTokenInformation(TokenUser)",
        )?;
        Ok(buffer)
    }

    fn require_desktop_user() -> Result<()> {
        let current = token(unsafe { GetCurrentProcess() })?;
        let mut elevation: TOKEN_ELEVATION = unsafe { zeroed() };
        let mut bytes = 0;
        check(
            unsafe {
                GetTokenInformation(
                    current.0,
                    TokenElevation,
                    (&mut elevation as *mut TOKEN_ELEVATION).cast(),
                    size_of::<TOKEN_ELEVATION>() as u32,
                    &mut bytes,
                )
            },
            "GetTokenInformation(TokenElevation)",
        )?;
        ensure!(elevation.TokenIsElevated == 0,
            "Run tools bitwarden --yes from the original user's non-elevated desktop, not an administrator terminal");
        let shell = unsafe { GetShellWindow() };
        ensure!(!shell.is_null(), "No desktop shell: Bitwarden installation cannot run as a service or background account");
        let mut pid = 0;
        ensure!(
            unsafe { GetWindowThreadProcessId(shell, &mut pid) } != 0 && pid != 0,
            "Cannot identify desktop shell user"
        );
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return Err(std::io::Error::last_os_error()).context("Open desktop shell process");
        }
        let process = Handle(process);
        let shell_token = token(process.0)?;
        let current_user = token_user(&current)?;
        let shell_user = token_user(&shell_token)?;
        let same = unsafe {
            EqualSid(
                (*(current_user.as_ptr().cast::<TOKEN_USER>())).User.Sid,
                (*(shell_user.as_ptr().cast::<TOKEN_USER>())).User.Sid,
            )
        };
        ensure!(same != 0, "Current account differs from the desktop user; run tools bitwarden --yes as that user without elevation");
        Ok(())
    }

    fn known_folder(id: &GUID) -> Result<PathBuf> {
        let mut value = null_mut();
        let hr = unsafe { SHGetKnownFolderPath(id, 0, null_mut(), &mut value) };
        if hr < 0 {
            bail!("SHGetKnownFolderPath failed (HRESULT 0x{:08X})", hr as u32);
        }
        ensure!(
            !value.is_null(),
            "SHGetKnownFolderPath returned a null path"
        );
        let path = unsafe {
            let mut length = 0;
            while *value.add(length) != 0 {
                length += 1;
            }
            let result = PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(
                value, length,
            )));
            CoTaskMemFree(value.cast());
            result
        };
        ensure!(path.is_absolute(), "Known folder is not an absolute path");
        Ok(path)
    }

    pub(super) fn known_install() -> Result<bool> {
        for (id, relative) in [
            (&FOLDERID_LocalAppData, "Programs\\Bitwarden\\Bitwarden.exe"),
            (&FOLDERID_LocalAppData, "Bitwarden\\Bitwarden.exe"),
            (&FOLDERID_ProgramFiles, "Bitwarden\\Bitwarden.exe"),
            (&FOLDERID_ProgramFilesX86, "Bitwarden\\Bitwarden.exe"),
        ] {
            let path = known_folder(id)?.join(relative);
            match path.metadata() {
                Ok(metadata) if metadata.is_file() => return Ok(true),
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("Check existing Bitwarden: {}", path.display()))
                }
            }
        }
        Ok(false)
    }

    fn require_packaged_registration(name: *const u16) -> Result<()> {
        struct PackageInfo(*mut c_void);
        impl Drop for PackageInfo {
            fn drop(&mut self) {
                unsafe {
                    ClosePackageInfo(self.0);
                }
            }
        }
        let mut reference = null_mut();
        let status = unsafe { OpenPackageInfoByFullName(name, 0, &mut reference) };
        ensure!(
            status == 0 && !reference.is_null(),
            "OpenPackageInfoByFullName failed (Win32 {status})"
        );
        let reference = PackageInfo(reference);
        let (mut bytes, mut count) = (0, 0);
        const PACKAGE_FILTER_HEAD: u32 = 0x10;
        let status = unsafe {
            GetPackageInfo(
                reference.0,
                PACKAGE_FILTER_HEAD,
                &mut bytes,
                null_mut(),
                &mut count,
            )
        };
        ensure!(
            status == 122 && (8..=262144).contains(&bytes),
            "GetPackageInfo sizing failed (Win32 {status})"
        );
        let mut buffer = vec![0u64; (bytes as usize).div_ceil(size_of::<u64>())];
        let status = unsafe {
            GetPackageInfo(
                reference.0,
                PACKAGE_FILTER_HEAD,
                &mut bytes,
                buffer.as_mut_ptr().cast(),
                &mut count,
            )
        };
        ensure!(
            status == 0 && count == 1 && bytes >= 8,
            "GetPackageInfo failed (Win32 {status}, count {count})"
        );
        let flags = unsafe { *buffer.as_ptr().cast::<u32>().add(1) };
        ensure!(flags & 0x10000 == 0, "App Installer is a developer-mode registration; use the packaged Microsoft Store installation");
        Ok(())
    }

    pub(super) fn winget() -> Result<PathBuf> {
        let family: Vec<u16> = "Microsoft.DesktopAppInstaller_8wekyb3d8bbwe\0"
            .encode_utf16()
            .collect();
        let (mut count, mut length) = (0, 0);
        let status = unsafe {
            GetPackagesByPackageFamily(
                family.as_ptr(),
                &mut count,
                null_mut(),
                &mut length,
                null_mut(),
            )
        };
        ensure!(status == 122 && count > 0 && count <= 64 && length <= 262144,
            "Microsoft App Installer is not available for this user (GetPackagesByPackageFamily Win32 {status}, count {count}); install or repair App Installer through Microsoft Store");
        let mut names = vec![null_mut(); count as usize];
        let mut buffer = vec![0u16; length as usize];
        let status = unsafe {
            GetPackagesByPackageFamily(
                family.as_ptr(),
                &mut count,
                names.as_mut_ptr(),
                &mut length,
                buffer.as_mut_ptr(),
            )
        };
        ensure!(
            status == 0,
            "GetPackagesByPackageFamily failed (Win32 {status})"
        );
        let mut candidates = Vec::new();
        for name in names.into_iter().take(count as usize) {
            require_packaged_registration(name)?;
            let mut length = 0;
            let status = unsafe { GetPackagePathByFullName(name, &mut length, null_mut()) };
            ensure!(
                status == 122 && length > 1 && length <= 32768,
                "GetPackagePathByFullName sizing failed (Win32 {status})"
            );
            let mut path = vec![0u16; length as usize];
            let status = unsafe { GetPackagePathByFullName(name, &mut length, path.as_mut_ptr()) };
            ensure!(
                status == 0,
                "GetPackagePathByFullName failed (Win32 {status})"
            );
            let end = path
                .iter()
                .position(|c| *c == 0)
                .context("Unterminated package path")?;
            let root = PathBuf::from(OsString::from_wide(&path[..end]));
            match root.join("winget.exe").try_exists() {
                Ok(false) => continue, // Resource-only packages have no executable.
                Ok(true) => candidates.push(package_executable(&root)?),
                Err(error) => {
                    return Err(error).context("Check registered App Installer executable")
                }
            }
        }
        candidates.sort();
        candidates.dedup();
        ensure!(candidates.len() == 1, "Expected one registered App Installer executable, found {}; repair App Installer for this user", candidates.len());
        Ok(candidates.remove(0))
    }

    fn run(exe: &Path, operation: Operation, deadline: Instant) -> Result<(u32, Vec<u8>)> {
        ensure!(
            Instant::now() < deadline,
            "Bitwarden operation exceeded its ten-minute deadline"
        );
        let application: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
        ensure!(
            !application[..application.len() - 1]
                .iter()
                .any(|c| *c == 0 || *c == b'"' as u16),
            "Invalid packaged executable path"
        );
        let mut command = vec![b'"' as u16];
        command.extend_from_slice(&application[..application.len() - 1]);
        command.extend("\" ".encode_utf16());
        command.extend(operation.args().join(" ").encode_utf16());
        command.push(0);
        let cwd: Vec<u16> = exe
            .parent()
            .context("Missing package directory")?
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();

        let job = unsafe { CreateJobObjectW(null(), null()) };
        if job.is_null() {
            return Err(std::io::Error::last_os_error()).context("CreateJobObjectW");
        }
        let job = Handle(job);
        let mut limits: ExtendedLimits = unsafe { zeroed() };
        limits.basic.flags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE; no breakaway.
        check(
            unsafe {
                SetInformationJobObject(
                    job.0,
                    9,
                    (&limits as *const ExtendedLimits).cast(),
                    size_of::<ExtendedLimits>() as u32,
                )
            },
            "SetInformationJobObject",
        )?;

        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: 1,
        };
        let (mut read, mut write) = (null_mut(), null_mut());
        check(
            unsafe { CreatePipe(&mut read, &mut write, &attributes, 0) },
            "CreatePipe",
        )?;
        let mut reader = unsafe { File::from_raw_handle(read) };
        let writer = Handle(write);
        check(
            unsafe { SetHandleInformation(read, HANDLE_FLAG_INHERIT, 0) },
            "Protect pipe reader from inheritance",
        )?;
        let input = File::open("NUL").context("Open null input")?;
        check(
            unsafe {
                SetHandleInformation(
                    input.as_raw_handle(),
                    HANDLE_FLAG_INHERIT,
                    HANDLE_FLAG_INHERIT,
                )
            },
            "Set null input inheritance",
        )?;
        let mut startup: STARTUPINFOW = unsafe { zeroed() };
        startup.cb = size_of::<STARTUPINFOW>() as u32;
        startup.dwFlags = STARTF_USESTDHANDLES;
        startup.hStdInput = input.as_raw_handle();
        startup.hStdOutput = writer.0;
        startup.hStdError = writer.0;
        let mut info: PROCESS_INFORMATION = unsafe { zeroed() };
        check(
            unsafe {
                CreateProcessW(
                    application.as_ptr(),
                    command.as_mut_ptr(),
                    null(),
                    null(),
                    1,
                    CREATE_SUSPENDED | CREATE_NO_WINDOW,
                    null(),
                    cwd.as_ptr(),
                    &startup,
                    &mut info,
                )
            },
            "Start packaged WinGet",
        )?;
        let process = Handle(info.hProcess);
        let thread = Handle(info.hThread);
        if let Err(error) = check(
            unsafe { AssignProcessToJobObject(job.0, process.0) },
            "Assign WinGet to timeout job",
        ) {
            unsafe {
                TerminateProcess(process.0, 1);
            }
            return Err(error);
        }
        ensure!(
            unsafe { ResumeThread(thread.0) } != u32::MAX,
            "Resume WinGet failed: {}",
            std::io::Error::last_os_error()
        );
        drop(writer);
        drop(input);
        let mut output = Vec::new();
        loop {
            ensure!(Instant::now() < deadline, "WinGet {operation:?} exceeded the ten-minute deadline; its job was terminated; check installation state before retrying");
            let mut available = 0;
            if unsafe { PeekNamedPipe(read, null_mut(), 0, null_mut(), &mut available, null_mut()) }
                == 0
            {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(109) {
                    return Err(error).context("Read WinGet output pipe");
                }
            }
            if available > 0 {
                let mut chunk = [0u8; 8192];
                let take = chunk.len().min(available as usize);
                let count = reader
                    .read(&mut chunk[..take])
                    .context("Read WinGet output")?;
                let keep = count.min(65536usize.saturating_sub(output.len()));
                output.extend_from_slice(&chunk[..keep]);
                continue;
            }
            match unsafe { WaitForSingleObject(process.0, 50) } {
                WAIT_TIMEOUT => {}
                WAIT_OBJECT_0 => {
                    let mut code = 0;
                    check(
                        unsafe { GetExitCodeProcess(process.0, &mut code) },
                        "GetExitCodeProcess(WinGet)",
                    )?;
                    let mut remaining = 0;
                    unsafe {
                        PeekNamedPipe(read, null_mut(), 0, null_mut(), &mut remaining, null_mut());
                    }
                    if remaining != 0 {
                        continue;
                    }
                    return Ok((code, output));
                }
                _ => {
                    return Err(std::io::Error::last_os_error())
                        .context("WaitForSingleObject(WinGet)")
                }
            }
        }
    }

    pub(super) fn install_check() -> Result<()> {
        require_desktop_user().context(super::NotHere)?;
        winget().context(super::NotHere)?;
        Ok(())
    }

    pub(super) fn install() -> Result<()> {
        require_desktop_user().context(super::NotHere)?;
        if known_install()? {
            return Ok(());
        }
        let exe = winget().context(super::NotHere)?;
        let deadline = Instant::now() + Duration::from_secs(600);
        let (code, source) = run(&exe, Operation::Source, deadline)?;
        if super::is_offline_code(code) {
            return Err(super::Offline.into());
        }
        ensure!(code == 0, "WinGet source export failed (exit 0x{code:08X})");
        verify_source(&source)?;
        let (code, _) = run(&exe, Operation::List, deadline)?;
        if super::is_offline_code(code) {
            return Err(super::Offline.into());
        }
        if list_found(code).context("Determine existing installation; no install was attempted")? {
            return Ok(());
        }
        // WinGet verifies the installer against the repository manifest SHA-256.
        // Hash/security errors propagate; there is no bypass or download fallback.
        let (code, _) = run(&exe, Operation::Install, deadline)?;
        if super::is_offline_code(code) || (code != 0 && super::dns_offline()) {
            return Err(super::Offline.into());
        }
        ensure!(code == 0, "WinGet Bitwarden installation failed (exit 0x{code:08X}); installer hash verification was not bypassed");
        let (code, _) = run(&exe, Operation::List, deadline)?;
        ensure!(
            list_found(code)?,
            "WinGet reported success but Bitwarden desktop was not found afterward"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_is_recognized_only_from_connectivity_failures() {
        assert!(is_offline_code(0x8007_2EE7));
        assert!(is_offline_code(0x8007_2EFD));
        for code in [0, 1, 0x8A15_0014, 0x8A15_0008, 0x8007_0005, 0xFFFF_FFFF] {
            assert!(!is_offline_code(code), "{code:#x}");
        }
        let error = anyhow::Error::new(Offline).context("Install Bitwarden");
        assert!(is_offline_error(&error));
        assert!(!is_offline_error(&anyhow::anyhow!("hash mismatch")));
        assert!(!is_not_here_error(&error));
    }

    #[test]
    fn account_refusals_are_told_apart_from_other_failures() {
        use anyhow::Context;
        let refused = Err::<(), _>(anyhow::anyhow!("not the desktop user")).context(NotHere);
        let refused = refused.unwrap_err();
        assert!(is_not_here_error(&refused));
        assert!(!is_offline_error(&refused));
        assert!(!is_not_here_error(&anyhow::anyhow!("hash mismatch")));
    }

    #[test]
    fn absence_is_only_the_documented_hresult() {
        assert!(list_found(0).unwrap());
        assert!(!list_found(0x8A15_0014).unwrap());
        for code in [
            1,
            0x8A15_000F,
            0x8A15_0011,
            0x8A15_0012,
            0x8A15_0016,
            0x8A15_0045,
            0xFFFF_FFFF,
        ] {
            assert!(list_found(code).is_err());
        }
    }

    #[test]
    fn fixed_arguments_preserve_security_and_scope() {
        for operation in [Operation::Source, Operation::List, Operation::Install] {
            assert!(operation.args().iter().all(|arg| arg
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))));
            assert!(!operation.args().iter().any(|arg| [
                "--ignore-security-hash",
                "--override",
                "--custom",
                "--force",
                "--allow-reboot"
            ]
            .contains(arg)));
        }
        for operation in [Operation::List, Operation::Install] {
            let args = operation.args();
            assert!(args
                .windows(2)
                .any(|p| p == ["--id", "Bitwarden.Bitwarden"]));
            assert!(args.windows(2).any(|p| p == ["--source", "winget"]));
            assert!(args.contains(&"--exact"));
            assert!(args.contains(&"--disable-interactivity"));
        }
        assert!(Operation::Install
            .args()
            .windows(2)
            .any(|p| p == ["--scope", "user"]));
        assert!(!Operation::List.args().contains(&"--scope")); // all installation scopes
    }

    #[test]
    fn a_renamed_repository_is_not_trusted() {
        let source = serde_json::json!({
            "Name": "winget", "Arg": "https://cdn.winget.microsoft.com/cache",
            "Type": "Microsoft.PreIndexed.Package", "Identifier": "Microsoft.Winget.Source_8wekyb3d8bbwe"
        });
        assert!(verify_source(&serde_json::to_vec(&source).unwrap()).is_ok());
        for key in ["Name", "Arg", "Type", "Identifier"] {
            let mut changed = source.clone();
            changed[key] = "untrusted".into();
            assert!(verify_source(&serde_json::to_vec(&changed).unwrap()).is_err());
        }
        assert!(verify_source(b"not JSON").is_err());
    }

    #[test]
    fn package_path_must_be_absolute_and_contain_an_executable() {
        assert!(package_executable(std::path::Path::new("relative")).is_err());
        let directory = tempfile::tempdir().unwrap();
        assert!(package_executable(directory.path()).is_err());
        std::fs::write(directory.path().join("winget.exe"), b"test fixture").unwrap();
        assert_eq!(
            package_executable(directory.path()).unwrap(),
            directory.path().canonicalize().unwrap().join("winget.exe")
        );
    }

    #[cfg(unix)]
    #[test]
    fn package_executable_cannot_redirect_outside_the_package() {
        let package = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), package.path().join("winget.exe")).unwrap();
        assert!(package_executable(package.path()).is_err());
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_is_an_error() {
        assert!(install_bitwarden().is_err());
    }
}
