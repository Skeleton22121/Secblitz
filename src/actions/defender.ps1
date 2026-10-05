# Fixed support operations only. Policy helpers and trusted module bootstrap
# come from the compiled backend, with its dispatcher excluded by Rust.
try {
    if ($supportId -cnotin @('defender_update','defender_quickscan')) { throw 'Unknown support operation' }
    Load 'CimCmdlets'
    $os = Get-CimInstance Win32_OperatingSystem
    if ($os.ProductType -ne 1 -or [int]$os.BuildNumber -lt 10240 -or ![Environment]::Is64BitProcess) { throw 'Unsupported Windows client capability' }
    $cs = Get-CimInstance Win32_ComputerSystem
    if ($cs.PartOfDomain -isnot [bool] -or $cs.PartOfDomain) { throw 'Domain-managed or unknown membership: support action declined' }
    if (MdmRegistered) { throw 'MDM-managed device: support action declined' }
    foreach ($p in @('HKLM:\SOFTWARE\Microsoft\Provisioning\OMADM\Accounts','HKLM:\SYSTEM\CurrentControlSet\Control\CloudDomainJoin\JoinInfo')) {
        if ((Test-Path -LiteralPath $p) -and @(Get-ChildItem -LiteralPath $p).Count -gt 0) { throw 'Enrollment or cloud-management evidence: support action declined' }
    }
    foreach ($pol in @('System32\GroupPolicy\Machine\Registry.pol','System32\GroupPolicy\gpt.ini')) {
        if (Test-Path -LiteralPath ([IO.Path]::Combine($env:SystemRoot, $pol))) { throw 'Local policy artifacts: support action declined' }
    }
    CheckScopedPolicy 'defender.support'
    CheckRsop 'defender.support'
    $providers = @(Get-CimInstance -Namespace root\SecurityCenter2 -ClassName AntiVirusProduct)
    if ($providers.Count -ne 1 -or $providers[0].instanceGuid -ne '{D68DDC3A-831F-4fae-9E44-DA132C1ACF46}') { throw 'Additional, missing or unrecognized antivirus provider: support action declined' }
    Load 'Defender'
    $s = Get-MpComputerStatus
    if ($s.AMServiceEnabled -isnot [bool] -or $s.AntivirusEnabled -isnot [bool] -or !$s.AMServiceEnabled -or !$s.AntivirusEnabled -or $s.AMRunningMode -cne 'Normal') { throw 'Defender is not confirmed active in Normal mode' }
    # Tamper protection is never disabled or altered; supported update/scan
    # operations do not change preferences. No source override or executable download.
    switch -CaseSensitive ($supportId) {
        'defender_update' { $null = Update-MpSignature -ErrorAction Stop }
        'defender_quickscan' { $null = Start-MpScan -ScanType QuickScan -ErrorAction Stop }
        default { throw 'Unknown support operation' }
    }
    # This acknowledges command return, not scan completion or threat absence.
    Emit @{ok=$true}
} catch { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }
