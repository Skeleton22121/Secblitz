//! Native launcher: trusted paths, a clean environment, suspended job assignment,
//! one process per probe, bounded pipes, deadline, memory and no child processes.
use super::*;
use base64::Engine;
use std::{
    ffi::{c_void, OsStr, OsString},
    fs::File,
    io::{Read, Write},
    mem::{size_of, zeroed},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::{mpsc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Security::{
        EqualSid, GetTokenInformation, TokenElevation, TokenUser, SECURITY_ATTRIBUTES,
        TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER,
    },
    System::{
        SystemInformation::GetWindowsDirectoryW,
        Threading::{
            CreateProcessW, GetCurrentProcess, GetExitCodeProcess, OpenProcess, OpenProcessToken,
            ResumeThread, TerminateProcess, WaitForSingleObject, CREATE_NO_WINDOW,
            CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, IO_COUNTERS, PROCESS_INFORMATION,
            PROCESS_QUERY_LIMITED_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOW,
        },
    },
    UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId},
};

#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(attributes: *const SECURITY_ATTRIBUTES, name: *const u16) -> HANDLE;
    fn SetInformationJobObject(job: HANDLE, class: i32, info: *const c_void, length: u32) -> i32;
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
    fn GlobalFree(memory: *mut c_void) -> *mut c_void;
    fn InitializeProcThreadAttributeList(
        list: *mut c_void,
        count: u32,
        flags: u32,
        size: *mut usize,
    ) -> i32;
    fn UpdateProcThreadAttribute(
        list: *mut c_void,
        flags: u32,
        attribute: usize,
        value: *mut c_void,
        size: usize,
        previous: *mut c_void,
        returned: *mut usize,
    ) -> i32;
    fn DeleteProcThreadAttributeList(list: *mut c_void);
}
#[repr(C)]
struct WinHttpProxyInfo {
    access_type: u32,
    proxy: *mut u16,
    bypass: *mut u16,
}
#[link(name = "winhttp")]
extern "system" {
    fn WinHttpGetDefaultProxyConfiguration(info: *mut WinHttpProxyInfo) -> i32;
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
#[repr(C)]
struct StartupInfoEx {
    startup: STARTUPINFOW,
    attributes: *mut c_void,
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct Attributes(Vec<usize>);
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.0.as_mut_ptr().cast());
        }
    }
}

type ProbeResult<T> = Result<T, UnknownReason>;
fn check(ok: i32) -> ProbeResult<()> {
    if ok == 0 {
        Err(UnknownReason::Unavailable)
    } else {
        Ok(())
    }
}
fn wide(value: impl AsRef<OsStr>) -> ProbeResult<Vec<u16>> {
    let value: Vec<u16> = value.as_ref().encode_wide().collect();
    if value.iter().any(|v| *v == 0 || *v == b'"' as u16) {
        return Err(UnknownReason::InvalidData);
    }
    Ok(value.into_iter().chain(Some(0)).collect())
}
fn windows_directory() -> ProbeResult<PathBuf> {
    let mut buffer = vec![0; 32768];
    let count = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if count == 0 || count >= buffer.len() {
        return Err(UnknownReason::Unavailable);
    }
    let path = PathBuf::from(OsString::from_wide(&buffer[..count]));
    if !path.is_absolute() {
        return Err(UnknownReason::InvalidData);
    }
    Ok(path)
}
fn token(process: HANDLE) -> ProbeResult<Handle> {
    let mut handle = null_mut();
    check(unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut handle) })?;
    Ok(Handle(handle))
}
fn token_user(token: &Handle) -> ProbeResult<Vec<usize>> {
    let mut bytes = 0;
    unsafe {
        GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut bytes);
    }
    if !(size_of::<TOKEN_USER>() as u32..=65536).contains(&bytes) {
        return Err(UnknownReason::InvalidData);
    }
    let mut buffer = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
    check(unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            bytes,
            &mut bytes,
        )
    })?;
    Ok(buffer)
}
fn require_original_user() -> ProbeResult<()> {
    let current = token(unsafe { GetCurrentProcess() })?;
    let mut elevation: TOKEN_ELEVATION = unsafe { zeroed() };
    let mut bytes = 0;
    check(unsafe {
        GetTokenInformation(
            current.0,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut bytes,
        )
    })?;
    if bytes as usize != size_of::<TOKEN_ELEVATION>() || elevation.TokenIsElevated != 0 {
        return Err(UnknownReason::OriginalUserNotVerified);
    }
    let shell = unsafe { GetShellWindow() };
    if shell.is_null() {
        return Err(UnknownReason::OriginalUserNotVerified);
    }
    let mut pid = 0;
    if unsafe { GetWindowThreadProcessId(shell, &mut pid) } == 0 || pid == 0 {
        return Err(UnknownReason::OriginalUserNotVerified);
    }
    let shell_process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if shell_process.is_null() {
        return Err(UnknownReason::OriginalUserNotVerified);
    }
    let shell_process = Handle(shell_process);
    let desktop = token(shell_process.0)?;
    let user = token_user(&current)?;
    let shell_user = token_user(&desktop)?;
    if unsafe {
        EqualSid(
            (*(user.as_ptr().cast::<TOKEN_USER>())).User.Sid,
            (*(shell_user.as_ptr().cast::<TOKEN_USER>())).User.Sid,
        )
    } == 0
    {
        return Err(UnknownReason::OriginalUserNotVerified);
    }
    Ok(())
}

