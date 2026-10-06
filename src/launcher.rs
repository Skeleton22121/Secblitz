//! Unelevated entry point: serves the broker pipe and launches the elevated GUI.
use crate::i18n::Lang;

pub fn message_box(title: &str, text: &str) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND,
        };
        let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND,
            );
        }
    }
    #[cfg(not(windows))]
    eprintln!("{title}: {text}");
}

pub fn friendly_problem(raw: &str) -> &'static str {
    let r = raw.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| r.contains(n));
    if has(&["requires windows", "only supported on windows", "windows 10/11"]) {
        "Secblitz works on Windows 10 and Windows 11 (64-bit) only. Open it on a PC that runs one of them."
    } else if has(&["declined", "cancel"]) {
        "Secblitz needs your permission to open. Open it again and choose Yes when Windows asks."
    } else if has(&["access is denied", "os error 5", "permission denied", "administrator", "elevat"]) {
        "Windows wouldn't let Secblitz open its files. Sign in with an account that can make changes to this PC, then open Secblitz again."
    } else if has(&["journal lock", "another secblitz", "already running"]) {
        "Secblitz is already busy with another task. Wait a minute, then open it again."
    } else if has(&["no space", "os error 112", "disk full"]) {
        "Your PC is almost out of space. Free up some space, then open Secblitz again."
    } else if has(&["journal", "corrupt", "missing header", "utf8"]) {
        "Secblitz couldn't read its saved information. Restart your PC and open Secblitz again. If it keeps happening, install the latest Secblitz."
    } else if has(&["timed out", "did not answer", "broker", "powershell", "script", "backend"]) {
        "Windows didn't answer in time. Restart your PC, then open Secblitz again."
    } else {
        "Something unexpected got in the way. Restart your PC and open Secblitz again. If it keeps happening, check for a Secblitz update."
    }
}

pub fn friendly_check_problem(raw: &str) -> &'static str {
    let found = friendly_problem(raw);
    if found.starts_with("Secblitz works on Windows 10") {
        found
    } else {
        "Something unexpected got in the way. Press Check again. If it keeps happening, restart your PC."
    }
}

pub fn show_failure(lang: Lang, error: &anyhow::Error) {
    eprintln!("{error:#}");
    let text = format!(
        "{}\n\n{}",
        lang.t("Secblitz couldn't open."),
        lang.t(friendly_problem(&format!("{error:#}")))
    );
    message_box("Secblitz", &text);
}

