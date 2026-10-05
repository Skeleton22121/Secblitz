[CmdletBinding()]
param(
    [string]$IsccPath,
    [switch]$DownloadInno,
    [string]$CertificateThumbprint,
    [switch]$RequirePublisherSignature,
    [string]$TimestampUrl = 'https://timestamp.digicert.com',
    [string]$SignToolPath = 'signtool.exe',
    [string]$SigningKeyPath,
    [string]$UpdateOrigin
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Build releases on x64 Windows with MSVC and the Windows SDK.' }
$root = Split-Path $PSScriptRoot -Parent
$dist = Join-Path $root 'dist'
$target = 'x86_64-pc-windows-msvc'

if ($RequirePublisherSignature -and -not $CertificateThumbprint) {
    throw 'Production signing requires credentials: supply -CertificateThumbprint for -RequirePublisherSignature.'
}
if ($CertificateThumbprint) {
    if ($CertificateThumbprint -notmatch '^[a-fA-F0-9]{40}$') { throw 'Expected a SHA-1 certificate thumbprint (40 hex digits).' }
    if ($TimestampUrl -notmatch '^https://[a-zA-Z0-9./_-]+$') { throw 'Invalid HTTPS timestamp URL.' }
    $SignToolPath = (Get-Command $SignToolPath -ErrorAction Stop).Source
}

# Build-time trust inputs only; never accept a URL from the installed task.
if (-not $PSBoundParameters.ContainsKey('UpdateOrigin')) {
    $UpdateOrigin = ([string](Get-Content -LiteralPath (Join-Path $root 'assets\update-origin.txt') -Raw)).Trim()
}
$publicKeyPath = Join-Path $root 'assets\update-public-key.hex'
if ($UpdateOrigin) {
    $origin = $null
    if (-not [Uri]::TryCreate($UpdateOrigin, [UriKind]::Absolute, [ref]$origin) -or
        $origin.Scheme -cne 'https' -or -not $origin.Host -or $origin.UserInfo -or
        $origin.Query -or $origin.Fragment -or $origin.AbsolutePath -ne '/' -or
        $UpdateOrigin -match '[\s"\\]') { throw 'UpdateOrigin must be an HTTPS origin without credentials, path, query or fragment.' }
    $UpdateOrigin = $origin.GetLeftPart([UriPartial]::Authority)
    $publicKey = ([string](Get-Content -LiteralPath $publicKeyPath -Raw)).Trim()
    if ($publicKey -cnotmatch '^[0-9a-f]{64}$' -or $publicKey -eq ('0' * 64)) {
        throw 'Configured updates require a non-placeholder Ed25519 public key in assets/update-public-key.hex (compiled by include_str!).'
    }
}
if ($SigningKeyPath) {
    if (-not $UpdateOrigin) { throw 'Release feed signing requires a configured UpdateOrigin.' }
    $SigningKeyPath = (Resolve-Path -LiteralPath $SigningKeyPath).ProviderPath
    if ($SigningKeyPath.StartsWith($root + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'The release signing secret must be outside the repository.'
    }
    if (-not (Test-Path -LiteralPath (Join-Path $root 'scripts\sign-release.py') -PathType Leaf)) { throw 'The release-feed signer scripts/sign-release.py is required.' }
}

function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE" }
}

function Assert-PublisherSignature([string]$Path) {
    Invoke-Checked $SignToolPath @('verify', '/pa', '/all', '/tw', $Path)
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate -or
        $signature.SignerCertificate.Thumbprint -ine $CertificateThumbprint -or
        $null -eq $signature.TimeStamperCertificate) {
        throw 'Release artifact must have a valid timestamped Authenticode signature from the expected publisher certificate.'
    }
}

function Sign-ReleaseFile([string]$Path) {
    Invoke-Checked $SignToolPath @('sign', '/sha1', $CertificateThumbprint, '/fd', 'SHA256', '/tr', $TimestampUrl, '/td', 'SHA256', $Path)
    Assert-PublisherSignature $Path
}