fn pipe() -> ProbeResult<(File, File)> {
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let (mut read, mut write) = (null_mut(), null_mut());
    check(unsafe { CreatePipe(&mut read, &mut write, &attributes, 0) })?;
    Ok(unsafe { (File::from_raw_handle(read), File::from_raw_handle(write)) })
}

fn environment(root: &Path) -> ProbeResult<Vec<u16>> {
    // No inherited PATH, PSModulePath, HOME, APPDATA, proxy, credentials, TEMP,
    // COMPLUS or CLR profiler variables. Known folders use the actual user token.
    let variables = [
        ("PATH", root.join("System32").into_os_string()),
        ("PSModuleAnalysisCachePath", OsString::from("NUL")),
        (
            "PSModulePath",
            root.join("System32/WindowsPowerShell/v1.0/Modules")
                .into_os_string(),
        ),
        ("SystemRoot", root.as_os_str().to_owned()),
        ("WINDIR", root.as_os_str().to_owned()),
    ];
    let mut output = Vec::new();
    for (name, value) in variables {
        output.extend(format!("{name}=").encode_utf16());
        output.extend(wide(value)?);
    }
    output.push(0);
    Ok(output)
}

/// Probes that run one fixed Windows tool directly instead of PowerShell.
/// Executable and arguments are compiled constants.
fn native_tool(id: ProbeId) -> Option<(&'static str, &'static str)> {
    match id {
        ProbeId::WinRe => Some(("System32/reagentc.exe", "/info")),
        ProbeId::WindowsHello => Some(("System32/dsregcmd.exe", "/status")),
        _ => None,
    }
}

