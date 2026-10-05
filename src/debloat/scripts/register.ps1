# Registers saved app packages that Secblitz has already copied back into
# WindowsApps and checked. Input: base64 JSON in $env:SECBLITZ_REGISTER with
# "manifests" (in order: frameworks, then bundle or main) and "provision"
# (family name or ""). Output: one JSON object.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$result = [ordered]@{ ok = $false; error = $null; status = @() }
try {
    $json = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String([string]$env:SECBLITZ_REGISTER))
    $request = ConvertFrom-Json -InputObject $json
    foreach ($m in @($request.manifests)) {
        Add-AppxPackage -Register ([string]$m) -DisableDevelopmentMode -ErrorAction Stop
    }
    $family = [string]$request.provision
    if ($family) {
        $null = [Windows.Management.Deployment.PackageManager, Windows.Management.Deployment, ContentType = WindowsRuntime]
        Add-Type -AssemblyName System.Runtime.WindowsRuntime
        $asTask = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
            $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and
            $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperationWithProgress`2'
        } | Select-Object -First 1
        $pm = [Windows.Management.Deployment.PackageManager]::new()
        $op = $pm.ProvisionPackageForAllUsersAsync($family)
        $task = $asTask.MakeGenericMethod([Windows.Management.Deployment.DeploymentResult], [Windows.Management.Deployment.DeploymentProgress]).Invoke($null, @($op))
        if (-not $task.Wait(600000)) { throw 'Making the app available to every account timed out' }
        if ($task.Result.ExtendedErrorCode) { throw $task.Result.ErrorText }
    }
    foreach ($m in @($request.manifests)) {
        $full = Split-Path -Leaf (Split-Path -Parent ([string]$m))
        if ($full -eq 'AppxMetadata') { $full = Split-Path -Leaf (Split-Path -Parent (Split-Path -Parent ([string]$m))) }
        $p = Get-AppxPackage -PackageTypeFilter Main,Bundle,Framework | Where-Object { $_.PackageFullName -eq $full } | Select-Object -First 1
        $result.status += ,([ordered]@{ fullName = $full; status = if ($p) { [string]$p.Status } else { 'Missing' } })
    }
    $result.ok = @($result.status | Where-Object { $_.status -ne 'Ok' }).Count -eq 0
    if (-not $result.ok) { $result.error = 'Windows did not accept the saved copy' }
}
catch {
    $result.error = [string]$_.Exception.Message
}
ConvertTo-Json -InputObject $result -Compress -Depth 4