function Assert-ReleasePe([string]$Path) {
    $reader = [IO.BinaryReader]::new([IO.File]::OpenRead($Path))
    try {
        if ($reader.ReadUInt16() -ne 0x5A4D) { throw 'Expected an MZ executable.' }
        $reader.BaseStream.Position = 0x3C
        $peOffset = $reader.ReadUInt32()
        $reader.BaseStream.Position = $peOffset
        if ($reader.ReadUInt32() -ne 0x00004550 -or $reader.ReadUInt16() -ne 0x8664) {
            throw 'Expected an x64 PE executable.'
        }
        $optionalHeader = $peOffset + 24
        $reader.BaseStream.Position = $optionalHeader
        if ($reader.ReadUInt16() -ne 0x20B) { throw 'Expected a PE32+ optional header.' }
        $reader.BaseStream.Position = $optionalHeader + 70
        $dllCharacteristics = $reader.ReadUInt16()
        if (($dllCharacteristics -band 0x160) -ne 0x160) {
            throw 'Executable must enable DYNAMIC_BASE, HIGH_ENTROPY_VA and NX_COMPAT (ASLR and DEP).'
        }
        $reader.BaseStream.Position = $optionalHeader + 112 + (5 * 8)
        if ($reader.ReadUInt32() -eq 0 -or $reader.ReadUInt32() -eq 0) {
            throw 'Executable must retain base relocations for ASLR.'
        }
    } finally {
        $reader.Dispose()
    }
}

