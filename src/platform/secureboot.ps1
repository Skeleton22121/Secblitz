# Read the same way by the Secure Boot probe and by the renewal action.
function SbIsVirtualMachine([string]$manufacturer, [string]$model) {
    return (("$manufacturer $model") -match '(?i)virtual machine|virtualbox|vmware|qemu|\bkvm\b|\bxen\b|bochs|parallels|hyper-v|bhyve|innotek')
}
function SbHasOtherBootLoader([string[]]$lines) {
    # Folder names are not translated, unlike the labels bcdedit prints.
    $own = @('microsoft', 'boot', 'dell', 'hp', 'hewlettpackard', 'lenovo', 'asus', 'acer', 'msi', 'samsung', 'toshiba')
    foreach ($line in @($lines)) {
        if ($line -match '(?i)\\EFI\\([^\\\s]+)\\') {
            if ($Matches[1] -inotin $own) { return $true }
        }
    }
    return $false
}
function SbFirmwareBootLines {
    $exe = [IO.Path]::Combine($env:SystemRoot, 'System32\bcdedit.exe')
    $lines = @(& $exe /enum firmware 2>$null)
    if ($LASTEXITCODE -ne 0) { throw 'Firmware boot list unavailable' }
    return $lines
}
