##
## Secblitz 0.6.0 video capture driver - OUTER ORCHESTRATOR (Session 0)
## Run via guestcontrol from the host (capture-v060.py or guest.py ps).
##
## This script runs in Session 0 (non-interactive, guestcontrol).
##   - Starts gdigrab (captures the VirtualBox virtual framebuffer, showing Session 1).
##   - Creates a Scheduled Task to run capture-v060-inner.ps1 in Session 1
##     (the interactive desktop, where wt.exe can open a visible window).
##   - Waits for the inner script's done sentinel.
##   - Stops gdigrab and writes driver-status.json.
##
## The inner script (capture-v060-inner.ps1) handles wt.exe + SendKeys in Session 1.
##
## Requirements:
##   - Administrator must be logged in at the console (Session 1).
##   - Screen resolution must be 1280x720 (set via VBoxManage setvideomodehint).
##   - ffmpeg.exe in C:\Windows\Temp\SecblitzV031Capture\
##   - secblitz.exe in C:\Windows\Temp\secblitz-v060-video\
##   - capture-v060-inner.ps1 in C:\Windows\Temp\secblitz-v060-video\
##
param(
    [string]$ExePath    = 'C:\Windows\Temp\secblitz-v060-video\secblitz.exe',
    [string]$FfmpegPath = 'C:\Windows\Temp\SecblitzV031Capture\ffmpeg.exe',
    [string]$OutDir     = 'C:\Windows\Temp\secblitz-v060-video',
    [string]$RecordName = 'take01.webm',
    [string]$InnerScript = 'C:\Windows\Temp\secblitz-v060-video\capture-v060-inner.ps1',
    [switch]$Rehearsal
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
$utf8 = [System.Text.UTF8Encoding]::new($false)

if (!(Test-Path $OutDir)) { New-Item -ItemType Directory -Path $OutDir | Out-Null }
if (!(Test-Path "$OutDir\empty.stdin")) {
    [IO.File]::WriteAllBytes("$OutDir\empty.stdin", [byte[]]@())
}

$statusFile   = "$OutDir\driver-status.json"
$sentinelFile = "$OutDir\driver-done.txt"
if (Test-Path $sentinelFile) { Remove-Item $sentinelFile -Force }

$taskName = 'SecblitzV060Capture'

$ffmpegProc = $null
$proof = [ordered]@{
    StartUtc    = [DateTime]::UtcNow.ToString('o')
    ExePath     = $ExePath
    Rehearsal   = [bool]$Rehearsal
    Completed   = $false
    ExitCode    = $null
    GdigrabPid  = $null
    EndUtc      = $null   # initialized so finally block never hits StrictMode error
    Error       = $null
}

try {
    ## --- 1. Verify prerequisites ---
    if (-not (Test-Path $ExePath))    { throw "secblitz.exe not found: $ExePath" }
    if (-not (Test-Path $InnerScript)) { throw "Inner script not found: $InnerScript" }
    if (-not $Rehearsal -and -not (Test-Path $FfmpegPath)) {
        throw "ffmpeg.exe not found: $FfmpegPath"
    }

    ## --- 2. Verify Session 1 has a logged-in user (required for wt.exe) ---
    $sessions = (query session 2>&1)
    $hasInteractive = $sessions | Where-Object { $_ -match 'console\s+\S+' }
    if (-not $hasInteractive) {
        throw "No user is logged into Session 1 (console). Log in as Administrator before recording."
    }

    ## --- 3. Start gdigrab recording (background, Session 0 ok) ---
    if (-not $Rehearsal) {
        $recording = "$OutDir\$RecordName"
        if (Test-Path $recording) { Remove-Item $recording -Force }
        # Use Diagnostics.Process like the v031 worker for reliable background launch.
        $ffmpegArgs = "-hide_banner -loglevel warning -y -f gdigrab -framerate 30 -draw_mouse 0 -video_size 1280x720 -offset_x 0 -offset_y 0 -i desktop -an -vcodec libvpx-vp9 -lossless 1 -r 30 -deadline realtime -cpu-used 8 -row-mt 1 -threads 3 $recording"
        $si = [Diagnostics.ProcessStartInfo]::new($FfmpegPath, $ffmpegArgs)
        $si.UseShellExecute        = $false
        $si.CreateNoWindow         = $true
        $si.RedirectStandardInput  = $true
        $si.RedirectStandardOutput = $true
        $si.RedirectStandardError  = $true
        $ffmpegProc = [Diagnostics.Process]::Start($si)
        $null = $ffmpegProc.StandardOutput.ReadToEndAsync()
        $errTask = $ffmpegProc.StandardError.ReadToEndAsync()
        $proof.GdigrabPid = $ffmpegProc.Id
        Start-Sleep -Milliseconds 2500  # Let gdigrab initialize.
        # Abort early if ffmpeg died immediately (e.g. wrong resolution).
        if ($ffmpegProc.HasExited) {
            $errOut = if ($errTask.IsCompleted) { $errTask.Result } else { '(not yet ready)' }
            [IO.File]::WriteAllText("$OutDir\ffmpeg.err", $errOut, $utf8)
            throw "ffmpeg exited immediately (exit=$($ffmpegProc.ExitCode)). Check ffmpeg.err. Is display 1280x720?"
        }
    }

    ## --- 4. Clean up any leftover scheduled task from a prior run ---
    schtasks /delete /f /tn $taskName 2>$null | Out-Null

    ## --- 5. Register the inner script as a scheduled task in Session 1 ---
    ## LogonType=InteractiveToken: task runs in the currently logged-on user's session.
    ## RunLevel=HighestAvailable: run elevated (Administrator).
    $psArgs = "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$InnerScript`" -ExePath `"$ExePath`" -OutDir `"$OutDir`""
    $action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $psArgs
    $settings = New-ScheduledTaskSettingsSet `
        -AllowStartIfOnBatteries `
        -DontStopIfGoingOnBatteries `
        -ExecutionTimeLimit (New-TimeSpan -Minutes 10) `
        -MultipleInstances IgnoreNew
    $principal = New-ScheduledTaskPrincipal `
        -UserId 'Administrator' `
        -LogonType Interactive `
        -RunLevel Highest
    Register-ScheduledTask -TaskName $taskName -Action $action `
        -Settings $settings -Principal $principal -Force | Out-Null

    ## --- 6. Run the task immediately ---
    Start-ScheduledTask -TaskName $taskName

    ## --- 7. Wait for inner script to signal done (up to 150 s) ---
    $deadline = [DateTime]::UtcNow.AddSeconds(150)
    while (-not (Test-Path $sentinelFile) -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 500
    }
    if (-not (Test-Path $sentinelFile)) {
        throw "Timed out waiting for inner script sentinel after 150 s."
    }

    ## --- 8. Read inner status and propagate ---
    $innerStatusFile = "$OutDir\driver-inner-status.json"
    if (Test-Path $innerStatusFile) {
        $inner = Get-Content $innerStatusFile -Raw | ConvertFrom-Json
        $proof.InnerCompleted = $inner.Completed
        $proof.InnerError     = $inner.Error
        $proof.InnerEndUtc    = $inner.EndUtc
        if (-not $inner.Completed) {
            throw "Inner script reported failure: $($inner.Error)"
        }
    }

    $proof.Completed = $true
    $proof.ExitCode  = 0
    $proof.EndUtc    = [DateTime]::UtcNow.ToString('o')

} catch {
    $proof.Completed = $false
    $proof.Error     = $_.ToString()
    [IO.File]::WriteAllText("$OutDir\driver-error.txt", ($_ | Out-String), $utf8)
} finally {
    ## Stop gdigrab: send 'q' for clean VP9 file flush, then force-kill.
    if ($ffmpegProc -and -not $ffmpegProc.HasExited) {
        try {
            $ffmpegProc.StandardInput.WriteLine('q')
            $ffmpegProc.StandardInput.Flush()
            $ffmpegProc.WaitForExit(8000) | Out-Null
        } catch {}
        if (-not $ffmpegProc.HasExited) {
            Stop-Process -Id $ffmpegProc.Id -Force -ErrorAction SilentlyContinue
        }
        ## Flush stderr to file.
        if (-not $null -eq $ffmpegProc) {
            try {
                $e = $ffmpegProc.StandardError.ReadToEnd()
                if ($e) { [IO.File]::WriteAllText("$OutDir\ffmpeg.err", $e, $utf8) }
            } catch {}
        }
    }

    ## Clean up scheduled task.
    schtasks /delete /f /tn $taskName 2>$null | Out-Null

    if ($null -eq $proof.EndUtc) {
        $proof.EndUtc = [DateTime]::UtcNow.ToString('o')
    }
    [IO.File]::WriteAllText($statusFile, (ConvertTo-Json $proof -Depth 4), $utf8)
    [IO.File]::WriteAllText($sentinelFile, 'done', $utf8)
}
