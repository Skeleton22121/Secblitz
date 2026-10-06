use super::*;
use super::{
    core::{Backend as _, Event, Phase},
    script::Action,
};
use std::{
    ffi::c_void,
    io::{Read, Write},
    mem::{size_of, zeroed},
    os::windows::io::AsRawHandle,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    ptr::null_mut,
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::*,
    System::{Registry::*, SystemInformation::*, Threading::*},
    UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId},
};

#[link(name = "kernel32")]
extern "system" {
    fn ProcessIdToSessionId(pid: u32, session: *mut u32) -> i32;
    fn PeekNamedPipe(
        pipe: HANDLE,
        buffer: *mut c_void,
        size: u32,
        read: *mut u32,
        available: *mut u32,
        left: *mut u32,
    ) -> i32;
}
#[link(name = "ntdll")]
extern "system" {
    fn NtQuerySystemInformation(
        class: i32,
        info: *mut c_void,
        size: u32,
        returned: *mut u32,
    ) -> i32;
}
#[link(name = "wtsapi32")]
extern "system" {
    fn WTSQuerySessionInformationW(
        server: HANDLE,
        session: u32,
        class: i32,
        buffer: *mut *mut u16,
        bytes: *mut u32,
    ) -> i32;
    fn WTSFreeMemory(memory: *mut c_void);
}
fn boot_id() -> Result<Uuid> {
    #[repr(C)]
    struct Boot {
        id: windows_sys::core::GUID,
        firmware: u32,
        flags: u64,
    }
    let mut boot: Boot = unsafe { zeroed() };
    let mut size = 0;
    ensure!(
        unsafe {
            NtQuerySystemInformation(
                90,
                (&mut boot as *mut Boot).cast(),
                size_of::<Boot>() as u32,
                &mut size,
            )
        } == 0
            && size as usize == size_of::<Boot>(),
        "Kernel boot identity unavailable"
    );
    let id = Uuid::from_fields(boot.id.data1, boot.id.data2, boot.id.data3, &boot.id.data4);
    ensure!(!id.is_nil(), "Invalid kernel boot identity");
    Ok(id)
}
fn active_session(session: u32) -> Result<()> {
    let (mut raw, mut bytes) = (null_mut(), 0);
    ensure!(
        unsafe { WTSQuerySessionInformationW(null_mut(), session, 8, &mut raw, &mut bytes) } != 0,
        "Interactive session state unavailable"
    );
    let active =
        !raw.is_null() && bytes as usize == size_of::<u32>() && unsafe { *raw.cast::<u32>() } == 0;
    if !raw.is_null() {
        unsafe {
            WTSFreeMemory(raw.cast());
        }
    }
    ensure!(active, "Original-user session is not active");
    Ok(())
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn windows_dir() -> Result<PathBuf> {
    let mut b = vec![0u16; 32768];
    let n = unsafe { GetWindowsDirectoryW(b.as_mut_ptr(), b.len() as u32) } as usize;
    ensure!(n > 0 && n < b.len(), "Windows directory unavailable");
    Ok(PathBuf::from(String::from_utf16(&b[..n])?))
}
pub(super) fn machine() -> Result<String> {
    let mut raw = [0u16; 256];
    let mut bytes = size_of_val(&raw) as u32;
    ensure!(
        unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                storage::wide("SOFTWARE\\Microsoft\\Cryptography").as_ptr(),
                storage::wide("MachineGuid").as_ptr(),
                RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
                null_mut(),
                raw.as_mut_ptr().cast(),
                &mut bytes,
            )
        } == 0
            && bytes >= 4
            && bytes as usize <= size_of_val(&raw),
        "Machine binding unavailable"
    );
    let end = raw
        .iter()
        .position(|c| *c == 0)
        .context("Invalid machine binding")?;
    hash(&(
        "secblitz.patching.machine.v1",
        Uuid::parse_str(&String::from_utf16(&raw[..end])?)?,
    ))
}
fn token(process: HANDLE) -> Result<Handle> {
    let mut token = null_mut();
    ensure!(
        unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } != 0,
        "Cannot inspect identity token"
    );
    Ok(Handle(token))
}
fn token_info<T: Copy>(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> Result<T> {
    let mut v: T = unsafe { zeroed() };
    let mut needed = 0;
    ensure!(
        unsafe {
            GetTokenInformation(
                token,
                class,
                (&mut v as *mut T).cast(),
                size_of::<T>() as u32,
                &mut needed,
            )
        } != 0
            && needed as usize == size_of::<T>(),
        "Cannot establish token information"
    );
    Ok(v)
}
fn user_sid(token: HANDLE) -> Result<String> {
    let mut needed = 0;
    unsafe {
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut needed);
    }
    ensure!(
        (size_of::<TOKEN_USER>() as u32..=65536).contains(&needed),
        "Invalid token user size"
    );
    // Aligned storage: TOKEN_USER contains a native pointer.
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
    let size = (buffer.len() * 8) as u32;
    ensure!(
        unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                size,
                &mut needed,
            )
        } != 0,
        "Cannot inspect token SID"
    );
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut text = null_mut();
    ensure!(
        unsafe {
            windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW(
                user.User.Sid,
                &mut text,
            )
        } != 0,
        "Cannot format token SID"
    );
    let _text = storage::Local(text.cast());
    let mut n = 0;
    while n < 256 && unsafe { *text.add(n) } != 0 {
        n += 1;
    }
    ensure!(n > 0 && n < 256, "Invalid SID length");
    Ok(String::from_utf16(unsafe {
        std::slice::from_raw_parts(text, n)
    })?)
}
fn binding(win: &Path) -> Result<Binding> {
    ensure!(
        cfg!(target_arch = "x86_64") && crate::platform::is_elevated()?,
        "Elevated Windows x64 required"
    );
    let mut system: SYSTEM_INFO = unsafe { zeroed() };
    unsafe {
        GetNativeSystemInfo(&mut system);
    }
    ensure!(
        unsafe { system.Anonymous.Anonymous.wProcessorArchitecture }
            == PROCESSOR_ARCHITECTURE_AMD64,
        "Patching requires native AMD64 Windows, not an emulated process"
    );
    let current = token(unsafe { GetCurrentProcess() })?;
    let ty: TOKEN_ELEVATION_TYPE = token_info(current.0, TokenElevationType)?;
    ensure!(ty == TokenElevationTypeFull, "Interactive split-token administrator required; service/over-the-shoulder elevation unsupported");
    let sid = user_sid(current.0)?;
    let linked: TOKEN_LINKED_TOKEN = token_info(current.0, TokenLinkedToken)?;
    let linked = Handle(linked.LinkedToken);
    let elevation: TOKEN_ELEVATION = token_info(linked.0, TokenElevation)?;
    ensure!(
        elevation.TokenIsElevated == 0 && user_sid(linked.0)? == sid,
        "Original unelevated identity not established"
    );
    let mut current_session = 0;
    ensure!(
        unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut current_session) } != 0
            && current_session != 0,
        "Interactive session required"
    );
    active_session(current_session)?;
    ensure!(
        token_info::<u32>(current.0, TokenSessionId)? == current_session
            && token_info::<u32>(linked.0, TokenSessionId)? == current_session,
        "Token session mismatch"
    );
    let shell_window = unsafe { GetShellWindow() };
    ensure!(!shell_window.is_null(), "Interactive shell unavailable");
    let mut pid = 0;
    ensure!(
        unsafe { GetWindowThreadProcessId(shell_window, &mut pid) } != 0 && pid != 0,
        "Shell identity unavailable"
    );
    let mut shell_session = 0;
    ensure!(
        unsafe { ProcessIdToSessionId(pid, &mut shell_session) } != 0
            && shell_session == current_session,
        "Different original-user session"
    );
    let shell = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    ensure!(!shell.is_null(), "Cannot inspect interactive shell");
    let shell = Handle(shell);
    let mut path = vec![0u16; 32768];
    let mut n = path.len() as u32;
    ensure!(
        unsafe { QueryFullProcessImageNameW(shell.0, 0, path.as_mut_ptr(), &mut n) } != 0,
        "Cannot establish shell executable"
    );
    let expected = win.join("explorer.exe");
    ensure!(
        String::from_utf16(&path[..n as usize])?.eq_ignore_ascii_case(&expected.to_string_lossy()),
        "Unsupported interactive shell"
    );
    let _pins = storage::pin_executable(&expected)?;
    let shell_token = token(shell.0)?;
    let elevation: TOKEN_ELEVATION = token_info(shell_token.0, TokenElevation)?;
    ensure!(
        elevation.TokenIsElevated == 0
            && user_sid(shell_token.0)? == sid
            && token_info::<u32>(shell_token.0, TokenSessionId)? == current_session,
        "Elevated caller is not the original desktop user"
    );
    let mut current_shell_pid = 0;
    ensure!(
        unsafe { GetShellWindow() } == shell_window
            && unsafe { GetWindowThreadProcessId(shell_window, &mut current_shell_pid) } != 0
            && current_shell_pid == pid,
        "Interactive shell changed during identity validation"
    );
    Ok(Binding {
        machine: machine()?,
        original_user: hash(&("secblitz.patching.original-user.v1", sid))?,
    })
}
pub(super) struct Backend {
    root: PathBuf,
    win: PathBuf,
    binding: Binding,
    cancel: Arc<AtomicBool>,
}
impl Backend {
    pub fn new(root: &Path) -> Result<Self> {
        let win = windows_dir()?;
        let binding = binding(&win)?;
        Ok(Self {
            root: root.to_owned(),
            win,
            binding,
            cancel: Arc::new(AtomicBool::new(false)),
        })
    }
    fn script(
        &self,
        action: Action,
        plan: Option<&Plan>,
        approval: Option<&Approval>,
        notify: &mut dyn FnMut(Event) -> Result<()>,
    ) -> Result<Vec<u8>> {
        let binding = self.binding()?;
        if let Some(p) = plan {
            ensure!(p.binding == binding, "Plan identity mismatch");
        }
        let text = script::build(action, plan, approval)?;
        let exe = self
            .win
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut pins = storage::pin_executable(&exe)?;
        for module in [
            "Microsoft.PowerShell.Management",
            "Microsoft.PowerShell.Utility",
            "CimCmdlets",
        ] {
            pins.extend(storage::pin_module(&self.win.join(format!(
                "System32/WindowsPowerShell/v1.0/Modules/{module}/{module}.psd1"
            )))?);
        }
        for dll in [
            "System32/MDMRegistration.dll",
            "System32/wuapi.dll",
            "System32/kernel32.dll",
        ] {
            pins.extend(storage::pin_executable(&self.win.join(dll))?);
        }
        let bootstrap = "[Console]::InputEncoding=[Text.UTF8Encoding]::new($false);[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); & ([ScriptBlock]::Create([Console]::In.ReadToEnd()))";
        let program_data = self
            .root
            .parent()
            .and_then(Path::parent)
            .context("Missing trusted ProgramData")?;
        let mut command = Command::new(exe);
        command
            .env_clear()
            .env("SystemRoot", &self.win)
            .env("WINDIR", &self.win)
            .env(
                "SystemDrive",
                self.win
                    .components()
                    .next()
                    .context("Missing system drive")?
                    .as_os_str(),
            )
            .env("ProgramData", program_data)
            .env("ALLUSERSPROFILE", program_data)
            .env("PATH", self.win.join("System32"))
            .env(
                "PSModulePath",
                self.win.join("System32/WindowsPowerShell/v1.0/Modules"),
            )
            .env("PSModuleAnalysisCachePath", "NUL")
            .env("TEMP", self.root.join("scratch"))
            .env("TMP", self.root.join("scratch"))
            .current_dir(self.win.join("System32"))
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                bootstrap,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(a) = approval {
            let t = now()?;
            ensure!(
                t >= a.approved_at && t < a.expires_at,
                "Approval expired before launch"
            );
        }
        ensure!(
            self.binding()? == binding,
            "Original-user identity changed during runtime pinning"
        );
        // Pins live until supervision ends, including pipe/timeout/journal errors.
        let result = supervise(
            command,
            text,
            if matches!(action, Action::Download | Action::Install) {
                3600
            } else {
                300
            },
            notify,
            approval,
            self.cancel.clone(),
        );
        drop(pins);
        result
    }
}
impl core::Backend for Backend {
    fn set_cancel(&mut self, cancel: Arc<AtomicBool>) {
        self.cancel = cancel;
    }
    fn binding(&self) -> Result<Binding> {
        let current = binding(&self.win)?;
        ensure!(
            current == self.binding,
            "Interactive original-user identity changed"
        );
        Ok(current)
    }
    fn discover(&mut self) -> Result<Catalog> {
        let data = self.script(Action::Discover, None, None, &mut |_| Ok(()))?;
        Ok(Catalog {
            binding: self.binding()?,
            searched_at: now()?,
            source: SOURCE.into(),
            updates: serde_json::from_slice(&data).context("Invalid WUA discovery output")?,
        })
    }
    fn execute(
        &mut self,
        phase: Phase,
        plan: &Plan,
        approval: &Approval,
        notify: &mut dyn FnMut(Event) -> Result<()>,
    ) -> Result<()> {
        let action = match phase {
            Phase::Download => Action::Download,
            Phase::Install => Action::Install,
        };
        let data = self.script(action, Some(plan), Some(approval), notify)?;
        script::acknowledged(&data)?;
        notify(Event::PhaseFinished)
    }
    fn verify(&mut self, plan: &Plan, process: Option<&ProcessIdentity>) -> Result<Verification> {
        if let Some(p) = process {
            ensure!(
                !alive(p)?,
                "Previous patching process is still alive; do not kill/replay"
            );
            // WUA uses out-of-process services, outside our job. A dead client
            // is not proof of request completion. An unacknowledged old phase
            // may only be verified after a DIFFERENT boot, then fresh quiescence
            // and installed-revision checks; never reset/replay its plan.
            core::recovery_after_boot(p, boot_id()?)?;
        }
        let data = self.script(Action::Verify, Some(plan), None, &mut |_| Ok(()))?;
        serde_json::from_slice(&data).context("Invalid independent WUA verification")
    }
}
fn identity(handle: HANDLE, pid: u32) -> Result<ProcessIdentity> {
    let (mut created, mut exit, mut kernel, mut user): (FILETIME, FILETIME, FILETIME, FILETIME) =
        unsafe { zeroed() };
    ensure!(
        unsafe { GetProcessTimes(handle, &mut created, &mut exit, &mut kernel, &mut user) } != 0,
        "Cannot identify patching process"
    );
    Ok(ProcessIdentity {
        pid,
        creation_time: ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64,
        boot_id: Some(boot_id()?),
    })
}
fn alive(expected: &ProcessIdentity) -> Result<bool> {
    let h = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | 0x0010_0000,
            0,
            expected.pid,
        )
    };
    if h.is_null() {
        ensure!(
            unsafe { GetLastError() } == ERROR_INVALID_PARAMETER,
            "Cannot establish previous process exit"
        );
        return Ok(false);
    }
    let h = Handle(h);
    if identity(h.0, expected.pid)? != *expected {
        return Ok(false);
    }
    let status = unsafe { WaitForSingleObject(h.0, 0) };
    ensure!(
        matches!(status, WAIT_OBJECT_0 | WAIT_TIMEOUT),
        "Previous process status unavailable"
    );
    Ok(status == WAIT_TIMEOUT)
}
fn drain(
    reader: &mut (impl Read + AsRawHandle),
    bytes: &mut Vec<u8>,
    cap: usize,
    overflow: &mut bool,
) -> Result<()> {
    for _ in 0..16 {
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
        {
            ensure!(
                unsafe { GetLastError() } == ERROR_BROKEN_PIPE,
                "Patching output pipe failed"
            );
            return Ok(());
        }
        if available == 0 {
            return Ok(());
        }
        let mut buf = [0; 8192];
        let n = reader.read(&mut buf[..(available as usize).min(8192)])?;
        let keep = n.min(cap.saturating_sub(bytes.len()));
        *overflow |= keep < n;
        bytes.extend_from_slice(&buf[..keep]);
    }
    Ok(())
}
fn supervise(
    command: Command,
    text: String,
    budget: u64,
    notify: &mut dyn FnMut(Event) -> Result<()>,
    approval: Option<&Approval>,
    cancel: Arc<AtomicBool>,
) -> Result<Vec<u8>> {
    let started = Instant::now();
    let mut child =
        process::spawn(&command, approval, &cancel).context("Start pinned Windows PowerShell")?;
    // NO fallible early returns after spawn. Journal Spawned BEFORE delivering
    // ANY script. A spawn/persistence gap therefore cannot submit a WUA mutation.
    let mut problem = identity(child.as_raw_handle(), child.id())
        .and_then(|p| notify(Event::Spawned(p)))
        .err();
    let writer = child.stdin.take().and_then(|mut input| {
        let send = problem.is_none();
        match std::thread::Builder::new()
            .name("patching-script-input".into())
            .spawn(move || {
                if send && !cancel.load(Ordering::SeqCst) {
                    input.write_all(text.as_bytes())
                } else {
                    Ok(())
                }
            }) {
            Ok(t) => Some(t),
            Err(e) => {
                problem.get_or_insert(e.into());
                None
            }
        }
    });
    let (mut stdout, mut stderr) = (child.stdout.take(), child.stderr.take());
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let (mut overflow, mut timed_out) = (false, false);
    let code = loop {
        if !timed_out && started.elapsed().as_secs() >= budget {
            timed_out = true;
            if let Err(e) = notify(Event::Timeout) {
                problem.get_or_insert(e);
            }
        }
        if let Some(r) = stdout.as_mut() {
            if let Err(e) = drain(r, &mut out, MAX_STATE_BYTES, &mut overflow) {
                problem.get_or_insert(e);
            }
        }
        if let Some(r) = stderr.as_mut() {
            if let Err(e) = drain(r, &mut err, 64 * 1024, &mut overflow) {
                problem.get_or_insert(e);
            }
        }
        match child.try_wait() {
            Ok(Some(exit)) => {
                // Drain finite buffered data after exit, never wait on descendant
                // EOF. Output cap plus fixed per-iteration reads bounds memory.
                for _ in 0..65 {
                    if let Some(r) = stdout.as_mut() {
                        if let Err(e) = drain(r, &mut out, MAX_STATE_BYTES, &mut overflow) {
                            problem.get_or_insert(e);
                        }
                    }
                    if let Some(r) = stderr.as_mut() {
                        if let Err(e) = drain(r, &mut err, 64 * 1024, &mut overflow) {
                            problem.get_or_insert(e);
                        }
                    }
                }
                break Some(exit);
            }
            Ok(None) => {}
            Err(e) => {
                problem.get_or_insert(e);
                // Root exit alone must never bypass failed descendant accounting.
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if let Some(writer) = writer {
        if writer.is_finished() {
            if !matches!(writer.join(), Ok(Ok(()))) {
                problem.get_or_insert_with(|| anyhow::anyhow!("Script delivery failed"));
            }
        } else {
            problem.get_or_insert_with(|| anyhow::anyhow!("Script delivery completion unknown"));
        }
    }
    if let Some(e) = problem {
        return Err(e);
    }
    ensure!(
        code == Some(0) && err.is_empty() && !overflow && !timed_out,
        "Patching subprocess {}; verification only (exit {}{}): {}",
        if timed_out { "timed out" } else { "failed" },
        code.map_or_else(|| "none".to_owned(), |c| format!("{c:#x}")),
        if overflow { ", output overflow" } else { "" },
        stderr_excerpt(&err)
    );
    Ok(out)
}

/// The start of the subprocess's error output as one printable line, for the
/// technical details (never parsed).
fn stderr_excerpt(err: &[u8]) -> String {
    let line = crate::text::excerpt(&String::from_utf8_lossy(err), 300);
    if line.is_empty() {
        "no error output".to_owned()
    } else {
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "native Windows boot/session query; no WUA, requires active same-user split-token admin"]
    fn boot_identity_and_original_user_binding_are_stable_without_wall_clock_estimates() {
        assert_eq!(boot_id().unwrap(), boot_id().unwrap());
        let win = windows_dir().unwrap();
        assert_eq!(binding(&win).unwrap(), binding(&win).unwrap());
    }
}
