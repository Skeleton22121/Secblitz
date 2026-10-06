//! Lets an update coexist with the per-user tray agent: signal the quiesce event, wait for tray
//! processes to exit, relaunch the tray unelevated. The tray is only asked to leave, never killed.
use anyhow::{ensure, Context, Result};
use std::{
    ffi::c_void,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_OBJECT_0},
    Security::{
        Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1},
        DuplicateTokenEx, GetTokenInformation, SecurityImpersonation, TokenElevationType,
        TokenLinkedToken, TokenPrimary, SECURITY_ATTRIBUTES, TOKEN_ALL_ACCESS,
    },
    System::{
        RemoteDesktop::{
            ProcessIdToSessionId, WTSActive, WTSEnumerateSessionsW, WTSFreeMemory,
            WTSQueryUserToken, WTS_SESSION_INFOW,
        },
        Threading::{
            CreateEventW, CreateProcessAsUserW, OpenProcess, SetEvent, WaitForSingleObject,
            CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION, PROCESS_SYNCHRONIZE, STARTUPINFOW,
        },
    },
};

pub(super) const QUIESCE_EVENT: &str = "Global\\SecblitzUpdateQuiesce";
/// SYSTEM full control; Users may only wait on it and read its owner
/// (SYNCHRONIZE | READ_CONTROL): trays honour the event only when SYSTEM or
/// Administrators own it, so without READ_CONTROL no tray could ever check.
const QUIESCE_SD: &str = "D:P(A;;GA;;;SY)(A;;0x00120000;;;BU)";

#[link(name = "ntdll")]
extern "system" {
    fn NtQueryInformationProcess(
        process: HANDLE,
        class: u32,
        info: *mut c_void,
        len: u32,
        returned: *mut u32,
    ) -> i32;
}
#[link(name = "userenv")]
extern "system" {
    fn CreateEnvironmentBlock(env: *mut *mut c_void, token: HANDLE, inherit: i32) -> i32;
    fn DestroyEnvironmentBlock(env: *mut c_void) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(p: *mut c_void) -> *mut c_void;
}

fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

/// Read a process command line (`ProcessCommandLineInformation`, class 60) with
/// only `PROCESS_QUERY_LIMITED_INFORMATION`.
pub(super) fn command_line(process: HANDLE) -> Result<String> {
    const CLASS: u32 = 60;
    let mut size = 1024u32;
    for _ in 0..4 {
        let mut buf = vec![0u64; (size as usize).div_ceil(8)];
        let mut needed = 0u32;
        let status = unsafe {
            NtQueryInformationProcess(
                process,
                CLASS,
                buf.as_mut_ptr().cast(),
                (buf.len() * 8) as u32,
                &mut needed,
            )
        };
        if status == 0xC0000004u32 as i32 || status == 0xC0000023u32 as i32 {
            ensure!(needed as usize <= 65536 + 64, "Command line too large");
            size = needed.max(size + 8);
            continue;
        }
        ensure!(
            status >= 0,
            "Cannot read process command line ({status:#x})"
        );
        #[repr(C)]
        struct UnicodeString {
            length: u16,
            maximum: u16,
            buffer: *const u16,
        }
        let header = unsafe { &*(buf.as_ptr() as *const UnicodeString) };
        let start = buf.as_ptr() as usize;
        let end = start + buf.len() * 8;
        let at = header.buffer as usize;
        ensure!(
            header.length % 2 == 0
                && at >= start + std::mem::size_of::<UnicodeString>()
                && at + header.length as usize <= end,
            "Invalid command line"
        );
        let units =
            unsafe { std::slice::from_raw_parts(header.buffer, header.length as usize / 2) };
        return Ok(String::from_utf16(units)?);
    }
    anyhow::bail!("Command line changed while reading")
}

pub(super) fn session_of(pid: u32) -> Option<u32> {
    let mut session = 0;
    (unsafe { ProcessIdToSessionId(pid, &mut session) } != 0).then_some(session)
}

/// The signalled quiesce event. Dropping it closes the handle, which destroys
/// the event, so a relaunched tray never sees a stale signal.
pub(super) struct Quiesce {
    _held: Handle,
}
impl Quiesce {
    pub(super) fn signal() -> Result<Self> {
        let mut sd = null_mut();
        let sddl = wide(QUIESCE_SD);
        ensure!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut sd,
                    null_mut(),
                )
            } != 0,
            "Cannot build quiesce descriptor"
        );
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd,
            bInheritHandle: 0,
        };
        let name = wide(QUIESCE_EVENT);
        let event = unsafe { CreateEventW(&attributes, 1, 0, name.as_ptr()) };
        let code = unsafe { GetLastError() };
        unsafe {
            LocalFree(sd);
        }
        ensure!(!event.is_null(), "Cannot create quiesce event ({code})");
        let event = Handle(event);
        // An existing event was not created by this worker; do not trust it.
        ensure!(code != ERROR_ALREADY_EXISTS, "Quiesce event already exists");
        ensure!(
            unsafe { SetEvent(event.0) } != 0,
            "Cannot signal quiesce event"
        );
        Ok(Self { _held: event })
    }
}

