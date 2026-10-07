# Host-only mocked gate tests. Never touches the registry, the task scheduler or firmware.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
$tokens = $null; $parseErrors = $null
$shared = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '../platform/secureboot.ps1'), [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw ($parseErrors | Out-String) }
foreach ($node in $shared.EndBlock.Statements) {
    if ($node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -in @('SbIsVirtualMachine', 'SbHasOtherBootLoader')) {
        . ([scriptblock]::Create($node.Extent.Text))
    }
}
$script:checks = 0
function Assert($ok, $message) { if (!$ok) { throw $message }; $script:checks++ }

foreach ($case in @(
    @('Microsoft Corporation', 'Virtual Machine', $true),
    @('VMware, Inc.', 'VMware7,1', $true),
    @('QEMU', 'Standard PC (Q35 + ICH9, 2009)', $true),
    @('innotek GmbH', 'VirtualBox', $true),
    @('Parallels Software International Inc.', 'Parallels Virtual Platform', $true),
    @('LENOVO', '21HD', $false),
    @('Dell Inc.', 'XPS 15 9520', $false),
    @('ASUSTeK COMPUTER INC.', 'ROG Zephyrus G14', $false),
    @('', '', $false)
)) {
    Assert ((SbIsVirtualMachine $case[0] $case[1]) -eq $case[2]) "Wrong virtual PC answer: $($case[0]) / $($case[1])"
}

$windowsOnly = @('Windows Boot Manager', 'path                    \EFI\Microsoft\Boot\bootmgfw.efi', 'path \EFI\BOOT\BOOTX64.EFI')
Assert (!(SbHasOtherBootLoader $windowsOnly)) 'Windows and removable-drive entries are not another system'
Assert (!(SbHasOtherBootLoader @('Pfad                    \efi\microsoft\boot\bootmgfw.efi'))) 'Folder case and a translated label do not matter'
Assert (!(SbHasOtherBootLoader @('path \EFI\Dell\SupportAssist\SupportAssist.efi', 'path \EFI\HP\Diagnostics\x.efi'))) 'Maker tools are not another system'
Assert (SbHasOtherBootLoader ($windowsOnly + 'path \EFI\ubuntu\shimx64.efi')) 'Ubuntu is another system'
Assert (SbHasOtherBootLoader @('path \EFI\fedora\shimx64.efi')) 'Fedora is another system'
Assert (SbHasOtherBootLoader @('path \EFI\refind\refind_x64.efi')) 'A boot menu program is another system'
Assert (!(SbHasOtherBootLoader @())) 'An empty list has no other system'
Assert (!(SbHasOtherBootLoader $null)) 'No list has no other system'

$script = Join-Path $PSScriptRoot 'secureboot.ps1'
$ast = [System.Management.Automation.Language.Parser]::ParseFile($script, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw ($parseErrors | Out-String) }
$operation = $null
foreach ($node in $ast.EndBlock.Statements) {
    if ($node -is [System.Management.Automation.Language.AssignmentStatementAst]) { . ([scriptblock]::Create($node.Extent.Text)) }
    elseif ($node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -ceq 'RenewalRefusal') { . ([scriptblock]::Create($node.Extent.Text)) }
    elseif ($node -is [System.Management.Automation.Language.TryStatementAst]) { $operation = [scriptblock]::Create($node.Extent.Text) }
}
Assert ($null -ne $operation) 'The operation was not found'
$text = Get-Content -LiteralPath $script -Raw
Assert (!$text.Contains('HighConfidenceOptOut') -and !$text.Contains('MicrosoftUpdateManagedOptIn')) 'The opt-out and opt-in values are never touched'
Assert (!$text.Contains('Restart-Computer') -and !$text.Contains('shutdown')) 'The PC is never restarted'

