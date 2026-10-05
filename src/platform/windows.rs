use super::validate_value;
use crate::model::{Backend, Control, Finding, Observation};
use anyhow::{bail, ensure, Context, Result};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    ffi::{c_void, OsStr},
    io::{Read, Write},
    mem::{size_of, zeroed},
    os::windows::{ffi::OsStrExt, io::AsRawHandle, process::CommandExt},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    ptr::{null, null_mut},
    sync::mpsc,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::{SystemInformation::GetWindowsDirectoryW, Threading::*},
    UI::Shell::*,
};

#[path = "journal.rs"]
mod journal;
pub use journal::state_dir;

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn wide(s: impl AsRef<OsStr>) -> Result<Vec<u16>> {
    let mut v: Vec<_> = s.as_ref().encode_wide().collect();
    ensure!(!v.contains(&0), "Embedded NUL in Windows string");
    v.push(0);
    Ok(v)
}
fn winerr() -> anyhow::Error {
    std::io::Error::last_os_error().into()
}

fn windows_dir() -> Result<PathBuf> {
    let mut buf = vec![0u16; 32768];
    let n = unsafe { GetWindowsDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    ensure!(
        n > 0 && n < buf.len(),
        "Cannot resolve trusted Windows directory"
    );
    let p = PathBuf::from(String::from_utf16(&buf[..n])?);
    ensure!(p.is_absolute(), "Windows directory is not absolute");
    Ok(p)
}

pub fn is_elevated() -> Result<bool> {
    unsafe {
        let mut token = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(winerr());
        }
        let token = Handle(token);
        let mut elevation: TOKEN_ELEVATION = zeroed();
        let mut len = 0;
        if GetTokenInformation(
            token.0,
            TokenElevation,
            &mut elevation as *mut _ as *mut c_void,
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        ) == 0
        {
            return Err(winerr());
        }
        ensure!(
            len as usize == size_of::<TOKEN_ELEVATION>(),
            "Invalid elevation information"
        );
        Ok(elevation.TokenIsElevated != 0)
    }
}

fn quote_arg(s: &str) -> String {
    let mut out = String::from("\"");
    let mut slashes = 0;
    for ch in s.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        if ch == '"' {
            out.push_str(&"\\".repeat(slashes * 2 + 1));
        } else {
            out.push_str(&"\\".repeat(slashes));
        }
        slashes = 0;
        out.push(ch);
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('"');
    out
}
pub fn elevate(args: &[String]) -> Result<()> {
    let exe = wide(std::env::current_exe()?.as_os_str())?;
    let params = wide(
        args.iter()
            .map(|s| quote_arg(s))
            .collect::<Vec<_>>()
            .join(" "),
    )?;
    let cwd = wide(windows_dir()?.join("System32"))?;
    let verb = wide("runas")?;
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            verb.as_ptr(),
            exe.as_ptr(),
            params.as_ptr(),
            cwd.as_ptr(),
            1,
        )
    } as isize;
    ensure!(
        result > 32,
        "Elevation was cancelled or failed (ShellExecute code {result})"
    );
    Ok(())
}

