use super::commands::{command_spec, dism_evidence};
use super::{
    core::{Control, Event, Execution, Facts, LaunchPermit},
    *,
};
use base64::Engine as _;
use std::{
    ffi::c_void,
    fs::File,
    io::{Read, Write},
    mem::{size_of, zeroed},
    os::windows::io::AsRawHandle,
    path::{Path, PathBuf},
    process::Command,
    ptr::null_mut,
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{Registry::*, SystemInformation::*, Threading::*},
    UI::WindowsAndMessaging::GetShellWindow,
};

#[link(name = "kernel32")]
extern "system" {
    fn ProcessIdToSessionId(pid: u32, session: *mut u32) -> i32;
    fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> HANDLE;
    fn Process32FirstW(snapshot: HANDLE, entry: *mut ProcessEntry) -> i32;
    fn Process32NextW(snapshot: HANDLE, entry: *mut ProcessEntry) -> i32;
    fn PeekNamedPipe(
        pipe: HANDLE,
        buffer: *mut c_void,
        size: u32,
        read: *mut u32,
        available: *mut u32,
        left: *mut u32,
    ) -> i32;
}
#[link(name = "user32")]
extern "system" {
    fn GetLastInputInfo(info: *mut LastInput) -> i32;
}
#[link(name = "wtsapi32")]
extern "system" {
    fn WTSEnumerateSessionsW(
        server: HANDLE,
        reserved: u32,
        version: u32,
        sessions: *mut *mut Session,
        count: *mut u32,
    ) -> i32;
    fn WTSFreeMemory(buffer: *mut c_void);
}
#[repr(C)]
struct Session {
    id: u32,
    name: *mut u16,
    state: i32,
}
#[repr(C)]
struct LastInput {
    size: u32,
    tick: u32,
}
#[repr(C)]
struct ProcessEntry {
    size: u32,
    usage: u32,
    pid: u32,
    heap: usize,
    module: u32,
    threads: u32,
    parent: u32,
    priority: i32,
    flags: u32,
    exe: [u16; 260],
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub(super) struct Backend {
    win: PathBuf,
    root: PathBuf,
    machine: String,
}
impl Backend {
    pub fn new(root: &Path) -> Result<Self> {
        ensure!(cfg!(target_arch = "x86_64"), "Native Windows x64 required");
        let mut buffer = vec![0u16; 32768];
        let n = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
        ensure!(n > 0 && n < buffer.len(), "Windows directory unavailable");
        let win = PathBuf::from(String::from_utf16(&buffer[..n])?);
        let mut raw = [0u16; 256];
        let mut bytes = size_of_val(&raw) as u32;
        let result = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                storage::wide("SOFTWARE\\Microsoft\\Cryptography").as_ptr(),
                storage::wide("MachineGuid").as_ptr(),
                RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
                null_mut(),
                raw.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        ensure!(
            result == 0 && bytes >= 4 && bytes as usize <= size_of_val(&raw),
            "Machine binding unavailable"
        );
        let end = raw
            .iter()
            .position(|c| *c == 0)
            .context("Invalid machine binding")?;
        let id = String::from_utf16(&raw[..end])?;
        let parsed = Uuid::parse_str(&id).context("Invalid machine binding")?;
        let machine = digest(&("secblitz.operations.machine.v1", parsed))?;
        Ok(Self {
            win,
            root: root.to_owned(),
            machine,
        })
    }
    fn command(&self, relative: &str) -> Result<(Command, Vec<File>)> {
        // relative is always a compiled literal, never a record or user path.
        let exe = self.win.join(relative);
        let pins = storage::pin_system_executable(&exe)?;
        let mut command = Command::new(exe);
        let system_drive = self
            .win
            .components()
            .next()
            .context("Missing system drive")?
            .as_os_str();
        let program_data = self
            .root
            .parent()
            .and_then(Path::parent)
            .context("Missing protected ProgramData parent")?;
        command
            .env_clear()
            .env("SystemRoot", &self.win)
            .env("WINDIR", &self.win)
            .env("SystemDrive", system_drive)
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
            .current_dir(self.win.join("System32"));
        Ok((command, pins))
    }
    fn script(
        &self,
        defender: bool,
        action: &str,
        permit: Option<&LaunchPermit>,
        control: Option<&Control>,
        notify: &mut dyn FnMut(Event),
    ) -> Result<Output> {
        ensure!(
            matches!(action, "probe" | "scan" | "verify"),
            "Invalid compiled script action"
        );
        let (mut command, mut pins) =
            self.command("System32/WindowsPowerShell/v1.0/powershell.exe")?;
        for module in [
            "Microsoft.PowerShell.Management",
            "Microsoft.PowerShell.Utility",
            "CimCmdlets",
        ]
        .into_iter()
        .chain(defender.then_some("Defender"))
        {
            pins.extend(storage::pin_system_module(&self.win.join(format!(
                "System32/WindowsPowerShell/v1.0/Modules/{module}/{module}.psd1"
            )))?);
        }
        pins.extend(storage::pin_system_executable(
            &self.win.join("System32/MDMRegistration.dll"),
        )?);
        let script = format!(
            "$maintenanceNotBefore=[uint64]{}\n$maintenanceExpiresAt=[uint64]{}\n{}",
            permit.map_or(0, |p| p.not_before),
            permit.map_or(0, |p| p.expires_at),
            commands::script(defender, action)?
        );
        let bootstrap = "$global:ProgressPreference='SilentlyContinue'; [Console]::InputEncoding=[Text.UTF8Encoding]::new($false); [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); & ([ScriptBlock]::Create([Console]::In.ReadToEnd()))";
        let encoded = base64::engine::general_purpose::STANDARD.encode(
            bootstrap
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
            &encoded,
        ]);
        let output = run(
            command,
            Some(script),
            if action == "scan" { 1800 } else { 90 },
            permit,
            control,
            notify,
            action != "scan",
        )?;
        drop(pins);
        ensure!(
            output.code == 0 && output.stderr.is_empty() && !output.overflow,
            "Maintenance script failed; no result assumed"
        );
        if action != "scan" {
            ensure!(!output.stopped, "Maintenance probe timed out or cancelled");
        }
        Ok(output)
    }
    fn probe(&self, defender: bool) -> Result<ProbeResult> {
        let result = self.script(defender, "probe", None, None, &mut |_| {})?;
        serde_json::from_slice(&result.stdout).context("Invalid maintenance probe result")
    }
    fn servicing(
        &self,
        kind: OperationKind,
        permit: Option<&LaunchPermit>,
        control: &Control,
        notify: &mut dyn FnMut(Event),
    ) -> Result<Execution> {
        let (relative, args) = command_spec(kind)?;
        let (mut command, pins) = self.command(relative)?;
        command.args(args);
        let output = run(
            command,
            None,
            kind.spec().timeout_seconds,
            permit,
            Some(control),
            notify,
            false,
        )?;
        drop(pins);
        let evidence = if output.code == 0 && !output.overflow && output.stderr.is_empty() {
            match kind {
                OperationKind::DismCheckHealth | OperationKind::DismScanHealth => {
                    dism_evidence(&output.stdout)
                }
                OperationKind::SfcVerify => Evidence::DiagnosticCompleted,
                _ => Evidence::Inconclusive,
            }
        } else {
            Evidence::Inconclusive
        };
        Ok(Execution {
            code: output.code,
            evidence,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeResult {
    boot_time: u64,
    reboot_pending: bool,
    quick_start: Option<u64>,
    quick_end: Option<u64>,
    unmetered: Option<bool>,
}

impl core::Backend for Backend {
    fn machine(&self) -> Result<String> {
        Ok(self.machine.clone())
    }
    fn facts(&mut self, kind: OperationKind, process: Option<&ProcessIdentity>) -> Result<Facts> {
        // Freshness starts BEFORE the slowest probe, not when it finishes.
        let captured_at = now()?;
        let probe = self.probe(kind == OperationKind::DefenderQuickScan)?;
        let readiness = crate::readiness::collect();
        use crate::model::Probe;
        let storage_ready = matches!(readiness.system_volume, Probe::Known(ref v) if !v.read_only && v.available_bytes >= 5 * 1024 * 1024 * 1024)
            && matches!(readiness.journal_volume, Probe::Known(ref v) if !v.read_only && v.available_bytes >= 64 * 1024 * 1024);
        ensure!(
            probe.boot_time > 0 && probe.boot_time <= captured_at,
            "Invalid boot identity"
        );
        let idle_seconds = idle_seconds();
        let busy = busy_processes()?
            || match process {
                Some(p) => process_alive(p)?,
                None => false,
            }
            || (kind == OperationKind::DefenderQuickScan
                && match (probe.quick_start, probe.quick_end) {
                    (Some(start), Some(end)) => start > end,
                    _ => true,
                });
        Ok(Facts {
            captured_at,
            elevated: crate::platform::is_elevated()?,
            unmanaged: true,
            ac: matches!(readiness.power, Probe::Known(ref p) if p.ac_connected == Some(true)),
            storage_ready,
            reboot_pending: probe.reboot_pending
                || readiness.windows_update_reboot != Probe::Known(false),
            busy,
            idle_seconds,
            unmetered: probe.unmetered,
            boot_time: probe.boot_time,
            defender_scan_end: probe.quick_end,
        })
    }
    fn execute(
        &mut self,
        kind: OperationKind,
        permit: &LaunchPermit,
        control: &Control,
        notify: &mut dyn FnMut(Event),
    ) -> Result<Execution> {
        if kind == OperationKind::DefenderQuickScan {
            let result = self.script(true, "scan", Some(permit), Some(control), notify)?;
            ensure!(
                serde_json::from_slice::<serde_json::Value>(&result.stdout)?
                    == serde_json::json!({"acknowledged":true}),
                "Scan command not acknowledged"
            );
            Ok(Execution {
                code: result.code,
                evidence: Evidence::Inconclusive,
            })
        } else {
            self.servicing(kind, Some(permit), control, notify)
        }
    }
    fn verify(
        &mut self,
        kind: OperationKind,
        baseline: &Baseline,
        permit: &LaunchPermit,
        control: &Control,
        notify: &mut dyn FnMut(Event),
    ) -> Result<Evidence> {
        match kind {
            OperationKind::DefenderQuickScan => {
                let result = self.script(true, "verify", Some(permit), Some(control), notify)?;
                let probe: ProbeResult = serde_json::from_slice(&result.stdout)?;
                Ok(
                    if matches!((probe.quick_start, probe.quick_end, baseline.defender_scan_end), (Some(start), Some(end), Some(before)) if start >= baseline.captured_at && end >= start && end > before && end <= now()?)
                    {
                        Evidence::DefenderScanCompleted
                    } else {
                        Evidence::Inconclusive
                    },
                )
            }
            OperationKind::DismCheckHealth => Ok(self
                .servicing(
                    OperationKind::DismCheckHealth,
                    Some(permit),
                    control,
                    notify,
                )?
                .evidence),
            OperationKind::DismScanHealth | OperationKind::DismRestoreHealth => Ok(self
                .servicing(OperationKind::DismScanHealth, Some(permit), control, notify)?
                .evidence),
            OperationKind::SfcVerify | OperationKind::SfcRepair => {
                let result =
                    self.servicing(OperationKind::SfcVerify, Some(permit), control, notify)?;
                // Completion proves only that the independent diagnostic ran.
                // SfcRepair remains NeedsReview, never Succeeded from exit zero.
                Ok(result.evidence)
            }
        }
    }
}

fn identity(handle: HANDLE, pid: u32) -> Result<ProcessIdentity> {
    let (mut created, mut exit, mut kernel, mut user): (FILETIME, FILETIME, FILETIME, FILETIME) =
        unsafe { zeroed() };
    ensure!(
        unsafe { GetProcessTimes(handle, &mut created, &mut exit, &mut kernel, &mut user) } != 0,
        "Cannot identify maintenance process"
    );
    Ok(ProcessIdentity {
        pid,
        creation_time: ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64,
    })
}
fn process_alive(expected: &ProcessIdentity) -> Result<bool> {
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | 0x0010_0000,
            0,
            expected.pid,
        )
    }; // SYNCHRONIZE
    if handle.is_null() {
        ensure!(
            unsafe { GetLastError() } == ERROR_INVALID_PARAMETER,
            "Cannot establish previous process exit"
        );
        return Ok(false);
    }
    let handle = Handle(handle);
    if identity(handle.0, expected.pid)? != *expected {
        return Ok(false);
    }
    let status = unsafe { WaitForSingleObject(handle.0, 0) };
    ensure!(
        matches!(status, WAIT_OBJECT_0 | WAIT_TIMEOUT),
        "Cannot query previous process state"
    );
    Ok(status == WAIT_TIMEOUT)
}
fn busy_processes() -> Result<bool> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(2, 0) };
    ensure!(
        snapshot != INVALID_HANDLE_VALUE,
        "Cannot enumerate servicing processes"
    );
    let snapshot = Handle(snapshot);
    let mut entry: ProcessEntry = unsafe { zeroed() };
    entry.size = size_of::<ProcessEntry>() as u32;
    ensure!(
        unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0,
        "Cannot inspect servicing processes"
    );
    let mut count = 0;
    loop {
        count += 1;
        ensure!(count <= 65536, "Process enumeration cap exceeded");
        let end = entry
            .exe
            .iter()
            .position(|c| *c == 0)
            .context("Invalid process name")?;
        let name = String::from_utf16(&entry.exe[..end])?.to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "dism.exe"
                | "dismhost.exe"
                | "sfc.exe"
                | "tiworker.exe"
                | "mpcmdrun.exe"
                | "usoclient.exe"
                | "mousocoreworker.exe"
        ) {
            return Ok(true);
        }
        if unsafe { Process32NextW(snapshot.0, &mut entry) } == 0 {
            ensure!(
                unsafe { GetLastError() } == ERROR_NO_MORE_FILES,
                "Servicing process enumeration failed"
            );
            return Ok(false);
        }
    }
}