#[cfg_attr(not(windows), allow(dead_code))]
fn args_are_plain(args: &[String]) -> bool {
    args.iter()
        .all(|a| !a.is_empty() && a.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'))
}

pub fn run(lang: Lang) -> anyhow::Result<i32> {
    #[cfg(windows)]
    {
        imp::run(lang)
    }
    #[cfg(not(windows))]
    {
        let _ = lang;
        anyhow::bail!("Secblitz requires Windows")
    }
}

pub fn elevate_and_wait(args: &[String]) -> anyhow::Result<i32> {
    #[cfg(windows)]
    {
        match imp::elevate(args)? {
            Some(child) => child.wait(),
            None => anyhow::bail!("The administrator prompt was declined"),
        }
    }
    #[cfg(not(windows))]
    {
        secblitz::platform::elevate(args)?;
        Ok(0)
    }
}

pub enum Instance {
    First(#[allow(dead_code)] Guard),
    #[cfg_attr(not(windows), allow(dead_code))]
    Existing,
}

pub struct Guard {
    #[cfg(windows)]
    _handle: imp::Owned,
    #[cfg(windows)]
    _namespace: Option<imp::Namespace>,
}

pub fn single_instance() -> anyhow::Result<Instance> {
    #[cfg(windows)]
    {
        imp::single_instance()
    }
    #[cfg(not(windows))]
    {
        Ok(Instance::First(Guard {}))
    }
}

pub fn user_sid() -> Option<String> {
    #[cfg(windows)]
    {
        imp::user_sid_string().ok()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
mod imp {
    use super::{args_are_plain, Guard, Instance};
    use crate::broker::{self, Reply, Request};
    use crate::i18n::Lang;
    use crate::user_apps::{self, AppState};
    use crate::user_settings::{self, Op, Setting, SystemRegistry};
    use anyhow::{ensure, Context, Result};
    use std::{ffi::c_void, os::windows::ffi::OsStrExt, ptr::null_mut, time::Duration};
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, GetLastError, LocalFree, BOOL, ERROR_ALREADY_EXISTS, ERROR_CANCELLED,
            ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
        },
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                ConvertStringSidToSidW, SDDL_REVISION_1,
            },
            GetTokenInformation, TokenUser, PSID,
            SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
        },
        Storage::FileSystem::{
            ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
            PIPE_ACCESS_DUPLEX,
        },
        System::{
            Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe,
                GetNamedPipeClientProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_TYPE_BYTE, PIPE_WAIT,
            },
            RemoteDesktop::ProcessIdToSessionId,
            SystemInformation::GetSystemDirectoryW,
            Threading::{
                AddIntegrityLabelToBoundaryDescriptor, AddSIDToBoundaryDescriptor,
                ClosePrivateNamespace, CreateBoundaryDescriptorW, CreateEventW, CreateMutexW,
                CreatePrivateNamespaceW, DeleteBoundaryDescriptor, GetCurrentProcess,
                GetExitCodeProcess, GetProcessId, OpenPrivateNamespaceW, OpenProcessToken,
                ResetEvent, WaitForMultipleObjects, WaitForSingleObject,
            },
            IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
        },
        UI::{
            Shell::{ShellExecuteExW, ShellExecuteW, SHELLEXECUTEINFOW},
            WindowsAndMessaging::{
                EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
                SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOWNORMAL,
            },
        },
    };

    pub struct Owned(pub HANDLE);
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    pub struct Elevated(Owned);

    impl Elevated {
        fn pid(&self) -> u32 {
            unsafe { GetProcessId((self.0).0) }
        }
        pub fn wait(&self) -> Result<i32> {
            if unsafe { WaitForSingleObject((self.0).0, u32::MAX) } != WAIT_OBJECT_0 {
                return Err(std::io::Error::last_os_error().into());
            }
            let mut code = 1;
            if unsafe { GetExitCodeProcess((self.0).0, &mut code) } == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(code as i32)
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    pub fn elevate(args: &[String]) -> Result<Option<Elevated>> {
        ensure!(args_are_plain(args), "Invalid elevation arguments");
        let exe: Vec<u16> = std::env::current_exe()?
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let params = wide(&args.join(" "));
        let verb = wide("runas");
        let mut dir = vec![0u16; 32768];
        let n = unsafe { GetSystemDirectoryW(dir.as_mut_ptr(), dir.len() as u32) } as usize;
        ensure!(
            n > 0 && n < dir.len(),
            "Cannot resolve the System32 directory"
        );
        dir.truncate(n);
        dir.push(0);
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = 0x40 | 0x100; // SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC
        info.lpVerb = verb.as_ptr();
        info.lpFile = exe.as_ptr();
        info.lpParameters = params.as_ptr();
        info.lpDirectory = dir.as_ptr();
        info.nShow = SW_SHOWNORMAL;
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            if unsafe { GetLastError() } == ERROR_CANCELLED {
                return Ok(None);
            }
            return Err(std::io::Error::last_os_error().into());
        }
        ensure!(
            !info.hProcess.is_null(),
            "Elevation returned no process handle"
        );
        Ok(Some(Elevated(Owned(info.hProcess))))
    }

    pub fn user_sid_string() -> Result<String> {
        unsafe {
            let mut token: HANDLE = null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            let token = Owned(token);
            let mut needed = 0u32;
            GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut needed);
            ensure!(needed > 0 && needed < 4096, "token information unavailable");
            let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
            if GetTokenInformation(
                token.0,
                TokenUser,
                buf.as_mut_ptr().cast(),
                needed,
                &mut needed,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut raw: *mut u16 = null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut raw) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            let mut len = 0;
            while *raw.add(len) != 0 {
                len += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(raw, len));
            LocalFree(raw.cast());
            Ok(s)
        }
    }

    /// True for the elevated half of a split (UAC) admin token: someone chose
    /// "Run as administrator". The broker must then not run, because its
    /// user-context actions (winget, HKCU, protocol handlers) would carry the
    /// admin token while the same user's unelevated programs can steer them.
    /// Built-in Administrator and UAC-off accounts have no split token.
    fn split_token_elevated() -> Result<bool> {
        secblitz::actions::split_token_elevated()
    }

    fn create_pipe(id: &str) -> Result<Owned> {
        let sddl = wide(&format!("D:P(A;;GA;;;{})(A;;GA;;;BA)", user_sid_string()?));
        let mut descriptor: *mut c_void = null_mut();
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let name = wide(&broker::pipe_name(id));
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                64,
                64,
                0,
                &attributes,
            )
        };
        let error = std::io::Error::last_os_error();
        unsafe { LocalFree(descriptor) };
        if handle == INVALID_HANDLE_VALUE {
            return Err(error).context("Cannot create the broker channel");
        }
        Ok(Owned(handle))
    }

    enum Wake {
        Io,
        Child,
    }

    fn wait_io(event: HANDLE, child: HANDLE) -> Wake {
        let handles = [event, child];
        match unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, u32::MAX) } {
            WAIT_OBJECT_0 => Wake::Io,
            _ => Wake::Child,
        }
    }

    enum Read {
        Full,
        Closed,
        ChildGone,
    }

    fn read_request(pipe: HANDLE, event: HANDLE, child: HANDLE, buf: &mut [u8; 3]) -> Read {
        let mut done = 0usize;
        while done < buf.len() {
            let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
            overlapped.hEvent = event;
            unsafe { ResetEvent(event) };
            let rest = &mut buf[done..];
            let started = unsafe {
                ReadFile(
                    pipe,
                    rest.as_mut_ptr(),
                    rest.len() as u32,
                    null_mut(),
                    &mut overlapped,
                )
            };
            if started == 0 {
                if unsafe { GetLastError() } != ERROR_IO_PENDING {
                    return Read::Closed;
                }
                if let Wake::Child = wait_io(event, child) {
                    unsafe {
                        CancelIoEx(pipe, &overlapped);
                        let mut n = 0u32;
                        GetOverlappedResult(pipe, &overlapped, &mut n, 1);
                    }
                    return Read::ChildGone;
                }
            }
            let mut n = 0u32;
            if unsafe { GetOverlappedResult(pipe, &overlapped, &mut n, 0) } == 0 || n == 0 {
                return Read::Closed;
            }
            done += n as usize;
        }
        Read::Full
    }

    fn write_reply(pipe: HANDLE, event: HANDLE, child: HANDLE, reply: Reply) -> bool {
        let byte = [reply.encode()];
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.hEvent = event;
        unsafe { ResetEvent(event) };
        let started = unsafe { WriteFile(pipe, byte.as_ptr(), 1, null_mut(), &mut overlapped) };
        if started == 0 {
            if unsafe { GetLastError() } != ERROR_IO_PENDING {
                return false;
            }
            if let Wake::Child = wait_io(event, child) {
                unsafe {
                    CancelIoEx(pipe, &overlapped);
                    let mut n = 0u32;
                    GetOverlappedResult(pipe, &overlapped, &mut n, 1);
                }
                return false;
            }
        }
        let mut n = 0u32;
        unsafe { GetOverlappedResult(pipe, &overlapped, &mut n, 0) != 0 && n == 1 }
    }

    fn serve(pipe: &Owned, child: &Elevated) {
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if event.is_null() {
            return;
        }
        let event = Owned(event);
        let child_pid = child.pid();
        let child_handle = (child.0).0;
        let mut strangers = 0;
        loop {
            let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
            overlapped.hEvent = event.0;
            unsafe { ResetEvent(event.0) };
            if unsafe { ConnectNamedPipe(pipe.0, &mut overlapped) } == 0 {
                match unsafe { GetLastError() } {
                    ERROR_PIPE_CONNECTED => {}
                    ERROR_IO_PENDING => {
                        if let Wake::Child = wait_io(event.0, child_handle) {
                            unsafe {
                                CancelIoEx(pipe.0, &overlapped);
                                let mut n = 0u32;
                                GetOverlappedResult(pipe.0, &overlapped, &mut n, 1);
                            }
                            return;
                        }
                    }
                    _ => return,
                }
            }
            let mut client = 0u32;
            let verified = unsafe { GetNamedPipeClientProcessId(pipe.0, &mut client) } != 0
                && client == child_pid;
            if !verified {
                unsafe { DisconnectNamedPipe(pipe.0) };
                strangers += 1;
                if strangers >= 5 {
                    return;
                }
                continue;
            }
            loop {
                let mut request = [0u8; 3];
                match read_request(pipe.0, event.0, child_handle, &mut request) {
                    Read::Full => {}
                    Read::Closed => break,
                    Read::ChildGone => return,
                }
                let reply = match Request::decode(request) {
                    Some(request) => handle(request),
                    None => Reply::Unavailable,
                };
                if !write_reply(pipe.0, event.0, child_handle, reply) {
                    break;
                }
            }
            unsafe { DisconnectNamedPipe(pipe.0) };
            if unsafe { WaitForSingleObject(child_handle, 0) } == WAIT_OBJECT_0 {
                return;
            }
        }
    }

    pub fn run(lang: Lang) -> Result<i32> {
        if split_token_elevated()? {
            super::message_box(
                "Secblitz",
                &lang.t("Please open Secblitz the usual way, not with “Run as administrator”. It asks for permission by itself when it needs it."),
            );
            return Ok(1);
        }
        let id = broker::new_id();
        let pipe = create_pipe(&id)?;
        let args: Vec<String> = ["gui", "--broker", &id, "--lang", lang.code()]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let Some(child) = elevate(&args)? else {
            return Ok(0);
        };
        serve(&pipe, &child);
        drop(pipe);
        child.wait()
    }


    fn handle(request: Request) -> Reply {
        use secblitz::actions::{run, Action};
        let open = |action| match run(action) {
            Ok(_) => Reply::Done,
            Err(_) => Reply::Failed,
        };
        match request {
            Request::OpenWindowsUpdate => open(Action::OpenWindowsUpdate),
            Request::OpenWindowsSecurity => open(Action::OpenWindowsSecurity),
            Request::OpenEncryption => open(Action::OpenEncryptionSettings),
            Request::OpenSignIn => open(Action::OpenSignInSettings),
            Request::OpenTamperProtection => open(Action::OpenTamperProtection),
            Request::OpenProtectionHistory => open(Action::OpenProtectionHistory),
            Request::OpenAppBrowserControl => open(Action::OpenAppBrowserControl),
            Request::OpenOptionalFeatures => open(Action::OpenOptionalFeatures),
            Request::OpenAccounts => open(Action::OpenAccounts),
            Request::OpenCoreIsolation => open(Action::OpenCoreIsolation),
            Request::OpenFirewall => open(Action::OpenFirewall),
            Request::OpenDeviceSecurity => open(Action::OpenDeviceSecurity),
            Request::OpenWorkAccounts => open(Action::OpenWorkAccounts),
            Request::OpenRecovery => open(Action::OpenRecovery),
            Request::OpenRemoteDesktop => open(Action::OpenRemoteDesktop),
            Request::OpenFindMyDevice => open(Action::OpenFindMyDevice),
            Request::OpenBitLocker => open(Action::OpenBitLocker),
            Request::OpenWifi => open(Action::OpenWifi),
            Request::OpenNetwork => open(Action::OpenNetwork),
            Request::OpenBackup => open(Action::OpenBackup),
            Request::OpenStorage => open(Action::OpenStorage),
            Request::OpenInstalledApps => open(Action::OpenInstalledApps),
            Request::OpenProtectionHistoryList => open(Action::OpenProtectionHistoryList),
            Request::InstallBitwarden => match secblitz::tools::install_bitwarden() {
                Ok(()) => Reply::Done,
                Err(e) if secblitz::tools::is_offline_error(&e) => Reply::Offline,
                Err(e) if secblitz::tools::is_not_here_error(&e) => Reply::Unavailable,
                Err(_) => Reply::Failed,
            },
            Request::BitwardenStatus => {
                match secblitz::tools::bitwarden_installed() {
                    Ok(true) => Reply::Done,
                    Err(_) => Reply::Unknown,
                    Ok(false) => {
                        match secblitz::tools::bitwarden_installable() {
                            Ok(()) => Reply::NotApplicable,
                            Err(e) if secblitz::tools::is_not_here_error(&e) => Reply::Unavailable,
                            Err(_) => Reply::Unknown,
                        }
                    }
                }
            }
            Request::BlockSuggestedApps => user_setting(Setting::SuggestedApps, Op::Apply),
            Request::ReinstallStoreApp(index) => reinstall_store_app(index),
            Request::StartStoreApp(index) => start_store_app(index),
            Request::StoreAppStatus(index) => store_app_status(index),
            Request::UserSetting(setting, op) => user_setting(setting, op),
            Request::AppUpdatesScan => match scan_apps() {
                Ok(states) => {
                    user_apps::remember(states);
                    Reply::Done
                }
                Err(reply) => {
                    user_apps::forget();
                    reply
                }
            },
            Request::AppUpdateQuery(index) => match user_apps::remembered(usize::from(index)) {
                Some(AppState::Available) => Reply::UpdateAvailable,
                Some(AppState::NothingToDo) => Reply::NotApplicable,
                Some(AppState::Unknown) | None => Reply::Unknown,
            },
            Request::AppUpdate(index) => update_app(usize::from(index)),
        }
    }


    fn user_setting(setting: Setting, op: Op) -> Reply {
        let mut registry = SystemRegistry;
        let journal = user_settings::journal_path();
        match user_settings::handle(&mut registry, journal.as_deref(), setting, op) {
            Ok(result) => Reply::from_result(result),
            Err(_) if op == Op::Query => Reply::Unknown,
            Err(_) => Reply::Failed,
        }
    }


    fn scan_apps() -> Result<[AppState; user_apps::APPS.len()], Reply> {
        let run = user_apps::run_winget(&user_apps::list_args(), Duration::from_secs(150));
        if run.code.is_some_and(secblitz::tools::is_offline_code) {
            return Err(Reply::Offline);
        }
        if run.code.is_none() && run.output.trim().is_empty() {
            return Err(Reply::Unavailable);
        }
        match user_apps::parse_upgrades(&run.output, run.code) {
            user_apps::Scan::Apps(states) => Ok(states),
            user_apps::Scan::Unreadable if secblitz::tools::dns_offline() => Err(Reply::Offline),
            user_apps::Scan::Unreadable => Err(Reply::Unknown),
        }
    }

    fn update_app(index: usize) -> Reply {
        let Some(args) = user_apps::upgrade_args(index) else {
            return Reply::Unavailable;
        };
        let run = user_apps::run_winget(&args, Duration::from_secs(13 * 60));
        if run.code.is_some_and(secblitz::tools::is_offline_code) {
            return Reply::Offline;
        }
        if run.code.is_none() {
            return Reply::Failed;
        }
        match scan_apps() {
            Ok(states) => {
                let reply = match states[index] {
                    AppState::NothingToDo => Reply::Done,
                    AppState::Available => {
                        if run.code != Some(0) && secblitz::tools::dns_offline() {
                            Reply::Offline
                        } else {
                            Reply::Failed
                        }
                    }
                    AppState::Unknown => Reply::Unknown,
                };
                user_apps::remember(states);
                reply
            }
            Err(_) => {
                user_apps::forget();
                if run.code == Some(0) {
                    Reply::Unknown
                } else {
                    Reply::Failed
                }
            }
        }
    }

    fn valid_store_id(id: &str) -> bool {
        (1..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric())
    }

    fn store_id(index: u16) -> Option<&'static str> {
        secblitz::debloat::catalog()
            .get(usize::from(index))
            .and_then(|app| app.store_id)
            .filter(|id| valid_store_id(id))
    }

    fn store_install(store_id: &str) -> Option<std::process::Child> {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let winget = secblitz::tools::winget_path().ok()?;
        Command::new(winget)
            .args([
                "install",
                "--id",
                store_id,
                "--source",
                "msstore",
                "--accept-package-agreements",
                "--accept-source-agreements",
                "--exact",
                "--silent",
                "--disable-interactivity",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()
    }

    fn store_jobs() -> &'static std::sync::Mutex<std::collections::HashMap<u16, std::process::Child>>
    {
        static JOBS: std::sync::OnceLock<
            std::sync::Mutex<std::collections::HashMap<u16, std::process::Child>>,
        > = std::sync::OnceLock::new();
        JOBS.get_or_init(Default::default)
    }

    fn start_store_app(index: u16) -> Reply {
        let Some(store_id) = store_id(index) else {
            return Reply::Unavailable;
        };
        let Ok(mut jobs) = store_jobs().lock() else {
            return Reply::Failed;
        };
        if jobs.contains_key(&index) {
            return Reply::Done;
        }
        match store_install(store_id) {
            Some(child) => {
                jobs.insert(index, child);
                Reply::Done
            }
            None => Reply::Failed,
        }
    }

    fn store_app_status(index: u16) -> Reply {
        let Ok(mut jobs) = store_jobs().lock() else {
            return Reply::Failed;
        };
        let Some(child) = jobs.get_mut(&index) else {
            return Reply::Unavailable;
        };
        let code = match child.try_wait() {
            Ok(None) => return Reply::Working,
            Ok(Some(status)) => status.code().map(|c| c as u32),
            Err(_) => None,
        };
        jobs.remove(&index);
        if code == Some(0) {
            Reply::Done
        } else if code.is_some_and(secblitz::tools::is_offline_code) || secblitz::tools::dns_offline()
        {
            Reply::Offline
        } else {
            Reply::Failed
        }
    }

    fn reinstall_store_app(index: u16) -> Reply {
        let Some(store_id) = store_id(index) else {
            return Reply::Unavailable;
        };
        let exit = store_install(store_id)
            .and_then(|mut child| {
                let deadline = std::time::Instant::now() + Duration::from_secs(14 * 60);
                loop {
                    match child.try_wait() {
                        Ok(Some(status)) => return Some(status.code().map(|c| c as u32)),
                        Ok(None) if std::time::Instant::now() < deadline => {
                            std::thread::sleep(Duration::from_millis(500))
                        }
                        _ => {
                            let _ = child.kill();
                            let _ = child.wait();
                            return None;
                        }
                    }
                }
            });
        if exit == Some(Some(0)) {
            return Reply::Done;
        }
        let code = exit.flatten();
        if code.is_some_and(secblitz::tools::is_offline_code) || secblitz::tools::dns_offline() {
            return Reply::Offline;
        }
        let uri = wide(&format!("ms-windows-store://pdp/?ProductId={store_id}"));
        let verb = wide("open");
        let result = unsafe {
            ShellExecuteW(
                null_mut(),
                verb.as_ptr(),
                uri.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result as usize > 32 {
            Reply::OpenedStore
        } else {
            Reply::Failed
        }
    }


    pub struct Namespace(HANDLE);
    impl Drop for Namespace {
        fn drop(&mut self) {
            unsafe {
                ClosePrivateNamespace(self.0, 0);
            }
        }
    }

    struct Boundary(HANDLE);
    impl Drop for Boundary {
        fn drop(&mut self) {
            unsafe { DeleteBoundaryDescriptor(self.0) }
        }
    }

    struct Sid(PSID);
    impl Drop for Sid {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }

    fn sid(text: &str) -> Result<Sid> {
        let mut raw: PSID = null_mut();
        if unsafe { ConvertStringSidToSidW(wide(text).as_ptr(), &mut raw) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Sid(raw))
    }

    fn private_namespace() -> Result<Namespace> {
        let raw = unsafe { CreateBoundaryDescriptorW(wide("Secblitz").as_ptr(), 0) };
        ensure!(!raw.is_null(), "boundary descriptor unavailable");
        let mut boundary = Boundary(raw);
        let admins = sid("S-1-5-32-544")?;
        let high = sid("S-1-16-12288")?;
        unsafe {
            ensure!(
                AddSIDToBoundaryDescriptor(&mut boundary.0, admins.0) != 0
                    && AddIntegrityLabelToBoundaryDescriptor(&mut boundary.0, high.0) != 0,
                "boundary descriptor rejected"
            );
        }
        let sddl = wide("D:P(A;;GA;;;BA)(A;;GA;;;SY)");
        let mut descriptor: *mut c_void = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let prefix = wide("Secblitz");
        let mut handle =
            unsafe { CreatePrivateNamespaceW(&attributes, boundary.0, prefix.as_ptr()) };
        if handle.is_null() && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            handle = unsafe { OpenPrivateNamespaceW(boundary.0, prefix.as_ptr()) };
        }
        let error = std::io::Error::last_os_error();
        unsafe { LocalFree(descriptor) };
        if handle.is_null() {
            return Err(error.into());
        }
        Ok(Namespace(handle))
    }

    pub fn single_instance() -> Result<Instance> {
        let mut session = 0u32;
        unsafe { ProcessIdToSessionId(std::process::id(), &mut session) };
        let namespace = private_namespace().ok();
        let name = match namespace {
            Some(_) => wide(&format!("Secblitz\\Gui-{session}")),
            None => wide("Local\\SecblitzGui"),
        };
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let handle = Owned(handle);
        if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
            return Ok(Instance::First(Guard {
                _handle: handle,
                _namespace: namespace,
            }));
        }
        for _ in 0..20 {
            if focus_existing() {
                return Ok(Instance::Existing);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(Instance::Existing)
    }

    struct Search {
        exe: String,
        found: bool,
    }

    unsafe extern "system" fn visit(hwnd: HANDLE, param: isize) -> BOOL {
        let search = &mut *(param as *mut Search);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut title = [0u16; 64];
        let n = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
        if n <= 0 || String::from_utf16_lossy(&title[..n as usize]) != "Secblitz" {
            return 1;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let same = broker::image_path(pid)
            .map(|p| p.eq_ignore_ascii_case(&search.exe))
            .unwrap_or(false);
        if !same || pid == std::process::id() {
            return 1;
        }
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd);
        search.found = true;
        0
    }

    fn focus_existing() -> bool {
        let Ok(exe) = std::env::current_exe() else {
            return false;
        };
        let mut search = Search {
            exe: exe.to_string_lossy().into_owned(),
            found: false,
        };
        unsafe {
            EnumWindows(Some(visit), &mut search as *mut Search as isize);
        }
        search.found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_start_problems_get_a_fix_and_unknown_ones_get_the_general_text() {
        assert!(friendly_problem("The administrator prompt was declined").contains("choose Yes"));
        assert!(friendly_problem("Secblitz requires Windows").contains("Windows 10"));
        assert!(friendly_problem("Another Secblitz operation holds the journal lock")
            .contains("busy"));
        let raw = "zxq 0x80004005 src/engine.rs:42 panicked";
        let text = friendly_problem(raw);
        assert!(text.starts_with("Something unexpected"));
        assert!(!text.contains("0x8") && !text.contains("engine.rs"));
    }

    #[test]
    fn broad_words_do_not_misroute() {
        for raw in ["blocked by policy", "clock skew", "unlock failed", "invalid argument", "bad schema"] {
            assert!(friendly_problem(raw).starts_with("Something unexpected"), "{raw}");
        }
    }

    #[test]
    fn check_problems_say_to_check_again() {
        let text = friendly_check_problem("The administrator prompt was declined");
        assert!(text.contains("Press Check again") && !text.contains("open"));
        assert!(friendly_check_problem("Secblitz requires Windows").contains("Windows 10"));
    }

    #[test]
    fn elevation_arguments_are_plain_words_only() {
        let ok: Vec<String> = ["gui", "--broker", &"a".repeat(32), "--lang", "en"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(args_are_plain(&ok));
        for bad in ["a b", "a&b", "a\"b", "", "..\\x", "a;b"] {
            assert!(!args_are_plain(&[bad.to_string()]), "{bad}");
        }
    }
}