function Load($name) { }
function Emit($value) { $script:reply = $value }
function RegValue($relative, $name) {
    $key = "$relative|$name"
    if ($script:registry.ContainsKey($key)) { return $script:registry[$key] }
    return $null
}
function WriteAvailableUpdates([int]$value) {
    $script:writes.Add($value)
    $script:registry["$sbKey|AvailableUpdates"] = $value
}
function HasSystemEvent([int[]]$ids) {
    $script:eventQueries.Add(($ids -join ','))
    return ($script:scenario -eq 'maker-event')
}
function Confirm-SecureBootUEFI {
    if ($script:scenario -eq 'legacy-bios') { throw [PlatformNotSupportedException]::new('Not supported') }
    if ($script:scenario -eq 'sb-unreadable') { throw 'Access denied' }
    return ($script:scenario -ne 'sb-off')
}
function Get-CimInstance {
    param($ClassName)
    switch ($ClassName) {
        'Win32_OperatingSystem' { return @{ProductType=$(if ($script:scenario -eq 'server') { 3 } else { 1 }); BuildNumber='26100'} }
        'Win32_ComputerSystem' {
            if ($script:scenario -eq 'cim-unreadable') { throw 'Query failed' }
            if ($script:scenario -eq 'vm') { return @(@{Manufacturer='Microsoft Corporation'; Model='Virtual Machine'}) }
            return @(@{Manufacturer='LENOVO'; Model='21HD'})
        }
        default { throw 'Unexpected CIM query' }
    }
}
function Get-ScheduledTask {
    param($TaskPath, $TaskName, $ErrorAction)
    if ($TaskPath -cne '\Microsoft\Windows\PI\' -or $TaskName -cne 'Secure-Boot-Update') { throw 'Wrong task' }
    if ($script:scenario -eq 'task-missing') { return @() }
    return @(@{State=$(if ($script:scenario -eq 'task-disabled') { 'Disabled' } else { 'Ready' })})
}
function Start-ScheduledTask {
    param($TaskPath, $TaskName, $ErrorAction)
    $script:starts++
    if ($script:scenario -eq 'task-start-fails') { throw 'Task refused to start' }
    if ($script:scenario -ne 'slow') { $script:registry["$sbKey|AvailableUpdates"] = 0x5904 }
}
function Start-Sleep { param($Seconds) $script:sleeps++ }
function SbFirmwareBootLines {
    if ($script:scenario -eq 'firmware-unreadable') { throw 'Firmware boot list unavailable' }
    if ($script:scenario -eq 'other-os') { return @('path \EFI\Microsoft\Boot\bootmgfw.efi', 'path \EFI\ubuntu\shimx64.efi') }
    return @('path \EFI\Microsoft\Boot\bootmgfw.efi', 'path \EFI\BOOT\BOOTX64.EFI')
}

$refusals = @{
    'legacy-bios'='not_uefi'; 'sb-unreadable'='unreadable'; 'sb-off'='secure_boot_off'; 'updated'='already_updated'
    'value-set'='already_started'; 'in-progress'='already_started'; 'cim-unreadable'='unreadable'; 'vm'='virtual_machine'
    'maker-event'='maker_blocked'; 'task-missing'='task_missing'; 'task-disabled'='task_disabled'
    'other-os'='other_system'; 'firmware-unreadable'='unreadable'
}
$started = @('allowed', 'value-zero', 'slow', 'task-start-fails')
$count = 0
foreach ($supportId in @('secureboot_renewal', 'defender_update', 'Secureboot_renewal', "secureboot_renewal'; exit")) {
    foreach ($script:scenario in @($refusals.Keys) + $started + @('server')) {
        $script:registry = @{}
        $script:writes = [Collections.Generic.List[int]]::new()
        $script:eventQueries = [Collections.Generic.List[string]]::new()
        $script:starts = 0
        $script:sleeps = 0
        $script:reply = $null
        if ($script:scenario -eq 'updated') { $script:registry["$sbKey\Servicing|UEFICA2023Status"] = 'Updated' }
        if ($script:scenario -eq 'in-progress') { $script:registry["$sbKey\Servicing|UEFICA2023Status"] = 'InProgress' }
        if ($script:scenario -eq 'value-set') { $script:registry["$sbKey|AvailableUpdates"] = 0x4000 }
        if ($script:scenario -eq 'value-zero') { $script:registry["$sbKey|AvailableUpdates"] = 0 }
        $failed = $false
        try { & $operation } catch { $failed = $true }
        $label = "$supportId / $script:scenario"
        $known = $supportId -ceq 'secureboot_renewal'
        if (!$known -or $script:scenario -eq 'server') {
            Assert ($failed -and $null -eq $script:reply -and $script:writes.Count -eq 0 -and $script:starts -eq 0) "Must fail before reading or changing anything: $label"
            $count++
            continue
        }
        Assert (!$failed -and $null -ne $script:reply -and $script:reply.ok -eq $true) "Must answer: $label"
        if ($refusals.ContainsKey($script:scenario)) {
            Assert ($script:reply.result -ceq 'refused' -and $script:reply.reason -ceq $refusals[$script:scenario]) "Wrong refusal: $label"
            Assert ($script:writes.Count -eq 0 -and $script:starts -eq 0) "A refusal must change nothing: $label"
        } else {
            Assert ($script:reply.result -ceq 'started') "Must start: $label"
            Assert ($script:writes.Count -eq 1 -and $script:writes[0] -eq 0x5944) "Must write 0x5944 once: $label"
            Assert ($script:eventQueries.Count -eq 1 -and $script:eventQueries[0] -ceq '1032,1795,1796,1802,1803') "Wrong failure events: $label"
            Assert ($script:starts -eq 1) "Must start the task once: $label"
            $confirmed = $script:scenario -notin @('slow', 'task-start-fails')
            Assert ($script:reply.confirmed -eq $confirmed) "Wrong confirmation: $label"
            Assert ($script:reply.task_started -eq ($script:scenario -ne 'task-start-fails')) "Wrong task answer: $label"
            if ($script:scenario -eq 'slow') { Assert ($script:sleeps -eq 30) "Must wait up to 60 seconds: $label" }
            if ($script:scenario -eq 'task-start-fails') { Assert ($script:sleeps -eq 0) "Must not wait for a task that did not start: $label" }
            if ($script:scenario -eq 'allowed') { Assert ($script:reply.available_updates -eq 0x5904) "Must report the stepped-down value: $label" }
        }
        $count++
    }
}
Write-Output "$count mocked renewal gate cases and $script:checks assertions passed"
