[CmdletBinding()]
param(
    [string]$DownloadDirectory = $env:RUNNER_TEMP
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Inno Setup installs on Windows only.' }
$iscc = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'
if (Test-Path -LiteralPath $iscc) { return }
if (-not $DownloadDirectory) { $DownloadDirectory = [IO.Path]::GetTempPath() }
$compilerInstaller = Join-Path $DownloadDirectory 'innosetup-6.4.3.exe'
Invoke-WebRequest 'https://files.jrsoftware.org/is/6/innosetup-6.4.3.exe' -OutFile $compilerInstaller
$expectedHash = 'f3c42116542c4cc57263c5ba6c4feabfc49fe771f2f98a79d2f7628b8762723b'
$actualHash = (Get-FileHash -LiteralPath $compilerInstaller -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualHash -cne $expectedHash) {
    throw "Inno Setup download does not match the pinned SHA-256 (got $actualHash)."
}
$signature = Get-AuthenticodeSignature -LiteralPath $compilerInstaller
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'CN=Jordan Russell(?:,|$)') {
    throw 'Official Inno Setup download failed publisher signature validation.'
}
$process = Start-Process -FilePath $compilerInstaller -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-' -Wait -PassThru
if ($process.ExitCode -ne 0) { throw "Inno compiler installation failed: $($process.ExitCode)" }