/// Neither the executable nor the arguments can be supplied by report data.
fn run(root: &Path, id: ProbeId, timeout: Duration) -> ProbeResult<Vec<u8>> {
    let deadline = Instant::now() + timeout;
    let (exe, arguments, input) = if let Some((tool, arguments)) = native_tool(id) {
        (root.join(tool), arguments.to_owned(), String::new())
    } else {
        let bootstrap = "$global:ProgressPreference='SilentlyContinue';[Console]::InputEncoding=[Text.UTF8Encoding]::new($false);[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);& ([ScriptBlock]::Create([Console]::In.ReadToEnd()))";
        let encoded = base64::engine::general_purpose::STANDARD.encode(
            bootstrap
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let script = format!(
            "$probe='{id:?}'\n{}\n{}\n{}",
            include_str!("common.ps1"),
            if id == ProbeId::BrowserExtensions {
                include_str!("browsers.ps1")
            } else {
                ""
            },
            include_str!("probes.ps1")
        );
        (
            root.join("System32/WindowsPowerShell/v1.0/powershell.exe"),
            format!("-NoLogo -NoProfile -NonInteractive -EncodedCommand {encoded}"),
            script,
        )
    };
    let application = wide(&exe)?;
    // Fixed paths are not sufficient: reject writable/reparse executables and
    // retain non-delete/non-write-sharing pins through process/job teardown.
    let mut pins =
        crate::operations::pin_system_executable(&exe).map_err(|_| UnknownReason::Unavailable)?;
    if native_tool(id).is_none() {
        for module in modules(id) {
            let relative = if module == "Microsoft.PowerShell.LocalAccounts" {
                "Microsoft.PowerShell.LocalAccounts/1.0.0.0/Microsoft.PowerShell.LocalAccounts.psd1"
                    .to_owned()
            } else {
                format!("{module}/{module}.psd1")
            };
            pins.extend(
                crate::operations::pin_system_module(&root.join(format!(
                    "System32/WindowsPowerShell/v1.0/Modules/{relative}"
                )))
                .map_err(|_| UnknownReason::Unavailable)?,
            );
        }
        if id == ProbeId::Management {
            pins.extend(
                crate::operations::pin_system_executable(
                    &root.join("System32/MDMRegistration.dll"),
                )
                .map_err(|_| UnknownReason::Unavailable)?,
            );
        }
    }
    let mut command: Vec<u16> = std::iter::once(b'"' as u16)
        .chain(application[..application.len() - 1].iter().copied())
        .chain(format!("\" {arguments}\0").encode_utf16())
        .collect();
    let cwd = wide(root.join("System32"))?;
    let environment = environment(root)?;
    let job = unsafe { CreateJobObjectW(null(), null()) };
    if job.is_null() {
        return Err(UnknownReason::Unavailable);
    }
    let job = Handle(job);
    let mut limits: ExtendedLimits = unsafe { zeroed() };
    limits.basic.flags = 0x2000 | 0x8 | 0x100; // KILL_ON_JOB_CLOSE | ACTIVE_PROCESS | PROCESS_MEMORY
    limits.basic.active_processes = 1;
    limits.process_memory = 512 * 1024 * 1024;
    check(unsafe {
        SetInformationJobObject(
            job.0,
            9,
            (&limits as *const ExtendedLimits).cast(),
            size_of::<ExtendedLimits>() as u32,
        )
    })?;
    let (child_input, mut writer) = pipe()?;
    let (mut reader, child_output) = pipe()?;
    check(unsafe { SetHandleInformation(writer.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) })?;
    check(unsafe { SetHandleInformation(reader.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) })?;
    // Restrict inheritance to these two pipe handles, including under concurrency.
    let mut attribute_size = 0;
    unsafe {
        InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut attribute_size);
    }
    if attribute_size == 0 || attribute_size > 65536 {
        return Err(UnknownReason::Unavailable);
    }
    let mut attribute_buffer = vec![0usize; attribute_size.div_ceil(size_of::<usize>())];
    check(unsafe {
        InitializeProcThreadAttributeList(
            attribute_buffer.as_mut_ptr().cast(),
            1,
            0,
            &mut attribute_size,
        )
    })?;
    let mut attributes = Attributes(attribute_buffer);
    let mut inherited = [child_input.as_raw_handle(), child_output.as_raw_handle()];
    check(unsafe {
        UpdateProcThreadAttribute(
            attributes.0.as_mut_ptr().cast(),
            0,
            0x00020002,
            inherited.as_mut_ptr().cast(),
            size_of_val(&inherited),
            null_mut(),
            null_mut(),
        )
    })?;
    let mut startup: StartupInfoEx = unsafe { zeroed() };
    startup.startup.cb = size_of::<StartupInfoEx>() as u32;
    startup.startup.dwFlags = STARTF_USESTDHANDLES;
    startup.startup.hStdInput = child_input.as_raw_handle();
    startup.startup.hStdOutput = child_output.as_raw_handle();
    startup.startup.hStdError = child_output.as_raw_handle();
    startup.attributes = attributes.0.as_mut_ptr().cast();
    let mut info: PROCESS_INFORMATION = unsafe { zeroed() };
    if Instant::now() >= deadline {
        return Err(UnknownReason::Timeout);
    }
    check(unsafe {
        CreateProcessW(
            application.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_NO_WINDOW | CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | 0x00080000,
            environment.as_ptr().cast(),
            cwd.as_ptr(),
            &startup.startup,
            &mut info,
        )
    })?;
    let process = Handle(info.hProcess);
    let thread = Handle(info.hThread);
    if unsafe { AssignProcessToJobObject(job.0, process.0) } == 0 {
        unsafe {
            TerminateProcess(process.0, 1);
            WaitForSingleObject(process.0, 1000);
        }
        return Err(UnknownReason::Unavailable);
    }
    if Instant::now() >= deadline {
        return Err(UnknownReason::Timeout);
    }
    if unsafe { ResumeThread(thread.0) } == u32::MAX {
        return Err(UnknownReason::ProcessFailed);
    }
    drop(child_input);
    drop(child_output);
    let input_thread = std::thread::Builder::new()
        .name("diagnostics-input".into())
        .spawn(move || writer.write_all(input.as_bytes()))
        .map_err(|_| UnknownReason::Unavailable)?;
    let result = (|| {
        let mut output = Vec::new();
        loop {
            if Instant::now() >= deadline {
                return Err(UnknownReason::Timeout);
            }
            let mut available = 0;
            if unsafe {
                PeekNamedPipe(
                    reader.as_raw_handle(),
                    null_mut(),
                    0,
                    null_mut(),
                    &mut available,
                    null_mut(),
                )
            } == 0
                && std::io::Error::last_os_error().raw_os_error() != Some(109)
            {
                return Err(UnknownReason::ProcessFailed);
            }
            if available != 0 {
                let mut chunk = [0; 8192];
                let take = chunk.len().min(available as usize);
                let count = reader
                    .read(&mut chunk[..take])
                    .map_err(|_| UnknownReason::ProcessFailed)?;
                if output.len() + count > MAX_OUTPUT_BYTES {
                    return Err(UnknownReason::OutputLimit);
                }
                output.extend_from_slice(&chunk[..count]);
                continue;
            }
            match unsafe { WaitForSingleObject(process.0, 20) } {
                WAIT_TIMEOUT => {}
                WAIT_OBJECT_0 => {
                    // Drain final bytes written immediately before exit.
                    let mut remaining = 0;
                    unsafe {
                        PeekNamedPipe(
                            reader.as_raw_handle(),
                            null_mut(),
                            0,
                            null_mut(),
                            &mut remaining,
                            null_mut(),
                        );
                    }
                    if remaining > 0 {
                        continue;
                    }
                    let mut code = 0;
                    check(unsafe { GetExitCodeProcess(process.0, &mut code) })?;
                    if code != 0 {
                        return Err(UnknownReason::ProcessFailed);
                    }
                    return Ok(output);
                }
                _ => return Err(UnknownReason::ProcessFailed),
            }
        }
    })();
    // All paths kill the job before joining a potentially blocked pipe writer.
    drop(job);
    let input_result = input_thread
        .join()
        .map_err(|_| UnknownReason::ProcessFailed)
        .and_then(|r| r.map_err(|_| UnknownReason::ProcessFailed));
    result.and_then(|output| input_result.map(|_| output))
}

