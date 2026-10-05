//! Atomic job assignment with explicit pipe inheritance. Servicing jobs have no
//! kill/memory/CPU/active-process limits: cancellation must not damage servicing.
use super::*;
use std::{
    ffi::{c_void, OsStr},
    fs::File,
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
    fn SetInformationJobObject(job: HANDLE, class: i32, info: *const c_void, size: u32) -> i32;
    fn QueryInformationJobObject(
        job: HANDLE,
        class: i32,
        info: *mut c_void,
        size: u32,
        returned: *mut u32,
    ) -> i32;
    fn CreatePipe(
        read: *mut HANDLE,
        write: *mut HANDLE,
        sa: *const SECURITY_ATTRIBUTES,
        size: u32,
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
struct Limits {
    basic: BasicLimits,
    io: IO_COUNTERS,
    process_memory: usize,
    job_memory: usize,
    peak_process_memory: usize,
    peak_job_memory: usize,
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
        "Native process setup: {}",
        std::io::Error::last_os_error()
    );
    Ok(())
}
fn wide(value: &OsStr) -> Result<Vec<u16>> {
    let value: Vec<_> = value.encode_wide().collect();
    ensure!(!value.contains(&0), "NUL in native process input");
    Ok(value.into_iter().chain(Some(0)).collect())
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
    /// A direct child exit is insufficient: descendants must leave the job too.
    pub fn try_wait(&self) -> Result<Option<u32>> {
        let wait = unsafe { WaitForSingleObject(self.process.0, 0) };
        ensure!(
            matches!(wait, WAIT_OBJECT_0 | WAIT_TIMEOUT),
            "Cannot wait for process"
        );
        if wait == WAIT_TIMEOUT {
            return Ok(None);
        }
        let mut accounting: Accounting = unsafe { zeroed() };
        check(unsafe {
            QueryInformationJobObject(
                self.job.0,
                1,
                (&mut accounting as *mut Accounting).cast(),
                size_of::<Accounting>() as u32,
                null_mut(),
            )
        })?;
        if accounting.active != 0 {
            return Ok(None);
        }
        let mut code = 0;
        check(unsafe { GetExitCodeProcess(self.process.0, &mut code) })?;
        Ok(Some(code))
    }
}

pub(super) fn spawn(
    command: &Command,
    read_only: bool,
    permit: Option<&core::LaunchPermit>,
    control: Option<&core::Control>,
) -> Result<Child> {
    // An inherited enclosing job can impose termination/resource limits even
    // when our own job has none. Never gamble with OS servicing: start outside
    // it when it allows that (Windows' compatibility assistant job does), else
    // refuse. The child joins our job atomically at creation, still suspended.
    let mut breakaway = 0;
    if !read_only {
        match crate::platform::enclosing_job()? {
            crate::platform::EnclosingJob::None => {}
            crate::platform::EnclosingJob::Breakaway => breakaway = CREATE_BREAKAWAY_FROM_JOB,
            crate::platform::EnclosingJob::Locked => {
                bail!("Servicing cannot inherit an enclosing process job")
            }
        }
    }
    let application = wide(command.get_program())?;
    // All arguments are compiled switches/base64. Reject quoting ambiguity
    // rather than grow a general shell/Windows argument interpreter here.
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
        let value = value.context("Missing explicit environment value")?;
        let key = wide(key)?;
        ensure!(!key.contains(&(b'=' as u16)), "Invalid environment key");
        environment.extend_from_slice(&key[..key.len() - 1]);
        environment.push(b'=' as u16);
        environment.extend(wide(value)?);
    }
    ensure!(!environment.is_empty(), "Empty native environment");
    environment.push(0);
    let job = unsafe { CreateJobObjectW(null(), null()) };
    ensure!(!job.is_null(), "Cannot create supervision job");
    let job = Handle(job);
    if read_only {
        let mut limits: Limits = unsafe { zeroed() };
        limits.basic.flags = 0x2000 | 0x8 | 0x100; // kill on close, no children, memory cap
        limits.basic.active_processes = 1;
        limits.process_memory = 512 * 1024 * 1024;
        check(unsafe {
            SetInformationJobObject(
                job.0,
                9,
                (&limits as *const Limits).cast(),
                size_of::<Limits>() as u32,
            )
        })?;
    }
    let (input, writer) = pipe()?;
    let (reader, output) = pipe()?;
    let (errors, error_output) = pipe()?;
    for file in [&writer, &reader, &errors] {
        check(unsafe { SetHandleInformation(file.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) })?;
    }
    let mut bytes = 0;
    unsafe {
        InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut bytes);
    }
    ensure!(
        bytes > 0 && bytes <= 65536,
        "Invalid process attribute size"
    );
    let mut buffer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
    check(unsafe {
        InitializeProcThreadAttributeList(buffer.as_mut_ptr().cast(), 2, 0, &mut bytes)
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
        ), // HANDLE_LIST
        (0x0002000d, jobs.as_mut_ptr().cast(), size_of_val(&jobs)), // JOB_LIST, Windows 10+
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
    let mut info: PROCESS_INFORMATION = unsafe { zeroed() };
    if let Some(permit) = permit {
        let time = now()?;
        ensure!(
            time >= permit.not_before && time < permit.expires_at,
            "Launch readiness expired during process setup"
        );
    }
    ensure!(
        !control.is_some_and(core::Control::cancelled),
        "Cancelled before process creation"
    );
    check(unsafe {
        CreateProcessW(
            application.as_ptr(),
            line.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_NO_WINDOW
                | CREATE_SUSPENDED
                | CREATE_UNICODE_ENVIRONMENT
                | 0x00080000
                | breakaway,
            environment.as_ptr().cast(),
            cwd.as_ptr(),
            &startup.base,
            &mut info,
        )
    })?;
    let process = Handle(info.hProcess);
    let thread = Handle(info.hThread);
    let ready = (|| -> Result<()> {
        if breakaway != 0 {
            ensure!(
                !crate::platform::enclosing_job_contains(info.dwProcessId)?,
                "Servicing process stayed in the enclosing job"
            );
        }
        if let Some(permit) = permit {
            let time = now()?;
            ensure!(
                time >= permit.not_before && time < permit.expires_at,
                "Launch readiness expired during process creation"
            );
        }
        ensure!(
            !control.is_some_and(core::Control::cancelled),
            "Cancelled before process resume"
        );
        Ok(())
    })();
    if let Err(error) = ready {
        // Still suspended: neither the image entry point nor a servicing action
        // has run. Only this never-started allocation may be terminated.
        unsafe {
            TerminateProcess(process.0, 1);
            WaitForSingleObject(process.0, INFINITE);
        }
        return Err(error);
    }
    if unsafe { ResumeThread(thread.0) } == u32::MAX {
        // This process never ran. It cannot be servicing; dispose the suspended
        // allocation rather than leave an unsupervised suspended process behind.
        unsafe {
            TerminateProcess(process.0, 1);
            WaitForSingleObject(process.0, INFINITE);
        }
        bail!("Cannot resume supervised process");
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
    use std::{path::PathBuf, time::Instant};

    fn command(script: &str) -> (Command, Vec<File>) {
        let mut path = [0u16; 32768];
        let n = unsafe {
            windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW(
                path.as_mut_ptr(),
                path.len() as u32,
            )
        } as usize;
        assert!(n > 0 && n < path.len());
        let root = PathBuf::from(String::from_utf16(&path[..n]).unwrap());
        let exe = root.join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let pins = storage::pin_system_executable(&exe).unwrap();
        let mut command = Command::new(exe);
        command
            .env_clear()
            .env("SystemRoot", &root)
            .env("WINDIR", &root)
            .env(
                "PSModulePath",
                root.join("System32/WindowsPowerShell/v1.0/Modules"),
            )
            .current_dir(root.join("System32"))
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ]);
        (command, pins)
    }

    #[test]
    #[ignore = "native Windows job/pipe test; only synthetic sleeping processes, no maintenance"]
    fn direct_exit_does_not_release_descendant_supervision() {
        // Child announces startup, then lives beyond its direct parent.
        let (command, _pins) = command("$s=[Diagnostics.ProcessStartInfo]::new();$s.FileName=[Diagnostics.Process]::GetCurrentProcess().MainModule.FileName;$s.Arguments='-NoLogo -NoProfile -NonInteractive -Command [Console]::Out.WriteLine(123); Start-Sleep -Seconds 4';$s.UseShellExecute=$false;$s.RedirectStandardOutput=$true;$p=[Diagnostics.Process]::Start($s);$null=$p.StandardOutput.ReadLine()");
        let mut child = spawn(&command, false, None, None).unwrap();
        drop(child.stdin.take());
        assert_eq!(
            unsafe { WaitForSingleObject(child.as_raw_handle(), 15000) },
            WAIT_OBJECT_0
        );
        assert_eq!(child.try_wait().unwrap(), None);
        let until = Instant::now() + Duration::from_secs(15);
        while child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    #[ignore = "native Windows read-only job cancellation test; no maintenance"]
    fn only_read_only_jobs_terminate_on_drop() {
        for read_only in [false, true] {
            let (command, _pins) = command("Start-Sleep -Seconds 3");
            let child = spawn(&command, read_only, None, None).unwrap();
            let observer = unsafe { OpenProcess(0x0010_0000, 0, child.id()) }; // SYNCHRONIZE
            assert!(!observer.is_null());
            let observer = Handle(observer);
            drop(child);
            assert_eq!(
                unsafe { WaitForSingleObject(observer.0, if read_only { 5000 } else { 100 }) },
                if read_only {
                    WAIT_OBJECT_0
                } else {
                    WAIT_TIMEOUT
                }
            );
            assert_eq!(
                unsafe { WaitForSingleObject(observer.0, 15000) },
                WAIT_OBJECT_0
            );
        }
    }
}
