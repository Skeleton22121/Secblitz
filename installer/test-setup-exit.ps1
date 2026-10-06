# Compile/run no-install harnesses against the production event handlers. These
# do not install files, register services or touch Secblitz application/data paths.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$IsccPath)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$work = Join-Path ([IO.Path]::GetTempPath()) ('SecblitzExitTest-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $work
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'maintenance.ps1') -Destination $work
$source = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'setup.iss') -Raw
$code = $source.Substring($source.IndexOf('[Code]'))
$pattern = '(?s)function Maintain\(Action: String\): Boolean;.*?(?=function PrepareToInstall)'
if ([regex]::Matches($code, $pattern).Count -ne 1) { throw 'Cannot locate the production maintenance seam.' }
foreach ($case in @('Success', 'Secure', 'InstallFilter', 'InstallMonitor', 'ResumeMonitor', 'ResumeFilter', 'Exception')) {
    $mock = @"
function Maintain(Action: String): Boolean;
begin
  if (Action = 'Prepare') and ('$case' = 'ResumeMonitor') then ResumeAfterUpgrade := True;
  if (Action = 'Prepare') and ('$case' = 'ResumeFilter') then ResumeFilterAfterUpgrade := True;
  if (Action = 'InstallMonitor') and ('$case' = 'Exception') then RaiseException('Injected maintenance exception');
  Result := Action <> '$case';
end;

"@
    $testCode = [regex]::Replace($code, $pattern, $mock)
    # CreateAppDir=no deliberately makes {app} unsuitable for the production
    # fixed-directory precondition. Bypass Prepare only in this no-install harness.
    $prepare = "function PrepareToInstall(var NeedsRestart: Boolean): String;`r`nbegin`r`n  Result := '';`r`n  if '$case' = 'ResumeMonitor' then ResumeAfterUpgrade := True;`r`n  if '$case' = 'ResumeFilter' then ResumeFilterAfterUpgrade := True;`r`nend;`r`n`r`n"
    $testCode = [regex]::Replace($testCode, '(?s)function PrepareToInstall\(var NeedsRestart: Boolean\): String;.*?(?=procedure CurStepChanged)', $prepare)
    $messages = @([regex]::Matches($code, "CustomMessage\('([A-Za-z0-9]+)'\)|\{cm:([A-Za-z0-9]+)") |
        ForEach-Object { if ($_.Groups[1].Success) { $_.Groups[1].Value } else { $_.Groups[2].Value } } |
        Where-Object { $_ -cne 'Failed' } | Sort-Object -Unique |
        ForEach-Object { "$_=Test message" }) -join "`r`n"
    $harness = @"
[Setup]
AppName=SecblitzExitTest
AppVersion=1.0
DefaultDirName={autopf64}\Secblitz
CreateAppDir=no
Uninstallable=no
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=$work
OutputBaseFilename=$case
[Tasks]
Name: "monitor"; Description: "Monitor test"
[CustomMessages]
Failed=Injected maintenance failure
$messages
"@ + "`r`n" + $testCode
    $script = Join-Path $work "$case.iss"
    Set-Content -LiteralPath $script -Value $harness -Encoding UTF8
    & $IsccPath $script
    if ($LASTEXITCODE -ne 0) { throw "Compile failed: $case" }
    $p = Start-Process -FilePath (Join-Path $work "$case.exe") -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/SP-', '/NORESTART', '/TASKS=monitor', "/LOG=`"$work\$case.log`"" -PassThru
    try {
        # Retain the handle before waiting: short-lived processes can otherwise
        # yield an empty ExitCode in Windows PowerShell 5.1.
        $null = $p.Handle
        if (-not $p.WaitForExit(60000)) { throw "Harness timeout: $case" }
        $expected = if ($case -eq 'Success') { 0 } else { 20 }
        if ($p.ExitCode -ne $expected) { throw "${case}: expected $expected, got $($p.ExitCode). Logs: $work" }
        Write-Host "PASS ${case}: exit $($p.ExitCode)"
    } finally { $p.Dispose() }
}
Write-Host "Exit-code regression evidence: $work"