fn modules(id: ProbeId) -> Vec<&'static str> {
    let mut names = vec![
        "Microsoft.PowerShell.Utility",
        "Microsoft.PowerShell.Management",
    ];
    names.extend_from_slice(match id {
        ProbeId::DefenderHealth | ProbeId::DefenderPolicy | ProbeId::DefenderProtection => {
            &["Defender"]
        }
        ProbeId::SecureBootCerts => &["SecureBoot", "Microsoft.PowerShell.Diagnostics"],
        ProbeId::UpdatePolicy | ProbeId::Persistence => &["CimCmdlets"],
        ProbeId::LegacyFeatures => &["Dism"],
        ProbeId::AccountHygiene => &["Microsoft.PowerShell.LocalAccounts"],
        ProbeId::AccountSetup => &["Microsoft.PowerShell.LocalAccounts", "CimCmdlets"],
        ProbeId::DnsEncryption => &["DnsClient"],
        ProbeId::Autostart => &[
            "CimCmdlets",
            "ScheduledTasks",
            "Microsoft.PowerShell.Security",
        ],
        ProbeId::Sharing => &["SmbShare"],
        ProbeId::FirewallRules => &["NetSecurity"],
        ProbeId::SecurityProviders
        | ProbeId::Management
        | ProbeId::BitLocker
        | ProbeId::Vbs
        | ProbeId::Ntfs => &["CimCmdlets"],
        ProbeId::SecureBoot => &["SecureBoot"],
        ProbeId::Tpm => &["TrustedPlatformModule"],
        ProbeId::Accounts => &["Microsoft.PowerShell.LocalAccounts"],
        ProbeId::RemoteAccess => &["SmbShare", "NetTCPIP"],
        ProbeId::Storage => &["Storage"],
        ProbeId::Backup => &["CimCmdlets", "Microsoft.PowerShell.Diagnostics"],
        ProbeId::Adapters => &["NetAdapter"],
        ProbeId::Dns => &["DnsClient"],
        ProbeId::Vpn => &["VpnClient"],
        _ => &[],
    });
    names
}

