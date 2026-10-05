//! Windows PowerShell 5.1 runner for the embedded debloat scripts.
//!
//! Scripts arrive over stdin (never on the command line), run hidden with a
//! clean environment, a deadline and bounded output. Untrusted values only
//! travel in an environment variable that the caller validated first.
use anyhow::{bail, ensure, Context, Result};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::windows::ffi::OsStringExt;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_OUTPUT: usize = 8 * 1024 * 1024;

pub const INVENTORY: &str = include_str!("scripts/inventory.ps1");
pub const REMOVE: &str = include_str!("scripts/remove.ps1");
pub const POLICY: &str = include_str!("scripts/policy.ps1");
pub const DESCRIBE: &str = include_str!("scripts/describe.ps1");
pub const REGISTER: &str = include_str!("scripts/register.ps1");

/// Runs before every script: only inbox modules, imported by absolute path,
/// and no autoloading, so a module planted in the user's Documents folder can
/// never answer for `Get-AppxPackage` in this elevated process.
const PRELUDE: &str = r#"$moduleRoot = [IO.Path]::Combine($env:SystemRoot, 'System32\WindowsPowerShell\v1.0\Modules')
$env:PSModulePath = $moduleRoot
$PSModuleAutoLoadingPreference = 'None'
foreach ($m in 'Microsoft.PowerShell.Management', 'Microsoft.PowerShell.Utility', 'Appx', 'Dism') {
    $null = Import-Module ([IO.Path]::Combine($moduleRoot, "$m\$m.psd1")) -ErrorAction Stop
}
"#;

fn windows_dir() -> Result<PathBuf> {
    let mut buffer = vec![0u16; 32768];
    let count = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    ensure!(
        count > 0 && count < buffer.len(),
        "Windows folder not found"
    );
    let path = PathBuf::from(OsString::from_wide(&buffer[..count]));
    ensure!(path.is_absolute(), "Windows folder is not absolute");
    Ok(path)
}

fn base64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Run `script` and return its last non-empty stdout line.
pub fn run(script: &'static str, env: &[(&str, &str)], timeout: Duration) -> Result<String> {
    let win = windows_dir()?;
    let ps = win.join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let bootstrap = "$global:ProgressPreference = 'SilentlyContinue'; [Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false); [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); & ([ScriptBlock]::Create([Console]::In.ReadToEnd()))";
    let encoded: Vec<u8> = bootstrap
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let mut command = Command::new(ps);
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &base64(&encoded),
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
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command.spawn().context("Start Windows PowerShell")?;
    let mut input = child.stdin.take().context("Missing stdin")?;
    let mut output = child.stdout.take().context("Missing stdout")?;
    std::thread::spawn(move || {
        let _ = input.write_all(PRELUDE.as_bytes());
        let _ = input.write_all(script.as_bytes());
    });
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = output
            .by_ref()
            .take(MAX_OUTPUT as u64 + 1)
            .read_to_end(&mut bytes);
        let _ = tx.send(result.map(|_| bytes));
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(bytes)) => {
            let status = child.wait().context("Wait for Windows PowerShell")?;
            ensure!(bytes.len() <= MAX_OUTPUT, "PowerShell output too large");
            let text = String::from_utf8_lossy(&bytes);
            let last = text
                .lines()
                .map(str::trim)
                .rfind(|l| !l.is_empty())
                .unwrap_or_default()
                .to_string();
            ensure!(
                status.success() || !last.is_empty(),
                "PowerShell failed ({status})"
            );
            Ok(last)
        }
        Ok(Err(e)) => {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Could not read PowerShell output: {e}")
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Windows took too long to answer")
        }
    }
}