$previousFlags = $env:RUSTFLAGS
$previousEncoded = $env:CARGO_ENCODED_RUSTFLAGS
$previousOrigin = $env:SECBLITZ_UPDATE_ORIGIN
Push-Location $root
try {
    $null = New-Item -ItemType Directory -Path $dist -Force
    $env:RUSTFLAGS = '-C target-feature=+crt-static'
    # Windows PowerShell removes empty environment values. A whitespace sentinel
    # compiles to NotConfigured (core trims it), even if the fallback asset is set.
    $env:SECBLITZ_UPDATE_ORIGIN = if ($UpdateOrigin) { $UpdateOrigin } else { ' ' }
    Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
    Invoke-Checked 'python' @((Join-Path $root 'installer\check-locales.py'))
    Invoke-Checked 'powershell.exe' @('-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', (Join-Path $root 'installer\test-lifecycle.ps1'), '-OwnershipFixtures')
    foreach ($test in @('test-diagnostics-script.ps1', 'test-operations-script.ps1', 'test-patching-script.ps1')) {
        Invoke-Checked 'powershell.exe' @('-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
            '-File', (Join-Path $root ('scripts\' + $test)))
    }
    Invoke-Checked 'rustup' @('target', 'add', $target)
    Invoke-Checked 'cargo' @('fmt', '--all', '--', '--check')
    Invoke-Checked 'cargo' @('test', '--locked', '--all-targets', '--target', $target)
    Invoke-Checked 'cargo' @('clippy', '--locked', '--all-targets', '--target', $target, '--', '-D', 'warnings')
    # build.rs does not track the icon itself yet: force a resource rebuild.
    Invoke-Checked 'cargo' @('clean', '-p', 'secblitz', '--release', '--target', $target, '--target-dir', (Join-Path $root 'target'))
    Invoke-Checked 'cargo' @('build', '--locked', '--release', '--target', $target, '--target-dir', (Join-Path $root 'target'))
    $metadataText = & cargo metadata --locked --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
    $metadata = $metadataText | ConvertFrom-Json
    $packages = @($metadata.packages | Where-Object name -eq 'secblitz')
    if ($packages.Count -ne 1) { throw 'Expected exactly one secblitz package in Cargo metadata.' }
    $version = $packages[0].version
    if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Installer requires a numeric major.minor.patch version.' }
    $exe = Join-Path $dist "secblitz-$version-windows-x64.exe"
    Copy-Item -LiteralPath (Join-Path $root "target\$target\release\secblitz.exe") -Destination $exe -Force
    $binaryVersion = & $exe '--version'
    if ($LASTEXITCODE -ne 0 -or ($binaryVersion -join "`n").Trim() -cne "secblitz $version") {
        throw "Executable version does not match Cargo package version $version."
    }
    # Use a VS Developer PowerShell (CI initializes it below). Inspect actual PE imports.
    $imports = & dumpbin.exe /dependents $exe
    if ($LASTEXITCODE -ne 0) { throw 'dumpbin failed.' }
    if (($imports -join "`n") -match '(?i)(VCRUNTIME\d*|MSVCP\d*|ucrtbase|api-ms-win-crt-[\w-]+)\.dll') {
        throw 'Dynamic CRT import detected; refusing to package.'
    }
    if (-not $IsccPath) {
        $IsccPath = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'
    }
    if (-not (Test-Path -LiteralPath $IsccPath) -and $DownloadInno) {
        $compilerInstaller = Join-Path $dist 'innosetup-6.4.3.exe'
        Invoke-WebRequest 'https://files.jrsoftware.org/is/6/innosetup-6.4.3.exe' -OutFile $compilerInstaller
        $signature = Get-AuthenticodeSignature -LiteralPath $compilerInstaller
        if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'CN=Jordan Russell(?:,|$)') {
            throw 'Official Inno Setup download failed publisher signature validation.'
        }
        $process = Start-Process -FilePath $compilerInstaller -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-' -Wait -PassThru
        if ($process.ExitCode -ne 0) { throw "Inno compiler installation failed: $($process.ExitCode)" }
    }
    if (-not (Test-Path -LiteralPath $IsccPath)) { throw 'Install official Inno Setup 6.4+ or pass -DownloadInno from an elevated build shell.' }
    $compilerArgs = @("/DAppVersion=$version", "/DSourceExe=$exe", "/DOutputPath=$dist")
    if ($CertificateThumbprint) {
        Sign-ReleaseFile $exe
        # Inno signs its uninstaller and final installer with this configured tool.
        $signCommand = '"' + $SignToolPath + '" sign /sha1 ' + $CertificateThumbprint + ' /fd SHA256 /tr ' + $TimestampUrl + ' /td SHA256 $f'
        $compilerArgs += '/DSignToolName=secblitz'
        $compilerArgs += '/Ssecblitz=' + $signCommand
    }
    $compilerArgs += (Join-Path $root 'installer\setup.iss')
    Invoke-Checked $IsccPath $compilerArgs
    $setup = Join-Path $dist "secblitz-$version-windows-x64-setup.exe"
    # Verify final artifacts before checksums or the signed update manifest.
    Assert-ReleasePe $exe
    if ($CertificateThumbprint) {
        Assert-PublisherSignature $exe
        Assert-PublisherSignature $setup
    }
    $hashes = foreach ($file in @($exe, $setup)) {
        '{0}  {1}' -f (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant(), (Split-Path $file -Leaf)
    }
    $hashes | Set-Content -LiteralPath (Join-Path $dist 'SHA256SUMS.txt') -Encoding ascii
    # Sign only the final bytes, after Inno and optional Authenticode have finished.
    if ($SigningKeyPath) {
        Invoke-Checked 'python' @((Join-Path $root 'scripts\sign-release.py'),
            '--key', $SigningKeyPath, '--public-key', $publicKeyPath, '--version', $version,
            '--installer', $setup, '--output', (Join-Path $dist 'stable.json'))
    }
    if ($CertificateThumbprint) { Write-Host 'Signed executable and installer verified.' }
    else { Write-Host 'UNSIGNED PREVIEW: no signing certificate was supplied. Production signing requires credentials; this preview is not publisher-certified.' }
    Write-Host "Release artifacts: $dist"
} finally {
    $env:RUSTFLAGS = $previousFlags
    $env:CARGO_ENCODED_RUSTFLAGS = $previousEncoded
    $env:SECBLITZ_UPDATE_ORIGIN = $previousOrigin
    Pop-Location
}