// Small ABI declarations avoid changing Cargo feature ownership. These structures
// are the documented JOBOBJECT_EXTENDED_LIMIT_INFORMATION ABI (Windows x64).
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
struct IoCounters {
    counts: [u64; 6],
}
#[repr(C)]
struct ExtendedLimits {
    basic: BasicLimits,
    io: IoCounters,
    process_memory: usize,
    job_memory: usize,
    peak_process_memory: usize,
    peak_job_memory: usize,
}
#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(attributes: *const SECURITY_ATTRIBUTES, name: *const u16) -> HANDLE;
    fn SetInformationJobObject(job: HANDLE, class: i32, info: *const c_void, len: u32) -> i32;
    fn AssignProcessToJobObject(job: HANDLE, process: HANDLE) -> i32;
}
fn job() -> Result<Handle> {
    unsafe {
        let h = CreateJobObjectW(null(), null());
        ensure!(
            !h.is_null(),
            "Cannot create bounded process job: {}",
            winerr()
        );
        let h = Handle(h);
        let mut limits: ExtendedLimits = zeroed();
        limits.basic.flags = 0x2000 | 0x8; // KILL_ON_JOB_CLOSE | ACTIVE_PROCESS
        limits.basic.active_processes = 1;
        if SetInformationJobObject(
            h.0,
            9,
            &limits as *const _ as *const c_void,
            size_of::<ExtendedLimits>() as u32,
        ) == 0
        {
            return Err(winerr());
        }
        Ok(h)
    }
}
fn base64(input: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in input.chunks(3) {
        let v = ((c[0] as u32) << 16)
            | ((c.get(1).copied().unwrap_or(0) as u32) << 8)
            | c.get(2).copied().unwrap_or(0) as u32;
        out.push(TABLE[((v >> 18) & 63) as usize] as char);
        out.push(TABLE[((v >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 {
            TABLE[((v >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            TABLE[(v & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
fn run<T: DeserializeOwned>(action: &str, id: Option<&str>, value: Option<&Value>) -> Result<T> {
    ensure!(
        cfg!(target_arch = "x86_64"),
        "Secblitz supports Windows x64 only"
    );
    super::validate_request(action, id, value)?;
    if let Some(id) = id.filter(|id| crate::hardening::is_hardening(id)) {
        // Windows feature servicing (DISM) is slow; everything else is quick.
        let limit = if id == "ps.v2_engine" && action == "write" {
            900
        } else {
            90
        };
        return run_script(
            super::hardening_script(action, id, value)?,
            Duration::from_secs(limit),
        );
    }
    let script = format!(
        "$action='{action}'\n$id='{}'\n$inputJson={}\n{}",
        id.unwrap_or(""),
        value
            .map(|v| format!("'{}'", v))
            .unwrap_or_else(|| "$null".into()),
        include_str!("backend.ps1")
    );
    run_script(script, Duration::from_secs(90))
}

pub fn permission_gate(id: &str) -> Result<()> {
    let reply: Value = run("permission_gate", Some(id), None)?;
    super::validate_permission_reply(&reply)
}

pub fn support_action(id: &str) -> Result<()> {
    // Independently validated before interpolation and before launching anything.
    let script = super::support_script(id)?;
    ensure!(
        cfg!(target_arch = "x86_64"),
        "Secblitz supports Windows x64 only"
    );
    ensure!(
        is_elevated()?,
        "Defender support actions require Administrator elevation"
    );
    let timeout = match id {
        "defender_update" => Duration::from_secs(120),
        "defender_quickscan" => Duration::from_secs(15 * 60),
        _ => bail!("Unknown support action id"),
    };
    let reply: Value = run_script(script, timeout).context(
        "Defender support action failed; work may continue in Defender. Review Windows Security; no completion or rollback is assumed")?;
    ensure!(
        reply == serde_json::json!({"ok":true}),
        "Defender command return was not acknowledged; review Windows Security"
    );
    Ok(())
}

fn run_script<T: DeserializeOwned>(script: String, timeout: Duration) -> Result<T> {
    let win = windows_dir()?;
    let ps = win.join("System32/WindowsPowerShell/v1.0/powershell.exe");
    // EncodedCommand is UTF-16LE, not a shell command line. All inserted data is
    // validated booleans, fixed enum strings, or a strict numeric registry object.
    // Only this small fixed bootstrap is on the command line (Windows has a
    // 32K command-line limit). The embedded script arrives over a private pipe.
    // PowerShell 5.1 module initialization reports progress in the outer host
    // scope as CLIXML on stderr, even when the invoked script suppresses its
    // own progress. Set this before creating/invoking the script block. Keep
    // stderr rejection: real errors must not be filtered out as "progress".
    let bootstrap = "$global:ProgressPreference = 'SilentlyContinue'; [Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false); [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); & ([ScriptBlock]::Create([Console]::In.ReadToEnd()))";
    let bytes: Vec<u8> = bootstrap
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let job = job()?;
    let mut child = Command::new(ps)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &base64(&bytes),
        ])
        .env_clear()
        .env("SystemRoot", &win)
        .env("WINDIR", &win)
        .env("PATH", win.join("System32"))
        .env(
            "PSModulePath",
            win.join("System32/WindowsPowerShell/v1.0/Modules"),
        )
        .env("PSModuleAnalysisCachePath", "NUL")
        .current_dir(win.join("System32"))
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Start inbox Windows PowerShell")?;
    // The fixed script waits on stdin before any probes. Failure to assign never
    // releases that gate. Job closure kills PowerShell and disallows descendants.
    if unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle() as HANDLE) } == 0 {
        let e = winerr();
        let _ = child.kill();
        let _ = child.wait();
        return Err(e.context("Assign PowerShell job"));
    }
    let result = (|| -> Result<T> {
        let mut input = child.stdin.take().context("Missing stdin")?;
        // A stalled pipe writer must not prevent the outer timeout from firing.
        let writer = std::thread::spawn(move || input.write_all(script.as_bytes()));
        let (tx, rx) = mpsc::sync_channel(16);
        fn pump(
            mut r: impl Read + Send + 'static,
            tx: mpsc::SyncSender<(bool, std::io::Result<Vec<u8>>)>,
            err: bool,
        ) {
            std::thread::spawn(move || loop {
                let mut b = vec![0; 8192];
                let result = r.read(&mut b).map(|n| {
                    b.truncate(n);
                    b
                });
                let done = result.as_ref().map_or(true, Vec::is_empty);
                if tx.send((err, result)).is_err() || done {
                    break;
                }
            });
        }
        pump(
            child.stdout.take().context("Missing stdout")?,
            tx.clone(),
            false,
        );
        pump(child.stderr.take().context("Missing stderr")?, tx, true);
        let deadline = Instant::now() + timeout;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut eof = 0;
        while eof < 2 {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .context("PowerShell timed out; mutation outcome may be unknown")?;
            let (err, bytes) = rx
                .recv_timeout(remaining)
                .context("PowerShell output timeout/disconnect; mutation outcome may be unknown")?;
            let bytes = bytes?;
            if bytes.is_empty() {
                eof += 1;
                continue;
            }
            ensure!(
                stdout.len() + stderr.len() + bytes.len() <= 2 * 1024 * 1024,
                "PowerShell exceeded 2 MiB output limit; mutation outcome may be unknown"
            );
            if err {
                stderr.extend(bytes);
            } else {
                stdout.extend(bytes);
            }
        }
        let status = loop {
            if let Some(s) = child.try_wait()? {
                break s;
            }
            ensure!(
                Instant::now() < deadline,
                "PowerShell exit timed out; mutation outcome may be unknown"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        ensure!(
            status.success() && stderr.is_empty(),
            "PowerShell failed ({status}): {}",
            String::from_utf8_lossy(&stderr)
        );
        writer
            .join()
            .map_err(|_| anyhow::anyhow!("PowerShell input writer failed"))??;
        serde_json::from_slice(&stdout).context("Invalid PowerShell response (no result assumed)")
    })();
    drop(job);
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

struct WindowsBackend;
pub fn backend() -> Result<Box<dyn Backend>> {
    ensure!(
        cfg!(target_arch = "x86_64"),
        "Secblitz supports Windows x64 only"
    );
    // Fail early if the trusted inbox interpreter is absent. OS/client capability
    // gates run independently before every mutation, not just construction.
    ensure!(
        windows_dir()?
            .join("System32/WindowsPowerShell/v1.0/powershell.exe")
            .is_file(),
        "Inbox Windows PowerShell is unavailable"
    );
    Ok(Box::new(WindowsBackend))
}
impl Backend for WindowsBackend {
    fn machine_id(&mut self) -> Result<String> {
        let id: String = run("machine", None, None)?;
        uuid::Uuid::parse_str(&id).context("Invalid Windows machine identity")?;
        Ok(id)
    }
    fn controls(&self) -> Vec<Control> {
        super::controls()
    }
    fn observe(&mut self, id: &str) -> Result<Observation> {
        let obs: Observation = run("observe", Some(id), None)?;
        validate_value(id, &obs.value)?;
        crate::model::validate_observation(id, &obs)?;
        Ok(obs)
    }
    fn write(&mut self, id: &str, value: &Value) -> Result<()> {
        validate_value(id, value)?;
        ensure!(is_elevated()?, "Administrator elevation is required");
        let reply: Value = run("write", Some(id), Some(value))?;
        ensure!(
            reply == serde_json::json!({"ok":true}),
            "Write was not acknowledged"
        );
        let actual = self.observe(id)?;
        // Dynamic controls may see items that appeared since; compare only the
        // items that were written.
        let seen = match crate::hardening::spec(id) {
            Some(spec) => spec.view(&actual.value, value),
            None => actual.value.clone(),
        };
        ensure!(
            &seen == value,
            "Preference readback did not match; mutation outcome requires review"
        );
        Ok(())
    }
    fn findings(&mut self) -> Result<Vec<Finding>> {
        run("findings", None, None)
    }
    fn readiness(&mut self) -> crate::model::Readiness {
        crate::readiness::collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_arguments() {
        assert_eq!(quote_arg(""), "\"\"");
        assert_eq!(quote_arg("a b"), "\"a b\"");
        assert_eq!(quote_arg("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote_arg("C:\\"), "\"C:\\\\\"");
    }
    #[test]
    fn encoding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
    }
    #[test]
    fn native_boundary_rejects_unknown_requests_before_launch() {
        for id in [
            "",
            "defender_update ",
            "Defender_update",
            "defender.realtime",
            "defender_update'; exit",
            "defender_quickscan\0",
        ] {
            assert!(support_action(id)
                .unwrap_err()
                .to_string()
                .contains("Unknown support action"));
        }
        for action in [
            "defender_update",
            "defender_quickscan",
            "support",
            "write; exit",
        ] {
            assert!(run::<Value>(action, None, None)
                .unwrap_err()
                .to_string()
                .contains("Invalid platform action"));
        }
        for id in [
            "defender_update",
            "defender_quickscan",
            "defender.realtime; exit",
        ] {
            assert!(
                run::<Value>("write", Some(id), Some(&serde_json::json!(false)))
                    .unwrap_err()
                    .to_string()
                    .contains("Unknown control id")
            );
        }
    }
    #[test]
    fn inbox_powershell_pipe_encoding_and_output_bounds() {
        // Exercise the actual 5.1 interpreter, private stdin pipe, and job. This
        // is non-mutating and does not need Administrator privileges.
        let reply: String = run_script(
            "[Console]::Write('\"Grüße 世界\"')".into(),
            Duration::from_secs(15),
        )
        .unwrap();
        assert_eq!(reply, "Grüße 世界");
        assert!(run_script::<Value>(
            "[Console]::Error.Write('fixture error'); [Console]::Write('{}')".into(),
            Duration::from_secs(15)
        )
        .is_err());
        assert!(run_script::<Value>(
            "[Console]::Write(('x' * 2200000))".into(),
            Duration::from_secs(15)
        )
        .is_err());
        let start = Instant::now();
        let error = run_script::<Value>(
            "[Threading.Thread]::Sleep(60000)".into(),
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert!(
            format!("{error:#}").contains("timeout") || format!("{error:#}").contains("timed out")
        );
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "Timeout failed to bound the child process"
        );
    }
    #[test]
    fn inbox_powershell_actual_backend_read_only() {
        // Synthetic Console.Write tests don't initialize the inbox modules and
        // missed 5.1's outer-scope CLIXML progress. Exercise real production
        // script/dispatcher/module imports; no setter or journal is invoked.
        let mut backend = WindowsBackend;
        let machine = backend
            .machine_id()
            .expect("Actual machine probe must not emit progress on stderr");
        uuid::Uuid::parse_str(&machine).unwrap();
        let observation = backend
            .observe("uac.enabled")
            .expect("Actual observation transport");
        validate_value("uac.enabled", &observation.value).unwrap();
        assert!(!observation.reason.is_empty());
        let findings = backend
            .findings()
            .expect("Actual findings must be a single valid JSON array");
        assert_eq!(findings.len(), 14);
        let mut titles = std::collections::HashSet::new();
        for finding in findings {
            assert!(titles.insert(finding.title));
            assert!(matches!(
                finding.status.as_str(),
                "ok" | "info" | "attention" | "unknown"
            ));
            assert!(!finding.detail.is_empty());
        }
    }
    #[test]
    fn inbox_powershell_native_mdm_registration() {
        // Exercise the real .NET Framework P/Invoke emitter and inbox DLL,
        // without requiring the machine to be unmanaged or weakening Gate.
        // An Observation alone would hide an emitter/API error as ineligible.
        let source = include_str!("backend.ps1").replace('\'', "''");
        let script = format!(
            r#"
$source='{source}'
$ErrorActionPreference='Stop'
$tokens=$null; $errors=$null
$ast=[System.Management.Automation.Language.Parser]::ParseInput($source,[ref]$tokens,[ref]$errors)
if ($errors.Count) {{ throw 'Backend parse failed' }}
foreach ($node in $ast.EndBlock.Statements) {{
    if ($node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -in @('QueryMdmRegistration','MdmRegistered')) {{
        . ([scriptblock]::Create($node.Extent.Text))
    }}
}}
if (MdmRegistered) {{ [Console]::Write('true') }} else {{ [Console]::Write('false') }}
"#
        );
        let _: bool = run_script(script, Duration::from_secs(30))
            .expect("Documented MDM registration API probe");
    }
}
