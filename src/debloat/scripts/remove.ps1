# Removes ONE app (name in $env:SECBLITZ_APP, already validated against the
# compiled catalog by the caller) for all users and from the Windows image so
# new accounts do not get it. Output: one JSON object.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$name = [string]$env:SECBLITZ_APP
$result = [ordered]@{ name = $name; removed = $false; protected = $false; error = $null }
function Test-ProtectedText([string]$text) {
    return ($text -like '*0x80073CFA*' -or $text -like '*NonRemovable*' -or $text -like '*cannot be removed*')
}
function Remove-ForEveryone([string]$fullName) {
    try { Remove-AppxPackage -AllUsers -Package $fullName; return $null }
    catch { return $_ }
}
try {
    if ([string]::IsNullOrWhiteSpace($name)) { throw 'No app name' }
    $installed = @(Get-AppxPackage -AllUsers -Name $name)
    foreach ($p in $installed) {
        if ($p.NonRemovable -or $p.IsFramework) { $result.protected = $true }
    }
    if (-not $result.protected) {
        # Windows 10 provisions inbox apps under the bundle name, so removing
        # the main package for all users fails to deprovision it (0x80070002).
        # Removing the bundle takes the main and resource packages with it.
        $bundles = @(Get-AppxPackage -AllUsers -PackageTypeFilter Bundle -Name $name)
        $windows10 = [Environment]::OSVersion.Version.Build -lt 22000
        $targets = if ($windows10 -and $bundles.Count -gt 0) { $bundles } else { $installed }
        foreach ($p in $targets) {
            $failure = Remove-ForEveryone $p.PackageFullName
            if ($null -ne $failure -and -not $p.IsBundle -and $bundles.Count -gt 0) {
                foreach ($b in $bundles) {
                    $failure = Remove-ForEveryone $b.PackageFullName
                    if ($null -ne $failure) { break }
                }
            }
            if ($null -ne $failure) {
                if (Test-ProtectedText ($failure.Exception.Message + ' ' + $failure.FullyQualifiedErrorId)) { $result.protected = $true; break }
                throw $failure
            }
            if (@(Get-AppxPackage -AllUsers -Name $name).Count -eq 0) { break }
        }
    }
    if (-not $result.protected) {
        foreach ($p in @(Get-AppxProvisionedPackage -Online | Where-Object { $_.DisplayName -eq $name })) {
            try { Remove-AppxProvisionedPackage -Online -PackageName $p.PackageName | Out-Null }
            catch {
                if (Test-ProtectedText ($_.Exception.Message + ' ' + $_.FullyQualifiedErrorId)) { $result.protected = $true; break }
                throw
            }
        }
    }
    if (-not $result.protected) {
        $left = @(Get-AppxPackage -AllUsers -Name $name)
        if ($left.Count -eq 0) { $result.removed = $true }
        else { $result.error = 'The app is still installed for at least one account.' }
    }
}
catch {
    $result.error = [string]$_.Exception.Message
}
ConvertTo-Json -InputObject $result -Compress
