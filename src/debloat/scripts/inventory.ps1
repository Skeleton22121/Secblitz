# Lists installed and pre-installed (provisioned) app packages as compact JSON.
# Read-only. Output: array of {name, version, installed, provisioned, nonRemovable, framework}.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$map = @{}
foreach ($p in @(Get-AppxPackage -AllUsers)) {
    $key = [string]$p.Name
    if (-not $map.ContainsKey($key)) {
        $map[$key] = [ordered]@{
            name = $key; version = [string]$p.Version; installed = $true; provisioned = $false
            nonRemovable = $false; framework = $false
        }
    }
    if ($p.NonRemovable) { $map[$key].nonRemovable = $true }
    if ($p.IsFramework) { $map[$key].framework = $true }
}
foreach ($p in @(Get-AppxProvisionedPackage -Online)) {
    $key = [string]$p.DisplayName
    if (-not $map.ContainsKey($key)) {
        $map[$key] = [ordered]@{
            name = $key; version = [string]$p.Version; installed = $false; provisioned = $true
            nonRemovable = $false; framework = $false
        }
    }
    else { $map[$key].provisioned = $true }
}
$list = @($map.Values)
if ($list.Count -eq 0) { '[]' } else { ConvertTo-Json -InputObject $list -Compress -Depth 3 }