fn proxy() -> Evidence {
    let mut info: WinHttpProxyInfo = unsafe { zeroed() };
    let ok = unsafe { WinHttpGetDefaultProxyConfiguration(&mut info) };
    let default_mode = if ok == 0 {
        Reading::Unknown(UnknownReason::Unavailable)
    } else {
        match info.access_type {
            1 => Reading::Known(ProxyMode::Direct),
            3 => Reading::Known(ProxyMode::NamedProxy),
            4 => Reading::Known(ProxyMode::Automatic),
            _ => Reading::Unknown(UnknownReason::InvalidData),
        }
    };
    // Strings are deliberately neither dereferenced nor serialized.
    unsafe {
        if !info.proxy.is_null() {
            GlobalFree(info.proxy.cast());
        }
        if !info.bypass.is_null() {
            GlobalFree(info.bypass.cast());
        }
    }
    Evidence::Proxy(Proxy { default_mode })
}

type AuditReceiver = mpsc::Receiver<ProbeResult<Evidence>>;
static AUDIT: OnceLock<Mutex<Option<AuditReceiver>>> = OnceLock::new();
static PROXY: OnceLock<Mutex<Option<AuditReceiver>>> = OnceLock::new();
static WIFI: OnceLock<Mutex<Option<AuditReceiver>>> = OnceLock::new();
fn native_bounded(
    slot: &'static OnceLock<Mutex<Option<AuditReceiver>>>,
    timeout: Duration,
    query: impl FnOnce() -> ProbeResult<Evidence> + Send + 'static,
) -> ProbeResult<Evidence> {
    let mut slot = slot
        .get_or_init(|| Mutex::new(None))
        .try_lock()
        .map_err(|_| UnknownReason::Busy)?;
    if let Some(receiver) = slot.as_ref() {
        if matches!(receiver.try_recv(), Err(mpsc::TryRecvError::Empty)) {
            return Err(UnknownReason::Busy);
        }
        *slot = None; // stale results are never attributed to this snapshot
    }
    let (send, receive) = mpsc::channel();
    std::thread::Builder::new()
        .name("diagnostics-native".into())
        .spawn(move || {
            let _ = send.send(query());
        })
        .map_err(|_| UnknownReason::Unavailable)?;
    *slot = Some(receive);
    match slot.as_ref().unwrap().recv_timeout(timeout) {
        Ok(result) => {
            *slot = None;
            result
        }
        Err(mpsc::RecvTimeoutError::Timeout) => Err(UnknownReason::Timeout),
        Err(_) => {
            *slot = None;
            Err(UnknownReason::ProcessFailed)
        }
    }
}
type WlanOpen = unsafe extern "system" fn(u32, *const c_void, *mut u32, *mut HANDLE) -> u32;
type WlanClose = unsafe extern "system" fn(HANDLE, *const c_void) -> u32;
type WlanEnum = unsafe extern "system" fn(HANDLE, *const c_void, *mut *mut u8) -> u32;
type WlanQuery = unsafe extern "system" fn(
    HANDLE,
    *const u8,
    u32,
    *const c_void,
    *mut u32,
    *mut *mut u8,
    *mut u32,
) -> u32;
type WlanFree = unsafe extern "system" fn(*mut c_void);

