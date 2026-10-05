# Describes ONE app (name in $env:SECBLITZ_APP, validated by the caller) so
# Secblitz can save a copy before removing it. Read-only. Output: one JSON
# object. Paths are not reported: Secblitz derives them itself.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$name = [string]$env:SECBLITZ_APP
$result = [ordered]@{ packages = @(); frameworks = @(); provisioned = $false; users = @(); template = $null; error = $null }
try {
    if ([string]::IsNullOrWhiteSpace($name)) { throw 'No app name' }
    $all = @(Get-AppxPackage -AllUsers -Name $name -PackageTypeFilter Main,Bundle,Resource)
    $frameworks = @{}
    $users = @{}
    foreach ($p in $all) {
        $kind = if ($p.IsBundle) { 'bundle' } elseif ($p.IsResourcePackage) { 'resource' } else { 'main' }
        $result.packages += ,([ordered]@{ fullName = [string]$p.PackageFullName; kind = $kind; family = [string]$p.PackageFamilyName })
        if ($kind -eq 'main') {
            foreach ($d in @($p.Dependencies)) {
                if ($d.IsFramework) { $frameworks[[string]$d.PackageFullName] = $true }
            }
            foreach ($u in @($p.PackageUserInformation)) {
                $sid = [string]$u.UserSecurityId.Sid
                if ($sid -like 'S-1-5-21-*' -and [string]$u.InstallState -eq 'Installed') { $users[$sid] = $true }
            }
        }
    }
    $result.frameworks = @($frameworks.Keys | Sort-Object)
    $result.users = @($users.Keys | Sort-Object)
    $result.provisioned = @(Get-AppxProvisionedPackage -Online | Where-Object { $_.DisplayName -eq $name }).Count -gt 0
    # A registered Microsoft app whose folder security is copied for restore.
    $t = Get-AppxPackage -PackageTypeFilter Main | Where-Object {
        $_.PackageFamilyName -like '*_8wekyb3d8bbwe' -and -not $_.IsFramework -and $_.SignatureKind -eq 'Store' -and
        $_.Name -ne $name -and [string]$_.Status -eq 'Ok'
    } | Sort-Object Name | Select-Object -First 1
    if ($t) { $result.template = [ordered]@{ family = [string]$t.PackageFamilyName; fullName = [string]$t.PackageFullName } }
}
catch {
    $result.error = [string]$_.Exception.Message
}
ConvertTo-Json -InputObject $result -Compress -Depth 4