pub(super) fn ensure_no_servicing_processes() -> Result<()> {
    ensure!(
        !busy_processes()?,
        "Deferred: a Windows servicing process is active"
    );
    Ok(())
}

fn idle_seconds() -> Option<u32> {
    // GetLastInputInfo is session-local. Never infer desktop idle from Session 0
    // or ignore an active RDP/second desktop session. Such callers need a scoped
    // owner exception; the service cannot silently manufacture idle evidence.
    let mut current = 0;
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut current) } == 0
        || current == 0
        || unsafe { GetShellWindow() }.is_null()
    {
        return None;
    }
    let (mut sessions, mut count) = (null_mut(), 0);
    if unsafe { WTSEnumerateSessionsW(null_mut(), 0, 1, &mut sessions, &mut count) } == 0 {
        return None;
    }
    struct Sessions(*mut Session);
    impl Drop for Sessions {
        fn drop(&mut self) {
            unsafe {
                WTSFreeMemory(self.0.cast());
            }
        }
    }
    let sessions = Sessions(sessions);
    if count == 0 || count > 1024 || sessions.0.is_null() {
        return None;
    }
    let active: Vec<_> = unsafe { std::slice::from_raw_parts(sessions.0, count as usize) }
        .iter()
        .filter(|s| s.state == 0)
        .map(|s| s.id)
        .collect();
    if active != [current] {
        return None;
    }
    let mut input = LastInput {
        size: size_of::<LastInput>() as u32,
        tick: 0,
    };
    if unsafe { GetLastInputInfo(&mut input) } == 0 {
        return None;
    }
    Some(unsafe { GetTickCount() }.wrapping_sub(input.tick) / 1000)
}