/// Security type of the connected Wi-Fi network (read-only). Only the two
/// algorithm numbers are read: the network name, address and profile are never
/// touched. No WLAN service means there is no Wi-Fi to assess.
fn wifi() -> ProbeResult<Evidence> {
    use windows_sys::Win32::{
        Foundation::FreeLibrary,
        System::LibraryLoader::{GetProcAddress, LoadLibraryExW},
    };
    const SEARCH_SYSTEM32: u32 = 0x800;
    const SERVICE_NOT_ACTIVE: u32 = 1062;
    let evidence = |class: &str| {
        Evidence::WifiSecurity(WifiSecurity {
            current_network: Reading::Known(class.into()),
        })
    };
    let name: Vec<u16> = "wlanapi.dll\0".encode_utf16().collect();
    let module = unsafe { LoadLibraryExW(name.as_ptr(), null_mut(), SEARCH_SYSTEM32) };
    if module.is_null() {
        return Ok(evidence("None"));
    }
    macro_rules! symbol {
        ($name:literal, $ty:ty) => {
            match unsafe { GetProcAddress(module, concat!($name, "\0").as_ptr()) } {
                Some(f) => unsafe {
                    std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(f)
                },
                None => {
                    unsafe { FreeLibrary(module) };
                    return Err(UnknownReason::Unavailable);
                }
            }
        };
    }
    let open = symbol!("WlanOpenHandle", WlanOpen);
    let close = symbol!("WlanCloseHandle", WlanClose);
    let enumerate = symbol!("WlanEnumInterfaces", WlanEnum);
    let query = symbol!("WlanQueryInterface", WlanQuery);
    let free = symbol!("WlanFreeMemory", WlanFree);
    let read =
        |base: *const u8, offset: usize| unsafe { base.add(offset).cast::<u32>().read_unaligned() };
    let result = (|| {
        let (mut version, mut client): (u32, HANDLE) = (0, null_mut());
        match unsafe { open(2, null(), &mut version, &mut client) } {
            0 => {}
            SERVICE_NOT_ACTIVE => return Ok("None"),
            _ => return Err(UnknownReason::Unavailable),
        }
        let result = (|| {
            let mut list: *mut u8 = null_mut();
            if unsafe { enumerate(client, null(), &mut list) } != 0 || list.is_null() {
                return Err(UnknownReason::Unavailable);
            }
            // WLAN_INTERFACE_INFO_LIST: count, index, then 532-byte entries
            // (GUID, 256 UTF-16 description units, state).
            let count = read(list, 0) as usize;
            let mut best: Option<&'static str> = None;
            let mut outcome = Ok(());
            if count > 64 {
                outcome = Err(UnknownReason::OutputLimit);
            }
            for i in 0..count.min(64) {
                let entry = unsafe { list.add(8 + i * 532) };
                if read(entry, 528) != 1 {
                    continue; // not connected
                }
                let (mut size, mut data, mut kind): (u32, *mut u8, u32) = (0, null_mut(), 0);
                // opcode 7: wlan_intf_opcode_current_connection
                let status =
                    unsafe { query(client, entry, 7, null(), &mut size, &mut data, &mut kind) };
                if status != 0 || data.is_null() {
                    outcome = Err(UnknownReason::Unavailable);
                    continue;
                }
                // WLAN_CONNECTION_ATTRIBUTES: security attributes start at byte 588.
                if size >= 604 {
                    let class =
                        parse::wifi_class(read(data, 588) != 0, read(data, 596), read(data, 600));
                    if best.is_none_or(|b| parse::wifi_rank(class) < parse::wifi_rank(b)) {
                        best = Some(class);
                    }
                } else {
                    outcome = Err(UnknownReason::InvalidData);
                }
                unsafe { free(data.cast()) };
            }
            unsafe { free(list.cast()) };
            outcome?;
            Ok(best.unwrap_or("None"))
        })();
        unsafe { close(client, null()) };
        result
    })();
    unsafe { FreeLibrary(module) };
    result.map(evidence)
}

