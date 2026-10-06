# Support operations only; policy helpers come from the compiled backend.
try {
    if ($supportId -cnotin @('defender_update','defender_quickscan','defender_remove_threats')) { throw 'Unknown support operation' }
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
    $reply = @{ok=$true}
    switch -CaseSensitive ($supportId) {
        'defender_update' { $null = Update-MpSignature -ErrorAction Stop }
        'defender_quickscan' { $null = Start-MpScan -ScanType QuickScan -ErrorAction Stop }
        'defender_remove_threats' {
            # Defender's own remediation, for the threats it already reports as
            # active. It moves what it removes to quarantine, where Windows
            # Security can restore it. Counts come from Defender, before and after.
            $found = @(Get-MpThreat -ErrorAction Stop | Where-Object { $_.IsActive -eq $true }).Count
            $left = $found
            if ($found -gt 0) {
                Remove-MpThreat -ErrorAction Stop
                for ($attempt = 0; $attempt -lt 15; $attempt++) {
                    $left = @(Get-MpThreat -ErrorAction Stop | Where-Object { $_.IsActive -eq $true }).Count
                    if ($left -eq 0) { break }
                    Start-Sleep -Seconds 2
                }
            }
            $reply = @{ok=$true; found=[int]$found; removed=[int][Math]::Max(0, $found - $left); left=[int]$left}
        }
        default { throw 'Unknown support operation' }
    }
    # This acknowledges command return, not scan completion or threat absence.
    Emit $reply
} catch { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }
