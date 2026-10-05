//! Atomic job assignment and explicit pipe inheritance. No job resource limits,
//! kill-on-close or breakaway: a WUA phase must not be killed by its observer.
use super::*;
use std::{
    ffi::{c_void, OsStr},
    fs::File,
    io::Read,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, RawHandle},
    },
    process::Command,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{Foundation::*, Security::SECURITY_ATTRIBUTES, System::Threading::*};

#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(sa: *const SECURITY_ATTRIBUTES, name: *const u16) -> HANDLE;
    fn QueryInformationJobObject(
        job: HANDLE,
        class: i32,
        info: *mut c_void,
        size: u32,
        returned: *mut u32,
    ) -> i32;
    fn IsProcessInJob(process: HANDLE, job: HANDLE, result: *mut i32) -> i32;
    fn CreatePipe(
        read: *mut HANDLE,
        write: *mut HANDLE,
        sa: *const SECURITY_ATTRIBUTES,
        size: u32,
    ) -> i32;
    fn PeekNamedPipe(
        pipe: HANDLE,
        buffer: *mut c_void,
        size: u32,
        read: *mut u32,
        available: *mut u32,
        left: *mut u32,
    ) -> i32;
    fn InitializeProcThreadAttributeList(
        list: *mut c_void,
        count: u32,
        flags: u32,
        size: *mut usize,
    ) -> i32;
    fn UpdateProcThreadAttribute(
        list: *mut c_void,
        flags: u32,
        attr: usize,
        value: *mut c_void,
        size: usize,
        previous: *mut c_void,
        returned: *mut usize,
    ) -> i32;
    fn DeleteProcThreadAttributeList(list: *mut c_void);
}
#[repr(C)]
struct Accounting {
    user: i64,
    kernel: i64,
    period_user: i64,
    period_kernel: i64,
    faults: u32,
    total: u32,
    active: u32,
    terminated: u32,
}
#[repr(C)]
struct Startup {
    base: STARTUPINFOW,
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
fn check(ok: i32) -> Result<()> {
    ensure!(
        ok != 0,
        "Native patching process setup: {}",
        std::io::Error::last_os_error()
    );
    Ok(())
}
fn wide(value: &OsStr) -> Result<Vec<u16>> {
    let mut value: Vec<_> = value.encode_wide().collect();
    ensure!(!value.contains(&0), "NUL in native process input");
    value.push(0);
    Ok(value)
}
fn pipe() -> Result<(File, File)> {
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let (mut read, mut write) = (null_mut(), null_mut());
    check(unsafe { CreatePipe(&mut read, &mut write, &sa, 0) })?;
    Ok(unsafe { (File::from_raw_handle(read), File::from_raw_handle(write)) })
}
pub(super) struct Child {
    process: Handle,
    job: Handle,
    pid: u32,
    pub stdin: Option<File>,
    pub stdout: Option<File>,
    pub stderr: Option<File>,
}
impl AsRawHandle for Child {
    fn as_raw_handle(&self) -> RawHandle {
        self.process.0
    }
}
impl Child {
    pub fn id(&self) -> u32 {
        self.pid
    }
    pub fn try_wait(&self) -> Result<Option<u32>> {
        let wait = unsafe { WaitForSingleObject(self.process.0, 0) };
        ensure!(
            matches!(wait, WAIT_OBJECT_0 | WAIT_TIMEOUT),
            "Cannot wait for patching process"
        );
        if wait == WAIT_TIMEOUT {
            return Ok(None);
        }
        let mut info: Accounting = unsafe { zeroed() };
        check(unsafe {
            QueryInformationJobObject(
                self.job.0,
                1,
                (&mut info as *mut Accounting).cast(),
                size_of::<Accounting>() as u32,
                null_mut(),
            )
        })?;
        if info.active != 0 {
            return Ok(None);
        }
        let mut code = 0;
        check(unsafe { GetExitCodeProcess(self.process.0, &mut code) })?;
        Ok(Some(code))
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        // Unwinding must not release the outer store/pins while any descendant
        // survives. Close undispatched input and drain owned output, never kill.
        drop(self.stdin.take());
        while !matches!(self.try_wait(), Ok(Some(_))) {
            for pipe in [&mut self.stdout, &mut self.stderr].into_iter().flatten() {
                let mut available = 0;
                if unsafe {
                    PeekNamedPipe(
                        pipe.as_raw_handle(),
                        null_mut(),
                        0,
                        null_mut(),
                        &mut available,
                        null_mut(),
                    )
                } != 0
                    && available != 0
                {
                    let mut bytes = [0; 8192];
                    let _ = pipe.read(&mut bytes[..(available as usize).min(8192)]);
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

pub(super) fn spawn(
    command: &Command,
    permit: Option<&Approval>,
    cancel: &AtomicBool,
) -> Result<Child> {
    // An enclosing job can impose kill/resource limits even on an unlimited
    // child job. No breakaway fallback or post-start assignment window is used.
    let mut in_job = 0;
    check(unsafe { IsProcessInJob(GetCurrentProcess(), null_mut(), &mut in_job) })?;
    ensure!(
        in_job == 0,
        "Patching cannot inherit an enclosing process job"
    );
    let application = wide(command.get_program())?;
    let mut line = Vec::new();
    for value in std::iter::once(command.get_program()).chain(command.get_args()) {
        let value = wide(value)?;
        let value = &value[..value.len() - 1];
        ensure!(
            !value.contains(&(b'"' as u16)) && value.last() != Some(&(b'\\' as u16)),
            "Unsupported process argument"
        );
        if !line.is_empty() {
            line.push(b' ' as u16);
        }
        line.push(b'"' as u16);
        line.extend_from_slice(value);
        line.push(b'"' as u16);
    }
    line.push(0);
    let cwd = wide(
        command
            .get_current_dir()
            .context("Missing trusted cwd")?
            .as_os_str(),
    )?;
    let mut environment = Vec::new();
    for (key, value) in command.get_envs() {
        let key = wide(key)?;
        ensure!(!key.contains(&(b'=' as u16)), "Invalid environment key");
        environment.extend_from_slice(&key[..key.len() - 1]);
        environment.push(b'=' as u16);
        environment.extend(wide(value.context("Missing explicit environment value")?)?);
    }
    ensure!(!environment.is_empty(), "Empty native environment");
    environment.push(0);
    let job = unsafe { CreateJobObjectW(null(), null()) };
    ensure!(!job.is_null(), "Cannot create patching supervision job");
    let job = Handle(job);
    let (input, writer) = pipe()?;
    let (reader, output) = pipe()?;
    let (errors, error_output) = pipe()?;
    for f in [&writer, &reader, &errors] {
        check(unsafe { SetHandleInformation(f.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) })?;
    }
    let mut size = 0;
    unsafe {
        InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut size);
    }
    ensure!(size > 0 && size <= 65536, "Invalid process attribute size");
    let mut buffer = vec![0usize; size.div_ceil(size_of::<usize>())];
    check(unsafe {
        InitializeProcThreadAttributeList(buffer.as_mut_ptr().cast(), 2, 0, &mut size)
    })?;
    let mut attributes = Attributes(buffer);
    let mut inherited = [
        input.as_raw_handle(),
        output.as_raw_handle(),
        error_output.as_raw_handle(),
    ];
    let mut jobs = [job.0];
    for (attribute, value, size) in [
        (
            0x00020002,
            inherited.as_mut_ptr().cast(),
            size_of_val(&inherited),
        ),
        (0x0002000d, jobs.as_mut_ptr().cast(), size_of_val(&jobs)),
    ] {
        check(unsafe {
            UpdateProcThreadAttribute(
                attributes.0.as_mut_ptr().cast(),
                0,
                attribute,
                value,
                size,
                null_mut(),
                null_mut(),
            )
        })?;
    }
    let mut startup: Startup = unsafe { zeroed() };
    startup.base.cb = size_of::<Startup>() as u32;
    startup.base.dwFlags = STARTF_USESTDHANDLES;
    startup.base.hStdInput = input.as_raw_handle();
    startup.base.hStdOutput = output.as_raw_handle();
    startup.base.hStdError = error_output.as_raw_handle();
    startup.attributes = attributes.0.as_mut_ptr().cast();
    let ready = || -> Result<()> {
        ensure!(
            !cancel.load(Ordering::SeqCst),
            "Patching cancelled before process launch"
        );
        if let Some(a) = permit {
            let time = now()?;
            ensure!(
                time >= a.approved_at && time < a.expires_at,
                "Approval expired during native process setup"
            );
        }
        Ok(())
    };
    ready()?;
    let mut info: PROCESS_INFORMATION = unsafe { zeroed() };
    check(unsafe {
        CreateProcessW(
            application.as_ptr(),
            line.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_NO_WINDOW | CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | 0x00080000,
            environment.as_ptr().cast(),
            cwd.as_ptr(),
            &startup.base,
            &mut info,
        )
    })?;
    let process = Handle(info.hProcess);
    let thread = Handle(info.hThread);
    if let Err(error) = ready() {
        // Never-started allocation only: no script or WUA call has run.
        unsafe {
            TerminateProcess(process.0, 1);
            WaitForSingleObject(process.0, INFINITE);
        }
        return Err(error);
    }
    if unsafe { ResumeThread(thread.0) } == u32::MAX {
        unsafe {
            TerminateProcess(process.0, 1);
            WaitForSingleObject(process.0, INFINITE);
        }
        bail!("Cannot resume supervised patching process");
    }
    Ok(Child {
        process,
        job,
        pid: info.dwProcessId,
        stdin: Some(writer),
        stdout: Some(reader),
        stderr: Some(errors),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use std::time::Instant;

    fn command(script: &str) -> (Command, Vec<File>) {
        let mut path = [0u16; 32768];
        let n = unsafe {
            windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW(
                path.as_mut_ptr(),
                path.len() as u32,
            )
        } as usize;
        assert!(n > 0 && n < path.len());
        let win = std::path::PathBuf::from(String::from_utf16(&path[..n]).unwrap());
        let exe = win.join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let pins = storage::pin_executable(&exe).unwrap();
        let encoded = base64::engine::general_purpose::STANDARD.encode(
            script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let mut command = Command::new(exe);
        command
            .env_clear()
            .env("SystemRoot", &win)
            .env("WINDIR", &win)
            .current_dir(win.join("System32"))
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                &encoded,
            ]);
        (command, pins)
    }

    #[test]
    #[ignore = "native Windows job test outside enclosing jobs; synthetic sleepers only, no WUA"]
    fn direct_exit_keeps_descendant_supervision_and_drop_never_kills() {
        let (command, _pins) = command("$s=[Diagnostics.ProcessStartInfo]::new();$s.FileName=[Diagnostics.Process]::GetCurrentProcess().MainModule.FileName;$s.Arguments='-NoLogo -NoProfile -NonInteractive -Command [Console]::Out.WriteLine(123); Start-Sleep -Seconds 5';$s.UseShellExecute=$false;$s.RedirectStandardOutput=$true;$p=[Diagnostics.Process]::Start($s);$null=$p.StandardOutput.ReadLine()");
        let mut child = spawn(&command, None, &AtomicBool::new(false)).unwrap();
        drop(child.stdin.take());
        assert_eq!(
            unsafe { WaitForSingleObject(child.as_raw_handle(), 15000) },
            WAIT_OBJECT_0
        );
        assert_eq!(
            child.try_wait().unwrap(),
            None,
            "direct exit is not descendant exit"
        );
        let started = Instant::now();
        drop(child); // must wait, not terminate the descendant or release outer pins early
        assert!(started.elapsed() >= Duration::from_secs(1));
    }

    #[test]
    #[ignore = "native Windows process test outside enclosing jobs; no WUA"]
    fn cancelled_or_expired_permit_never_launches() {
        let (command, _pins) = command("throw 'must never run'");
        assert!(spawn(&command, None, &AtomicBool::new(true)).is_err());
        let permit = Approval {
            digest: "0".repeat(64),
            approved_at: 1,
            expires_at: 2,
            consent: Consent {
                owner_opt_in: true,
                accept_windows_update_source: true,
                accept_reviewed_eulas: true,
                acknowledge_no_automatic_rollback: true,
            },
        };
        assert!(spawn(&command, Some(&permit), &AtomicBool::new(false)).is_err());
    }
}