fn permissions() -> ProbeResult<Evidence> {
    let findings = crate::permissions::audit().map_err(|_| UnknownReason::Unavailable)?;
    if findings.len() > 32 {
        return Err(UnknownReason::OutputLimit);
    }
    let mut items = Vec::new();
    for finding in findings {
        let service = finding
            .title
            .strip_prefix("Service permissions: ")
            .ok_or(UnknownReason::InvalidData)?;
        if service.is_empty()
            || service.len() > 64
            || !service.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return Err(UnknownReason::InvalidData);
        }
        let status = match finding.status.as_str() {
            "review" => Status::Attention,
            "info" => Status::Informational,
            _ => Status::Unknown,
        };
        items.push(PermissionFinding {
            service: service.into(),
            status,
        });
    }
    Ok(Evidence::Permissions(Permissions {
        services: Reading::Known(Inventory {
            items,
            truncated: false,
        }),
    }))
}

pub(super) fn collect(context: &Context) -> Vec<Diagnostic> {
    // Serialize collection to bound process/thread resources across callers.
    static COLLECTION: Mutex<()> = Mutex::new(());
    let Ok(_guard) = COLLECTION.try_lock() else {
        return ProbeId::ALL
            .iter()
            .map(|&id| unavailable(id, UnknownReason::Busy))
            .collect();
    };
    if !cfg!(target_arch = "x86_64") {
        return ProbeId::ALL
            .iter()
            .map(|&id| unavailable(id, UnknownReason::PlatformUnsupported))
            .collect();
    }
    let deadline = Instant::now() + Duration::from_secs(COLLECTION_TIMEOUT_SECONDS);
    let root = windows_directory();
    let mut output = Vec::new();
    for &id in ProbeId::ALL {
        if id == ProbeId::BrowserExtensions {
            let reason = if context.original_user == OriginalUserScope::Omit {
                Some(UnknownReason::NotRequested)
            } else if require_original_user().is_err() {
                Some(UnknownReason::OriginalUserNotVerified)
            } else {
                None
            };
            if let Some(reason) = reason {
                output.push(unavailable(id, reason));
                continue;
            }
        }
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            output.push(unavailable(id, UnknownReason::CollectionDeadline));
            continue;
        };
        let timeout = remaining.min(Duration::from_secs(PROBE_TIMEOUT_SECONDS));
        let result = match id {
            ProbeId::Proxy => {
                native_bounded(&PROXY, timeout.min(Duration::from_secs(2)), || Ok(proxy()))
            }
            ProbeId::Permissions => {
                native_bounded(&AUDIT, timeout.min(Duration::from_secs(2)), permissions)
            }
            ProbeId::WifiSecurity => {
                native_bounded(&WIFI, timeout.min(Duration::from_secs(3)), wifi)
            }
            _ => match &root {
                Ok(root) => run(root, id, timeout).and_then(|bytes| match id {
                    ProbeId::WinRe => Ok(Evidence::WinRe(parse::winre(&bytes))),
                    ProbeId::WindowsHello => Ok(Evidence::WindowsHello(parse::dsreg(&bytes))),
                    _ => parse::decode(id, &bytes),
                }),
                Err(reason) => Err(*reason),
            },
        };
        output.push(match result {
            Ok(evidence) => Diagnostic {
                id,
                scope: id.scope(),
                observed_at_unix_seconds: now(),
                source: id.source().into(),
                status: Status::Unknown,
                evidence: Some(evidence),
                failure: None,
                assessments: vec![],
            },
            Err(reason) => unavailable(id, reason),
        });
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "native elevated Windows original-user rejection; no collection or mutation"]
    fn elevated_process_cannot_substitute_its_browser_profile() {
        assert!(
            crate::platform::is_elevated().unwrap(),
            "Run this identity-only test elevated"
        );
        assert_eq!(
            require_original_user(),
            Err(UnknownReason::OriginalUserNotVerified)
        );
    }
}
