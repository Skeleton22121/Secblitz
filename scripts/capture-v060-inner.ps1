##
## Secblitz 0.6.0 video capture - INNER SCRIPT (Session 1 / interactive desktop)
## Launched via a scheduled task by capture-v060-driver.ps1.
## Must run in the user's interactive session so wt.exe appears on screen.
##
## NEW STORYBOARD (redesign build, matches team-lead Phase B instructions):
##   Home with logo + status card (3s hold)
##   -> Check again (live checklist, DOWN x2 ENTER, wait for check, 3s hold, ESC)
##   -> Fix recommended (ENTER, default=0)
##   -> Recap with Risk lines (hold 2s)
##   -> Apply these fixes (UP UP ENTER)
##   -> Post-check completes -> Fix results doc (4s hold, ESC)
##   -> Advanced (DOWN x3 ENTER)
##   -> Undo (ENTER default=0, say() then confirm: UP ENTER)
##   -> Undo result doc (3s hold, ESC, ESC to exit Advanced)
##   -> Exit (DOWN x4 ENTER)
##
## Key derivations from src/guided.rs:
##   ROOT_ITEMS default=0 = Fix recommended; Check again = index 2 (DOWN x2 ENTER)
##   approve_plan recap: default=2 = Back -> UP UP ENTER for "Apply these fixes"
##   ADVANCED_ITEMS default=0 = Undo -> ENTER; confirm default=1 "No, go back" -> UP ENTER
##   After each sub-action, home menu resets to default=0
##   EXIT: DOWN x4 from root = index 4 = "Exit"
##
param(
    [string]$ExePath  = 'C:\Windows\Temp\secblitz-v060-video\secblitz.exe',
    [string]$OutDir   = 'C:\Windows\Temp\secblitz-v060-video'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
$utf8 = [System.Text.UTF8Encoding]::new($false)

$innerStatus = "$OutDir\driver-inner-status.json"
$sentinel    = "$OutDir\driver-done.txt"
if (Test-Path $sentinel) { Remove-Item $sentinel -Force }

Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class W32Inner {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
}
'@

function Hold([int]$ms) { Start-Sleep -Milliseconds $ms }

function SendKey([string]$key, [int]$count = 1) {
    Hold 220
    for ($i = 0; $i -lt $count; $i++) {
        [System.Windows.Forms.SendKeys]::SendWait($key)
        if ($count -gt 1) { Hold 180 }
    }
}

function FocusTerminal {
    foreach ($name in @('WindowsTerminal','conhost','cmd')) {
        $p = Get-Process -Name $name -ErrorAction SilentlyContinue | Select-Object -Last 1
        if ($p -and $p.MainWindowHandle -ne 0) {
            [W32Inner]::ShowWindow($p.MainWindowHandle, 9)  # SW_RESTORE
            [W32Inner]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
            Hold 400
            return $true
        }
    }
    return $false
}

$proof = [ordered]@{
    StartUtc      = [DateTime]::UtcNow.ToString('o')
    ExePath       = $ExePath
    Completed     = $false
    ExitCode      = $null
    FocusAcquired = $false
    AppPid        = $null
    EndUtc        = $null   # initialized so finally never hits StrictMode error
    Error         = $null
}

try {
    ## Open Windows Terminal 120x32, run secblitz.exe guide elevated.
    ## This session IS already Administrator (scheduled task with RunLevel=Highest).
    $wtArgs = '--size 120,32 -- cmd.exe /k "' + $ExePath + '" guide'
    $wtProc = Start-Process -FilePath 'wt.exe' -ArgumentList $wtArgs -PassThru
    $proof.AppPid = $wtProc.Id
    Hold 4000   # Wait for wt.exe to open and secblitz.exe to start.
    $proof.FocusAcquired = (FocusTerminal)
    Hold 500

    ## Wait for the initial startup scan (can take 5-20 s).
    Hold 22000
    FocusTerminal | Out-Null
    Hold 500

    ## ----------------------------------------------------------------
    ## HOME SCREEN: logo + "X of N checks need attention" visible.
    ## Cursor is at index 0 (Fix recommended).
    ## ----------------------------------------------------------------
    Hold 3500   # 3.5 s hold: viewer sees logo + status card.

    ## CHECK AGAIN (live checklist): DOWN x2 -> index 2, ENTER.
    SendKey '{DOWN}' 2
    Hold 400
    SendKey '{ENTER}'
    ## Check again runs a fresh audit; shows "Check results" document.
    Hold 22000  # Wait for check to complete and document to appear.

    ## CHECK RESULTS DOCUMENT: show the live protection checklist.
    Hold 3500   # 3.5 s hold: viewer reads the protection status.
    SendKey '{ESCAPE}'  # Close Check results document.
    Hold 1500   # Home menu re-appears with cursor at index 0.

    ## ----------------------------------------------------------------
    ## FIX RECOMMENDED: cursor is at 0, ENTER to select.
    ## ----------------------------------------------------------------
    FocusTerminal | Out-Null
    SendKey '{ENTER}'
    ## Recap with "Risk:" annotations appears.
    Hold 1500   # Let recap render.
    Hold 2500   # 2.5 s: viewer reads fix list and Risk lines.

    ## Navigate recap from Back (default=2) to "Apply these fixes" (index 0): UP UP ENTER.
    SendKey '{UP}'
    Hold 220
    SendKey '{UP}'
    Hold 220
    SendKey '{ENTER}'
    ## Apply runs, then automatic post-check runs.
    Hold 22000  # Generous wait.

    ## FIX RESULTS DOCUMENT: "You're now protected from:" visible.
    Hold 4000   # 4 s hold: payoff -- viewer reads the protection list.
    SendKey '{ESCAPE}'  # Close result document.
    Hold 1500

    ## ----------------------------------------------------------------
    ## ADVANCED > UNDO
    ## ----------------------------------------------------------------
    FocusTerminal | Out-Null
    SendKey '{DOWN}' 3   # Index 3 = Advanced.
    Hold 400
    SendKey '{ENTER}'
    Hold 1000

    ## UNDO (default=0): ENTER -> say() note shows -> confirm prompt appears.
    SendKey '{ENTER}'
    Hold 1500   # Let say() note + confirm render.
    Hold 1500   # Brief hold on confirm prompt.

    ## Confirm from "No, go back" (default=1): UP -> "Yes, continue" -> ENTER.
    SendKey '{UP}'
    Hold 300
    SendKey '{ENTER}'
    ## Undo runs, post-check, result document.
    Hold 20000  # Generous wait.

    ## UNDO RESULT DOCUMENT: last real footage frame.
    Hold 3500   # 3.5 s hold: viewer reads undo result.
    SendKey '{ESCAPE}'  # Close undo result document.
    Hold 1000

    ## ESC to exit Advanced loop (choose() returns None -> advanced() returns).
    SendKey '{ESCAPE}'
    Hold 1200

    ## ----------------------------------------------------------------
    ## EXIT: DOWN x4 from home index 0 = index 4 = "Exit"
    ## ----------------------------------------------------------------
    FocusTerminal | Out-Null
    SendKey '{DOWN}' 4
    Hold 400
    SendKey '{ENTER}'
    Hold 2000   # Let app exit cleanly.

    $proof.Completed = $true
    $proof.ExitCode  = 0
    $proof.EndUtc    = [DateTime]::UtcNow.ToString('o')

} catch {
    $proof.Completed = $false
    $proof.Error     = $_.ToString()
    [IO.File]::WriteAllText("$OutDir\driver-error.txt", ($_ | Out-String), $utf8)
} finally {
    if ($null -eq $proof.EndUtc) {
        $proof.EndUtc = [DateTime]::UtcNow.ToString('o')
    }
    [IO.File]::WriteAllText($innerStatus, (ConvertTo-Json $proof -Depth 4), $utf8)
    ## Write sentinel LAST so the outer driver sees a complete status file.
    [IO.File]::WriteAllText($sentinel, 'done', $utf8)
}