struct Output {
    code: u32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    overflow: bool,
    stopped: bool,
}

fn drain(
    reader: &mut (impl Read + AsRawHandle),
    output: &mut Vec<u8>,
    overflow: &mut bool,
) -> Result<()> {
    // Peek before every bounded read; no blocking read, unbounded allocation or
    // reader thread holding engine.lock after the client wait deadline.
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
                "Maintenance output pipe failed"
            );
            return Ok(());
        }
        if available == 0 {
            return Ok(());
        }
        let mut buffer = [0u8; 8192];
        let n = reader.read(&mut buffer[..(available as usize).min(8192)])?;
        let keep = n.min((256 * 1024usize).saturating_sub(output.len()));
        *overflow |= keep < n;
        output.extend_from_slice(&buffer[..keep]);
    }
    Ok(())
}

fn run(
    command: Command,
    script: Option<String>,
    budget: u64,
    permit: Option<&LaunchPermit>,
    control: Option<&Control>,
    notify: &mut dyn FnMut(Event),
    read_only: bool,
) -> Result<Output> {
    if control.is_some_and(Control::cancelled) {
        bail!("Cancelled before process launch");
    }
    if let Some(permit) = permit {
        let time = now()?;
        ensure!(
            time >= permit.not_before && time < permit.expires_at,
            "Launch approval/readiness expired"
        );
    }
    let started = Instant::now();
    let mut child = process::spawn(&command, read_only, permit, control)
        .context("Start trusted Windows maintenance executable")?;
    // From this boundary onward, servicing must retain supervision until the
    // whole job exits. A journal failure only requests cooperative cancellation.
    // Only read-only query jobs may be terminated at their bounded deadline.
    let mut problem = None;
    match identity(child.as_raw_handle(), child.id()) {
        Ok(process) => notify(Event::Spawned(process)),
        Err(error) => problem = Some(error),
    }
    let writer = script.and_then(|script| {
        child.stdin.take().and_then(|mut input| {
            let send = problem.is_none() && !control.is_some_and(Control::cancelled);
            match std::thread::Builder::new()
                .name("maintenance-script-input".into())
                .spawn(move || {
                    if send {
                        input.write_all(script.as_bytes())
                    } else {
                        Ok(())
                    }
                }) {
                Ok(writer) => Some(writer),
                Err(error) => {
                    problem.get_or_insert(error.into());
                    None
                }
            }
        })
    });
    // No script: close the input pipe so native commands observe EOF.
    drop(child.stdin.take());
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let (mut overflow, mut stopped) = (false, false);
    let mut last_tick = u64::MAX;
    let code = loop {
        let elapsed = started.elapsed().as_secs();
        if !stopped {
            let reason = if control.is_some_and(Control::cancelled) {
                Some(StopReason::CancelRequested)
            } else if elapsed >= budget {
                Some(StopReason::Timeout)
            } else {
                None
            };
            if let Some(reason) = reason {
                stopped = true;
                notify(Event::Stop(reason));
            }
        }
        if read_only && stopped {
            // Only read-only PowerShell queries use a kill-on-close job. DISM,
            // SFC and Start-MpScan always remain supervised without termination.
            bail!("Read-only maintenance query timed out or was cancelled");
        }
        if last_tick != elapsed {
            notify(Event::Tick(elapsed));
            last_tick = elapsed;
        }
        if let Some(reader) = stdout.as_mut() {
            if let Err(error) = drain(reader, &mut out, &mut overflow) {
                problem.get_or_insert(error);
            }
        }
        if let Some(reader) = stderr.as_mut() {
            if let Err(error) = drain(reader, &mut err, &mut overflow) {
                problem.get_or_insert(error);
            }
        }
        match child.try_wait() {
            Ok(Some(exit)) => {
                // Child has exited; finish only the bounded amount already in
                // its pipes. Descendants cannot block reads or hold up a join.
                if let Some(reader) = stdout.as_mut() {
                    if let Err(error) = drain(reader, &mut out, &mut overflow) {
                        problem.get_or_insert(error);
                    }
                }
                if let Some(reader) = stderr.as_mut() {
                    if let Err(error) = drain(reader, &mut err, &mut overflow) {
                        problem.get_or_insert(error);
                    }
                }
                break exit;
            }
            Ok(None) => {}
            Err(error) => {
                problem.get_or_insert(error);
                // Keep the process handle and lock until the OS confirms exit.
                // Direct process exit is not proof that its job is empty.
                // Retain the shared lock and pins until a later query succeeds.
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if let Some(writer) = writer {
        // The script consumes stdin before execution; once child exits the
        // writer gets EOF/broken-pipe. Never join an unbounded inherited pipe.
        if writer.is_finished() {
            match writer.join() {
                Ok(Ok(())) => {}
                _ => {
                    problem.get_or_insert_with(|| {
                        anyhow::anyhow!("Maintenance script delivery failed")
                    });
                }
            }
        } else {
            problem
                .get_or_insert_with(|| anyhow::anyhow!("Maintenance script writer did not finish"));
        }
    }
    if let Some(error) = problem {
        return Err(error);
    }
    Ok(Output {
        code,
        stdout: out,
        stderr: err,
        overflow,
        stopped,
    })
}
