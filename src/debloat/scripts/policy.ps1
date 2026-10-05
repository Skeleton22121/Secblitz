# Tells Windows not to push consumer / suggested apps (machine-wide policy).
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$key = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\CloudContent'
if (-not (Test-Path -LiteralPath $key)) { New-Item -Path $key -Force | Out-Null }
New-ItemProperty -LiteralPath $key -Name 'DisableWindowsConsumerFeatures' -Value 1 -PropertyType DWord -Force | Out-Null
ConvertTo-Json -InputObject ([ordered]@{ ok = $true }) -Compress
