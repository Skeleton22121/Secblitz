//! Unelevated entry point (no arguments): create the broker pipe, request UAC
//! for `gui --broker <id>` every time, serve broker requests until the GUI
//! exits. Also provides the GUI single-instance guard and native message boxes.
use crate::i18n::Lang;

/// Native, plain-text message box (never console output). Windows only; on
/// other hosts the text goes to stderr so tests and tooling still see it.
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

/// Show a start-up problem in calm words, with the cause under a label.
pub fn show_failure(lang: Lang, error: &anyhow::Error) {
    let text = format!(
        "{}\n\n{}: {error:#}",
        lang.t("Secblitz couldn't open. Please try again."),
        lang.t("Technical details")
    );
    message_box("Secblitz", &text);
}

/// Arguments may only be plain words, options and hex ids.
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

/// Run `secblitz.exe <args>` elevated and wait for it. A declined UAC prompt
/// is an error.
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

/// Only one dashboard window per user session.
pub enum Instance {
    /// This process owns the guard; keep it alive until exit.
    First(#[allow(dead_code)] Guard),
    /// Another Secblitz window exists (and was asked to come forward).
    #[cfg_attr(not(windows), allow(dead_code))]
    Existing,
}

pub struct Guard {
    #[cfg(windows)]
    _handle: imp::Owned,
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
                SDDL_REVISION_1,
            },
            GetTokenInformation, TokenUser, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
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
            Registry::{
                RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
                KEY_SET_VALUE, REG_DWORD,
            },
            Threading::{
                CreateEventW, CreateMutexW, GetCurrentProcess, GetExitCodeProcess, GetProcessId,
                OpenProcessToken, ResetEvent, WaitForMultipleObjects, WaitForSingleObject,
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

    /// UAC prompt for `secblitz.exe <args>`. `Ok(None)` when the user said no.
    pub fn elevate(args: &[String]) -> Result<Option<Elevated>> {
        // All arguments are fixed ASCII words/options or hex ids, so no
        // quoting is needed. The executable uses a separate field.
        ensure!(args_are_plain(args), "Invalid elevation arguments");
        let exe: Vec<u16> = std::env::current_exe()?
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let params = wide(&args.join(" "));
        let verb = wide("runas");
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = 0x40 | 0x100; // SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC
        info.lpVerb = verb.as_ptr();
        info.lpFile = exe.as_ptr();
        info.lpParameters = params.as_ptr();
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

    fn user_sid_string() -> Result<String> {
        unsafe {
            let mut token: HANDLE = null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            let token = Owned(token);
            let mut needed = 0u32;
            GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut needed);
            ensure!(needed > 0 && needed < 4096, "token information unavailable");
            // u64 storage keeps the TOKEN_USER view aligned.
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

    /// Single instance, current user + Administrators only, no remote clients.
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

    /// Wait for the pending overlapped operation or the child's exit.
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

    /// Accept the elevated GUI (and only it) and serve until it exits.
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
            // Only the elevated process we started may talk to us.
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
            // The GUI is gone or misbehaving; stop when the process is gone.
            if unsafe { WaitForSingleObject(child_handle, 0) } == WAIT_OBJECT_0 {
                return;
            }
        }
    }

    pub fn run(lang: Lang) -> Result<i32> {
        let id = broker::new_id();
        let pipe = create_pipe(&id)?;
        let args: Vec<String> = ["gui", "--broker", &id, "--lang", lang.code()]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let Some(child) = elevate(&args)? else {
            // The user chose not to continue; that is not an error.
            return Ok(0);
        };
        serve(&pipe, &child);
        drop(pipe);
        child.wait()
    }

    // ----- fixed user-context actions -----

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
            Request::InstallBitwarden => match secblitz::tools::install_bitwarden() {
                Ok(()) => Reply::Done,
                Err(e) if secblitz::tools::is_offline_error(&e) => Reply::Offline,
                Err(_) => Reply::Failed,
            },
            Request::BlockSuggestedApps => match block_suggested_apps() {
                Ok(()) => Reply::Done,
                Err(_) => Reply::Failed,
            },
            Request::ReinstallStoreApp(index) => reinstall_store_app(index),
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

    // ----- per-user settings (HKCU of the signed-in person) -----

    fn user_setting(setting: Setting, op: Op) -> Reply {
        let mut registry = SystemRegistry;
        let journal = user_settings::journal_path();
        match user_settings::handle(&mut registry, journal.as_deref(), setting, op) {
            Ok(result) => Reply::from_result(result),
            Err(_) if op == Op::Query => Reply::Unknown,
            Err(_) => Reply::Failed,
        }
    }

    // ----- app updates (WinGet, unelevated) -----

    /// One `winget upgrade` listing, read defensively.
    fn scan_apps() -> Result<[AppState; user_apps::APPS.len()], Reply> {
        let run = user_apps::run_winget(&user_apps::list_args(), Duration::from_secs(150));
        if run.code.is_some_and(secblitz::tools::is_offline_code) {
            return Err(Reply::Offline);
        }
        if run.code.is_none() && run.output.trim().is_empty() {
            // WinGet is missing or never answered.
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
        // Read again: only a program that no longer lists a newer version
        // counts as updated.
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

    const CONTENT_DELIVERY: &str =
        r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager";
    const SUGGESTION_VALUES: [&str; 7] = [
        "SilentInstalledAppsEnabled",
        "PreInstalledAppsEnabled",
        "OemPreInstalledAppsEnabled",
        "SubscribedContent-338388Enabled",
        "SubscribedContent-338389Enabled",
        "SubscribedContent-353694Enabled",
        "SubscribedContent-353696Enabled",
    ];

    fn block_suggested_apps() -> Result<()> {
        unsafe {
            let mut key: HKEY = null_mut();
            let path = wide(CONTENT_DELIVERY);
            let status = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                std::ptr::null(),
                0,
                KEY_SET_VALUE,
                std::ptr::null(),
                &mut key,
                null_mut(),
            );
            ensure!(status == 0, "Cannot open the preferences key ({status})");
            let zero = 0u32.to_le_bytes();
            let mut failed = false;
            for name in SUGGESTION_VALUES {
                let name = wide(name);
                failed |= RegSetValueExW(key, name.as_ptr(), 0, REG_DWORD, zero.as_ptr(), 4) != 0;
            }
            RegCloseKey(key);
            ensure!(!failed, "Some preferences could not be saved");
        }
        Ok(())
    }

    /// Store product ids are short alphanumeric strings (e.g. 9NBLGGH4NNS1).
    fn valid_store_id(id: &str) -> bool {
        (1..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric())
    }

    fn reinstall_store_app(index: u16) -> Reply {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        let Some(store_id) = secblitz::debloat::catalog()
            .get(usize::from(index))
            .and_then(|app| app.store_id)
        else {
            return Reply::Unavailable;
        };
        if !valid_store_id(store_id) {
            return Reply::Unavailable;
        }
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let exit = Command::new("winget")
            .args([
                "install",
                "--id",
                store_id,
                "--source",
                "msstore",
                "--accept-package-agreements",
                "--accept-source-agreements",
                "--silent",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()
            .and_then(|mut child| {
                // Give the install a generous but finite time.
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
        // `Some(Some(0))` = installed; `Some(Some(code))` = winget failed with
        // that code; `Some(None)`/`None` = no usable code or timed out.
        if exit == Some(Some(0)) {
            return Reply::Done;
        }
        // No connection: opening the Store would only show another error.
        let code = exit.flatten();
        if code.is_some_and(secblitz::tools::is_offline_code) || secblitz::tools::dns_offline() {
            return Reply::Offline;
        }
        // Fall back to the Store page so the person can install it there.
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

    // ----- single instance -----

    pub fn single_instance() -> Result<Instance> {
        let name = wide("Local\\SecblitzGui");
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let handle = Owned(handle);
        if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
            return Ok(Instance::First(Guard { _handle: handle }));
        }
        for _ in 0..20 {
            if focus_existing() {
                return Ok(Instance::Existing);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        // Someone holds the name but shows no window: still do not start twice.
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