pub(super) fn wait_for_exit(pids: &[u32], timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    for pid in pids {
        let h = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, *pid) };
        if h.is_null() {
            continue; // Already gone (or inaccessible, which a tray never is for SYSTEM).
        }
        let h = Handle(h);
        let ms = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(u32::MAX as u128 - 1) as u32;
        if unsafe { WaitForSingleObject(h.0, ms) } != WAIT_OBJECT_0 {
            return false;
        }
    }
    true
}

fn active_sessions() -> Vec<u32> {
    let mut list: *mut WTS_SESSION_INFOW = null_mut();
    let mut count = 0;
    if unsafe { WTSEnumerateSessionsW(null_mut(), 0, 1, &mut list, &mut count) } == 0 {
        return Vec::new();
    }
    let sessions = (0..count as usize)
        .map(|i| unsafe { &*list.add(i) })
        .filter(|s| s.State == WTSActive && s.SessionId != 0)
        .map(|s| s.SessionId)
        .collect();
    unsafe {
        WTSFreeMemory(list.cast());
    }
    sessions
}

fn user_token(session: u32) -> Option<Handle> {
    let mut raw = null_mut();
    if unsafe { WTSQueryUserToken(session, &mut raw) } == 0 {
        return None;
    }
    let token = Handle(raw);
    // For administrators WTSQueryUserToken may return the elevated half of a
    // split token; always run the tray with the limited one.
    let mut kind = 0i32;
    let mut returned = 0;
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TokenElevationType,
            (&mut kind as *mut i32).cast(),
            4,
            &mut returned,
        )
    };
    if ok != 0 && kind == 2 {
        // TokenElevationTypeFull
        let mut linked: HANDLE = null_mut();
        let ok = unsafe {
            GetTokenInformation(
                token.0,
                TokenLinkedToken,
                (&mut linked as *mut HANDLE).cast(),
                std::mem::size_of::<HANDLE>() as u32,
                &mut returned,
            )
        };
        if ok == 0 {
            return None;
        }
        let linked = Handle(linked);
        let mut primary = null_mut();
        let ok = unsafe {
            DuplicateTokenEx(
                linked.0,
                TOKEN_ALL_ACCESS,
                null(),
                SecurityImpersonation,
                TokenPrimary,
                &mut primary,
            )
        };
        return (ok != 0).then_some(Handle(primary));
    }
    Some(token)
}

fn launch_in_session(exe: &Path, session: u32) -> Result<()> {
    let token = user_token(session).context("No user token")?;
    let mut env: *mut c_void = null_mut();
    // Without the user's own environment the tray would inherit SYSTEM's; the
    // Run key starts it at the next logon instead.
    ensure!(
        unsafe { CreateEnvironmentBlock(&mut env, token.0, 0) } != 0,
        "Cannot build the user environment"
    );
    let application = wide(exe);
    let mut command = wide(format!("\"{}\" tray", exe.display()));
    let directory = wide(exe.parent().context("Installed executable has no folder")?);
    let mut desktop = wide("winsta0\\default");
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    startup.lpDesktop = desktop.as_mut_ptr();
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        CreateProcessAsUserW(
            token.0,
            application.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            0,
            CREATE_UNICODE_ENVIRONMENT,
            env,
            directory.as_ptr(),
            &startup,
            &mut info,
        )
    };
    let code = unsafe { GetLastError() };
    unsafe {
        DestroyEnvironmentBlock(env);
    }
    ensure!(ok != 0, "Cannot start the tray ({code})");
    unsafe {
        CloseHandle(info.hThread);
        CloseHandle(info.hProcess);
    }
    Ok(())
}

/// Scope guard around an update that needs the tray out of the way. Dropping it
/// (success, failure or early return) first destroys the quiesce event and then
/// restarts the tray in the sessions that had one.
pub(super) struct TrayRestore {
    exe: PathBuf,
    sessions: Vec<u32>,
    event: Option<Quiesce>,
}
impl TrayRestore {
    pub(super) fn new(exe: &Path, tray_pids: &[u32]) -> Self {
        let mut sessions: Vec<u32> = tray_pids.iter().filter_map(|p| session_of(*p)).collect();
        sessions.sort_unstable();
        sessions.dedup();
        Self {
            exe: exe.to_owned(),
            sessions,
            event: None,
        }
    }
    pub(super) fn quiesce(&mut self) -> Result<()> {
        // Nothing to ask when no tray runs. A squatted event name only means the
        // trays cannot be asked; the caller then defers the update.
        if !self.sessions.is_empty() {
            self.event = Quiesce::signal().ok();
        }
        Ok(())
    }
}
impl Drop for TrayRestore {
    fn drop(&mut self) {
        self.event = None; // Close first: a restarted tray must not see the signal.
        if self.sessions.is_empty() {
            return;
        }
        let active = active_sessions();
        for session in self.sessions.iter().filter(|s| active.contains(s)) {
            let _ = launch_in_session(&self.exe, *session);
        }
    }
}
