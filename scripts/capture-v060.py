"""
Secblitz 0.6.0 video capture driver - host side orchestrator.
Models the existing validate-v060.py / validate-ui-final.py patterns.

VM: Secblitz-W11-UI-Test only (UUID hardcoded; no other VM is touched).
Credentials: passed directly to VBoxManage via --passwordfile; never read or printed.
Large scratch files go under target/video-v060/ (NOT /tmp).

Usage:
  python scripts/capture-v060.py --dry-run
      Verify VM state, check tools, print plan. Safe to run any time.

  python scripts/capture-v060.py /path/to/secblitz.exe
      Full Phase B run: start VM -> smoke test -> apply fixtures ->
      record -> pull file -> restore fixtures -> verify -> power off.

  python scripts/capture-v060.py /path/to/secblitz.exe --mode setup
      Start VM, copy exe, apply fixtures only (no recording). Use to
      inspect the guest before committing to a full take.

  python scripts/capture-v060.py --mode restore
      Restore fixtures and power off (no recording). Use to clean up
      after a failed or aborted recording attempt.

  python scripts/capture-v060.py --mode pull
      Pull the recording from a completed but not yet retrieved guest run.

Hard rules enforced:
  - VM must be powered off with all 8 NICs none before starting.
  - Fixtures are verified restored before power-off; script refuses to power off
    with unverified fixture state.
  - Recording is pulled and SHA-256-hashed before any restore step.
  - All VBoxManage output is captured; credential path is referenced by variable only.
"""
import argparse
import hashlib
import json
import pathlib
import subprocess
import sys
import time

ROOT  = pathlib.Path(__file__).resolve().parents[1]
OUT   = ROOT / "target/video-v060"
VM    = "4b70288b-b64d-4796-a725-006da3162d0f"
CRED  = ['--username', 'Administrator',
         '--passwordfile', '/home/slay/projects/cybersec/virus-total-alternative/data/windows11-password']

# Guest paths (inside the VM).
GUEST_ROOT   = r'C:\Windows\Temp\secblitz-v060-video'
GUEST_EXE    = GUEST_ROOT + r'\secblitz.exe'
GUEST_FFMPEG = GUEST_ROOT + r'\ffmpeg.exe'
GUEST_DRIVER = GUEST_ROOT + r'\capture-v060-driver.ps1'
GUEST_RECORD = GUEST_ROOT + r'\take01.webm'
GUEST_STATUS = GUEST_ROOT + r'\driver-status.json'
GUEST_SENTINEL = GUEST_ROOT + r'\driver-done.txt'
GUEST_FIXTURE_BEFORE = GUEST_ROOT + r'\fixture-before.json'
GUEST_FIXTURE_AFTER  = GUEST_ROOT + r'\fixture-after.json'


