# Web protection routing rule. One Name Resolution Policy rule for every name
# ("."), listing the filter first. Only a rule that carries both fixed marks
# below is ever read, changed or removed; network adapters are never touched.
# Input:  $env:SECBLITZ_NRPT_MODE     Show | Set | Remove
#         $env:SECBLITZ_NRPT_SERVERS  comma-separated addresses (Set only)
# Output: one JSON line {"servers":[...],"count":N} or {"servers":null,"count":0}
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$displayName = 'Secblitz web protection'
$comment = 'Managed by Secblitz'
$mode = [string]$env:SECBLITZ_NRPT_MODE
if (@('Show', 'Set', 'Remove') -cnotcontains $mode) { throw 'Unknown mode' }

function Get-Ours {
    @(Get-DnsClientNrptRule | Where-Object { $_.DisplayName -eq $displayName -and $_.Comment -eq $comment })
}

if ($mode -eq 'Set') {
    $servers = @()
    foreach ($item in ([string]$env:SECBLITZ_NRPT_SERVERS).Split(',')) {
        $address = $null
        if (-not [System.Net.IPAddress]::TryParse($item.Trim(), [ref]$address)) { throw 'Not an address' }
        $servers += $address.ToString()
    }
    if ($servers.Count -lt 1 -or $servers.Count -gt 12) { throw 'Wrong number of servers' }
}

if ($mode -ne 'Show') {
    foreach ($rule in (Get-Ours)) {
        Remove-DnsClientNrptRule -Name $rule.Name -Force
    }
}
if ($mode -eq 'Set') {
    Add-DnsClientNrptRule -Namespace '.' -NameServers $servers -DisplayName $displayName -Comment $comment | Out-Null
}
if ($mode -ne 'Show') {
    Clear-DnsClientCache
}

$ours = @(Get-Ours)
if ($ours.Count -eq 0) {
    '{"servers":null,"count":0}'
} else {
    $listed = @()
    foreach ($entry in @($ours[0].NameServers)) {
        foreach ($part in ([string]$entry).Split(',')) {
            $part = $part.Trim()
            $address = $null
            if ($part -and [System.Net.IPAddress]::TryParse($part, [ref]$address)) { $listed += $address.ToString() }
        }
    }
    '{"servers":[' + (($listed | ForEach-Object { '"' + $_ + '"' }) -join ',') + '],"count":' + $ours.Count + '}'
}