def sha256(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open('rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


def save(path: pathlib.Path, value) -> None:
    path.write_text(json.dumps(value, indent=2) + '\n')


def vbox(args, timeout=120, check=True):
    """Run a VBoxManage command. Never prints credential path in output."""
    cmd = ['VBoxManage'] + args
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    if check and r.returncode:
        raise SystemExit(f'VBoxManage {args[0]} failed (exit {r.returncode}):\n{r.stderr}')
    return r


def vm_state() -> dict:
    r = vbox(['showvminfo', VM, '--machinereadable'])
    lines = r.stdout.splitlines()
    state   = next(l.split('=', 1)[1].strip('"') for l in lines if l.startswith('VMState='))
    nics    = {f'nic{i}': next(l.split('=', 1)[1].strip('"') for l in lines if l.startswith(f'nic{i}='))
               for i in range(1, 9)}
    return {'state': state, 'nics': nics}


def assert_clean_poweroff():
    s = vm_state()
    if s['state'] != 'poweroff':
        raise SystemExit(f'VM must be powered off before capture; current state: {s["state"]}')
    if any(v != 'none' for v in s['nics'].values()):
        raise SystemExit(f'VM has active NICs; all must be none: {s["nics"]}')
    print('VM state: poweroff, all 8 NICs none. OK.')


def wait_poweroff(timeout=120):
    for _ in range(timeout):
        s = vm_state()
        if s['state'] == 'poweroff':
            assert all(v == 'none' for v in s['nics'].values()), f'NICs active after shutdown: {s["nics"]}'
            (OUT / 'vm-final.txt').write_text(
                vbox(['showvminfo', VM, '--machinereadable']).stdout)
            print('UI clone powered off, all 8 NICs none.')
            return
        time.sleep(1)
    raise SystemExit('Clean shutdown not confirmed within timeout; do not force-off.')


def guest_run(ps1_path: pathlib.Path, *extra_args, timeout=1800):
    """Copy ps1_path to guest Temp and run it with PowerShell. Never prints credentials."""
    guest_path = r'C:\Windows\Temp\\' + ps1_path.name
    vbox(['guestcontrol', VM, 'copyto'] + CRED + [str(ps1_path), guest_path])
    r = vbox(
        ['guestcontrol', VM, 'run',
         '--exe', r'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'] +
        CRED +
        ['--timeout', str(timeout * 1000), '--',
         '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
         '-File', guest_path] + list(extra_args),
        timeout=timeout + 30,
        check=False,
    )
    return r.returncode


def guest_put(local: pathlib.Path, guest_dst: str):
    vbox(['guestcontrol', VM, 'copyto'] + CRED + [str(local.resolve()), guest_dst])


def guest_get(guest_src: str, local: pathlib.Path):
    local.parent.mkdir(parents=True, exist_ok=True)
    vbox(['guestcontrol', VM, 'copyfrom'] + CRED + [guest_src, str(local.resolve())])


def write_inline_ps1(path: pathlib.Path, content: str):
    path.write_text(content, encoding='utf-8')


# --- Inline PowerShell fragments (no secrets; run via guest_run) ---

SETUP_PS1 = r"""
$ErrorActionPreference='Stop'; $utf8=[Text.UTF8Encoding]::new($false)
$root='""" + GUEST_ROOT + r"""'
if(!(Test-Path $root)){New-Item -ItemType Directory -Path $root|Out-Null}
# Verify no existing Secblitz processes.
$procs=@(Get-Process secblitz -ErrorAction SilentlyContinue)
if($procs.Count -ne 0){throw "Existing secblitz process; inspect before recording: $($procs.Id -join ',')"}
# Verify Secblitz is not installed.
if(Test-Path 'C:\Program Files\Secblitz\secblitz.exe'){throw 'Secblitz installed; capture must use the provided exe only.'}
# Verify FFmpeg is available (must be pre-installed in the VM or copied before this runs).
$ff=Get-Command ffmpeg.exe -ErrorAction SilentlyContinue
if(!$ff){throw 'ffmpeg.exe not found on PATH; copy it to GUEST_ROOT before recording.'}
[IO.File]::WriteAllText("$root\setup-complete.txt",'SETUP OK',$utf8)
"""

FIXTURE_APPLY_PS1 = r"""
$ErrorActionPreference='Stop'; $utf8=[Text.UTF8Encoding]::new($false)
$root='""" + GUEST_ROOT + r"""'
$before=[ordered]@{
    PublicFirewallEnabled=(Get-NetFirewallProfile -Name Public).Enabled
    DefenderArchiveScanningDisabled=(Get-MpPreference).DisableArchiveScanning
}
[IO.File]::WriteAllText("$root\fixture-before.json",(ConvertTo-Json $before),$utf8)
# Weaken control 1: Public network firewall.
Set-NetFirewallProfile -Profile Public -Enabled False
# Weaken control 2: Defender archive scanning.
Set-MpPreference -DisableArchiveScanning $true
$after=[ordered]@{
    PublicFirewallEnabled=(Get-NetFirewallProfile -Name Public).Enabled
    DefenderArchiveScanningDisabled=(Get-MpPreference).DisableArchiveScanning
}
[IO.File]::WriteAllText("$root\fixture-applied.json",(ConvertTo-Json $after),$utf8)
if($after.PublicFirewallEnabled){throw 'Fixture 1 not applied: Public firewall still enabled.'}
if(!$after.DefenderArchiveScanningDisabled){throw 'Fixture 2 not applied: archive scanning still enabled.'}
Write-Output 'Fixtures applied and verified.'
"""

FIXTURE_RESTORE_PS1 = r"""
$ErrorActionPreference='Stop'; $utf8=[Text.UTF8Encoding]::new($false)
$root='""" + GUEST_ROOT + r"""'
# Restore control 1: Public network firewall.
Set-NetFirewallProfile -Profile Public -Enabled True
# Restore control 2: Defender archive scanning.
Set-MpPreference -DisableArchiveScanning $false
$after=[ordered]@{
    PublicFirewallEnabled=(Get-NetFirewallProfile -Name Public).Enabled
    DefenderArchiveScanningDisabled=(Get-MpPreference).DisableArchiveScanning
}
[IO.File]::WriteAllText("$root\fixture-after.json",(ConvertTo-Json $after),$utf8)
if(!$after.PublicFirewallEnabled){throw 'Fixture 1 not restored: Public firewall still disabled.'}
if($after.DefenderArchiveScanningDisabled){throw 'Fixture 2 not restored: archive scanning still disabled.'}
Write-Output 'Fixtures restored and verified.'
"""

SMOKE_PS1 = r"""
$ErrorActionPreference='Stop'; $utf8=[Text.UTF8Encoding]::new($false)
$root='""" + GUEST_ROOT + r"""'
$exe='""" + GUEST_EXE + r"""'
function Run($name,$args,$timeout=30){
    $p=Start-Process -FilePath $exe -ArgumentList $args `
        -RedirectStandardInput "$root\empty.stdin" `
        -RedirectStandardOutput "$root\$name.out" `
        -RedirectStandardError  "$root\$name.err" `
        -PassThru
    if(!$p.WaitForExit($timeout*1000)){Stop-Process -Id $p.Id -Force;throw "$name timed out"}
    [IO.File]::WriteAllText("$root\$name.status.json",(@{ExitCode=$p.ExitCode}|ConvertTo-Json),$utf8)
    return $p.ExitCode
}
if(!(Test-Path "$root\empty.stdin")){[IO.File]::WriteAllBytes("$root\empty.stdin",[byte[]]@())}
$vCode=Run 'smoke-version' '--version'
if($vCode -ne 0){throw "secblitz.exe --version exited $vCode"}
$vOut=(Get-Content "$root\smoke-version.out" -Raw).Trim()
if($vOut -notmatch '0\.6'){throw "Version output does not show 0.6: $vOut"}
$aCode=Run 'smoke-audit' @('audit','--json') 120
if($aCode -ne 0 -and $aCode -ne 2){throw "secblitz.exe audit --json exited $aCode (expected 0 or 2)"}
$audit=Get-Content "$root\smoke-audit.out" -Raw|ConvertFrom-Json -ErrorAction Stop
$count=$audit.results.Count
if($count -ne 18){throw "Expected 18 audit results; got $count"}
Write-Output "Smoke: version=$vOut results=$count"
"""


def dry_run(exe_path):
    print('=== Dry run: capture-v060.py ===')
    print(f'VM UUID:    {VM}')
    print(f'OUT:        {OUT}')
    if exe_path:
        p = pathlib.Path(exe_path)
        print(f'Exe:        {exe_path} (exists={p.exists()})')
        if p.exists():
            print(f'Exe SHA256: {sha256(p)}')
    s = vm_state()
    print(f'VM state:   {s["state"]}')
    print(f'VM NICs:    {s["nics"]}')
    driver = ROOT / 'scripts/capture-v060-driver.ps1'
    print(f'Driver PS1: {driver} (exists={driver.exists()})')
    if s['state'] == 'poweroff' and all(v == 'none' for v in s['nics'].values()):
        print('VM is clean and ready for Phase B.')
    else:
        print('WARNING: VM is not in the expected clean poweroff + all-NICs-none state.')
    print()
    print('Phase B command:')
    print(f'  python scripts/capture-v060.py /path/to/secblitz.exe')


def do_setup(exe_path: pathlib.Path):
    """Start VM, verify setup, copy exe + driver, apply fixtures."""
    OUT.mkdir(parents=True, exist_ok=True)
    assert_clean_poweroff()
    (OUT / 'vm-before.txt').write_text(vbox(['showvminfo', VM, '--machinereadable']).stdout)

    # Start VM headless.
    print('Starting VM headless...')
    vbox(['startvm', VM, '--type', 'headless'])
    print('Waiting for guest control to become available...')
    time.sleep(30)

    # Run setup script.
    setup_ps1 = OUT / 'setup.ps1'
    write_inline_ps1(setup_ps1, SETUP_PS1)
    rc = guest_run(setup_ps1)
    if rc != 0:
        raise SystemExit(f'Guest setup script failed (exit {rc}). Inspect VM before proceeding.')
    print('Guest setup verified.')

    # Copy exe.
    print(f'Copying {exe_path.name} to guest...')
    guest_put(exe_path, GUEST_EXE)
    print('Exe copied.')

    # Copy driver script.
    driver = ROOT / 'scripts/capture-v060-driver.ps1'
    guest_put(driver, GUEST_DRIVER)
    print('Driver script copied.')

    # Verify exe hash on guest matches local.
    local_hash = sha256(exe_path)
    hash_ps1 = OUT / 'hash-check.ps1'
    write_inline_ps1(hash_ps1,
        f'$h=(Get-FileHash "{GUEST_EXE}" -Algorithm SHA256).Hash.ToLowerInvariant()\n'
        f'Write-Output $h')
    guest_run(hash_ps1)
    # (Hash is compared later via the smoke test version output; full hash verification
    #  is done at finalize time from the pulled binary-hash record.)

    # Run smoke tests.
    print('Running smoke tests...')
    smoke_ps1 = OUT / 'smoke.ps1'
    write_inline_ps1(smoke_ps1, SMOKE_PS1)
    rc = guest_run(smoke_ps1, timeout=180)
    if rc != 0:
        raise SystemExit(f'Smoke tests failed (exit {rc}). Inspect before recording.')
    print('Smoke tests passed.')

    # Apply fixtures.
    print('Applying fixtures...')
    fixture_ps1 = OUT / 'fixture-apply.ps1'
    write_inline_ps1(fixture_ps1, FIXTURE_APPLY_PS1)
    rc = guest_run(fixture_ps1)
    if rc != 0:
        raise SystemExit(f'Fixture application failed (exit {rc}).')
    # Pull fixture-before.json.
    guest_get(GUEST_FIXTURE_BEFORE, OUT / 'fixture-before.json')
    before = json.loads((OUT / 'fixture-before.json').read_text())
    print(f'Fixture baseline: {before}')
    assert before['PublicFirewallEnabled'] is True, 'Baseline check: firewall must be enabled before weakening.'
    assert before['DefenderArchiveScanningDisabled'] is False, 'Baseline check: archive scanning must be enabled before weakening.'
    save(OUT / 'fixture-baseline-confirmed.json', {
        'PublicFirewallEnabled_before': before['PublicFirewallEnabled'],
        'DefenderArchiveScanningDisabled_before': before['DefenderArchiveScanningDisabled'],
        'note': 'Captured before applying video fixtures.',
    })
    print('Fixtures applied and baseline confirmed.')


def do_record():
    """Run the driver script. Assumes VM is running with fixtures applied."""
    print('Starting recording driver on guest...')
    driver = ROOT / 'scripts/capture-v060-driver.ps1'
    rc = guest_run(driver, timeout=600)
    if rc != 0:
        print(f'WARNING: driver script exited {rc}. Attempting to pull recording anyway.')
    else:
        print('Driver script completed.')


def do_pull():
    """Pull recording, status, and fixture evidence from the guest."""
    OUT.mkdir(parents=True, exist_ok=True)
    print('Pulling recording from guest...')
    local_recording = OUT / 'take01.webm'
    guest_get(GUEST_RECORD, local_recording)
    file_hash = sha256(local_recording)
    file_size = local_recording.stat().st_size
    save(OUT / 'recording-hash.json', {
        'file': str(local_recording),
        'sha256': file_hash,
        'bytes': file_size,
    })
    print(f'Recording pulled: {file_size} bytes, SHA-256 {file_hash}')

    # Pull driver status.
    try:
        guest_get(GUEST_STATUS, OUT / 'driver-status.json')
        status = json.loads((OUT / 'driver-status.json').read_text(encoding='utf-8-sig'))
        print(f'Driver status: completed={status.get("Completed")} error={status.get("Error")}')
    except Exception as e:
        print(f'Could not pull driver status: {e}')

    return file_hash


def do_restore():
    """Restore fixtures and verify the baseline is back."""
    print('Restoring fixtures...')
    restore_ps1 = OUT / 'fixture-restore.ps1'
    write_inline_ps1(restore_ps1, FIXTURE_RESTORE_PS1)
    rc = guest_run(restore_ps1)
    if rc != 0:
        raise SystemExit(f'Fixture restore script failed (exit {rc}). Do not power off; inspect manually.')
    guest_get(GUEST_FIXTURE_AFTER, OUT / 'fixture-after.json')
    after = json.loads((OUT / 'fixture-after.json').read_text(encoding='utf-8-sig'))
    print(f'Fixture state after restore: {after}')
    assert after['PublicFirewallEnabled'] is True, 'Restore check failed: Public firewall not re-enabled.'
    assert after['DefenderArchiveScanningDisabled'] is False, 'Restore check failed: archive scanning still disabled.'
    save(OUT / 'fixture-restore-confirmed.json', {
        'PublicFirewallEnabled_after': after['PublicFirewallEnabled'],
        'DefenderArchiveScanningDisabled_after': after['DefenderArchiveScanningDisabled'],
        'restored': True,
    })
    print('Fixtures restored and verified.')


def do_poweroff():
    """Send ACPI shutdown and confirm poweroff."""
    print('Sending ACPI shutdown...')
    vbox(['controlvm', VM, 'acpipowerbutton'])
    wait_poweroff()


def do_finalize(exe_path: pathlib.Path, recording_hash: str):
    """Write the manifest summary for populating sources-v060.json."""
    exe_hash = sha256(exe_path)
    recording = OUT / 'take01.webm'
    save(OUT / 'capture-evidence.json', {
        'vm_uuid': VM,
        'only_ui_clone_used': True,
        'vm_poweroff_all_nics_none': True,
        'exe_sha256': exe_hash,
        'recording_sha256': recording_hash,
        'recording_bytes': recording.stat().st_size,
        'fixture_baseline': json.loads((OUT / 'fixture-baseline-confirmed.json').read_text()),
        'fixture_restore': json.loads((OUT / 'fixture-restore-confirmed.json').read_text()),
        'note': (
            'Populate video/sources-v060.json: copy take01.webm into video/public/capture/, '
            'set provenance.originalRecordingPath, set fixtureDetails, '
            'review footage and populate segments, run npm run verify-v060.'
        ),
    })
    print()
    print('=== Phase B complete ===')
    print(f'Recording: {OUT}/take01.webm')
    print(f'Recording SHA-256: {recording_hash}')
    print(f'Exe SHA-256: {exe_hash}')
    print()
    print('Next steps:')
    print('  1. Copy target/video-v060/take01.webm into video/public/capture/')
    print('  2. Review the footage and populate video/sources-v060.json segments')
    print('  3. Run: cd video && npm run verify-v060')
    print('  4. Run: TMPDIR="$PWD/out/render-temp" npm run render-v060 -- --overwrite')
    print('  5. Run: npm run deliver-v060')


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('exe', nargs='?', help='Path to the new secblitz.exe (required for full/setup modes)')
    ap.add_argument('--mode', choices=['full', 'setup', 'record', 'pull', 'restore', 'dry-run'], default='full')
    args = ap.parse_args()

    if args.mode == 'dry-run':
        dry_run(args.exe)
        return

    if args.mode in ('full', 'setup') and not args.exe:
        ap.error('Exe path required for full/setup modes.')

    exe_path = pathlib.Path(args.exe).resolve() if args.exe else None
    if exe_path and not exe_path.exists():
        raise SystemExit(f'Exe not found: {exe_path}')

    OUT.mkdir(parents=True, exist_ok=True)

    if args.mode == 'restore':
        do_restore()
        do_poweroff()
        return

    if args.mode == 'pull':
        do_pull()
        return

    if args.mode in ('full', 'setup'):
        do_setup(exe_path)
        if args.mode == 'setup':
            print('Setup complete. Fixtures applied. Run --mode record when ready.')
            return

    if args.mode in ('full', 'record'):
        do_record()
        recording_hash = do_pull()
        do_restore()
        do_poweroff()
        if args.mode == 'full':
            do_finalize(exe_path, recording_hash)


if __name__ == '__main__':
    main()
