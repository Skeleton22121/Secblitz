# Private, non-mutating registry fixtures. Real production reads/writes operate
# on an in-memory key double; no backend initialization/dispatcher is executed.
param([string]$BackendPath = (Join-Path $PSScriptRoot 'backend.ps1'))
$ErrorActionPreference='Stop'
Set-StrictMode -Version 2
$tokens=$null; $errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($BackendPath,[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($node in $ast.EndBlock.Statements) {
    if ($node -is [Management.Automation.Language.FunctionDefinitionAst]) { . ([scriptblock]::Create($node.Extent.Text)) }
}
$script:checks=0
function Assert($ok,$message) { if (!$ok) { throw $message }; $script:checks++ }
function Reject([scriptblock]$operation,$message) {
    $caught=$null
    try { & $operation } catch { $caught=$_.Exception.Message }
    Assert ($null -ne $caught -and $caught -like "*$message*") "Expected '$message', got '$caught'"
}
function Gate($id) { $script:gates++; if ($script:blocked) { throw 'management gate' } }
function Test-Path { param($LiteralPath,$ErrorAction); if ($script:readDenied) { throw 'registry access denied' }; return $script:keyExists }
function Get-Item { param($LiteralPath,$ErrorAction); if ($script:readDenied) { throw 'registry access denied' }; return $script:key }
function New-ItemProperty {
    param($LiteralPath,$Name,$PropertyType,$Value,[switch]$Force,$ErrorAction)
    Assert ($LiteralPath -ceq $script:spec.path -and $Name -ceq $script:spec.name -and $PropertyType -ceq 'DWord' -and $Value -is [int]) 'Unexpected registry setter arguments'
    $script:writes++
    if (!$script:ignoreWrite) { $script:entries[$Name]=@{kind=[Microsoft.Win32.RegistryValueKind]::DWord;value=$Value} }
}
function Remove-ItemProperty {
    param($LiteralPath,$Name,$ErrorAction)
    Assert ($LiteralPath -ceq $script:spec.path -and $Name -ceq $script:spec.name) 'Unexpected registry deletion'
    $script:writes++
    if (!$script:ignoreWrite) { $script:entries.Remove($Name) }
}
function Reset {
    $script:keyExists=$true; $script:readDenied=$false; $script:blocked=$false
    $script:ignoreWrite=$false; $script:writes=0; $script:gates=0
    $script:entries=@{Unrelated=@{kind=[Microsoft.Win32.RegistryValueKind]::String;value='preserve'}}
    $script:key=[pscustomobject]@{SubKeyCount=0}
    $script:key | Add-Member ScriptMethod GetValueNames { return @($script:entries.Keys) }
    $script:key | Add-Member ScriptMethod GetValueKind { param($name); return $script:entries[$name].kind }
    $script:key | Add-Member ScriptMethod GetValue { param($name); if ($name -eq 'DefaultPassword') { throw 'SECRET DATA WAS READ' }; return $script:entries[$name].value }
}
$expectedSpecs=@{
    'installer.always_install_elevated'=@('HKLM:\SOFTWARE\Policies\Microsoft\Windows\Installer','AlwaysInstallElevated',0,1)
    'lsa.restrict_anonymous_sam'=@('HKLM:\SYSTEM\CurrentControlSet\Control\Lsa','RestrictAnonymousSAM',1,0)
    'lsa.limit_blank_password_use'=@('HKLM:\SYSTEM\CurrentControlSet\Control\Lsa','LimitBlankPasswordUse',1,0)
    'wdigest.use_logon_credential'=@('HKLM:\SYSTEM\CurrentControlSet\Control\SecurityProviders\WDigest','UseLogonCredential',0,1)
}
foreach ($id in @('installer.always_install_elevated','lsa.restrict_anonymous_sam','lsa.limit_blank_password_use','wdigest.use_logon_credential')) {
    $script:spec=PrivilegeRegistrySpec $id
    $expected=$expectedSpecs[$id]
    Assert ($spec.path -ceq $expected[0] -and $spec.name -ceq $expected[1] -and $spec.target -eq $expected[2] -and $spec.unsafe -eq $expected[3]) 'Fixed HKLM registry contract changed'
    $target=@{present=$true;value=$spec.target}; $original=@{present=$true;value=$spec.unsafe}
    foreach ($keyExists in @($false,$true)) {
        Reset; $script:keyExists=$keyExists
        $observed=ReadControl $id
        Assert (!$observed.present -and $null -eq $observed.value -and !(PrivilegeRepairable $id $observed)) 'Absence was not preserved'
        Reject { WriteControl $id $target } 'explicitly unsafe'
        Assert ($script:writes -eq 0) 'Created missing registry state'
    }
    foreach ($bad in @('1',[double]1,2,-1,$true,$null)) {
        Reset; $script:entries[$spec.name]=@{kind=[Microsoft.Win32.RegistryValueKind]::DWord;value=$bad}
        Reject { ReadControl $id } 'Invalid binary registry DWORD'
        Reject { WriteControl $id $target } 'Invalid binary registry DWORD'
        Assert ($script:writes -eq 0) 'Overwrote invalid original'
    }
    foreach ($kind in @('String','QWord','Binary','ExpandString','MultiString')) {
        Reset; $script:entries[$spec.name]=@{kind=[Microsoft.Win32.RegistryValueKind]::$kind;value=$spec.unsafe}
        Reject { ReadControl $id } 'not a DWORD'
        Reject { WriteControl $id $target } 'not a DWORD'
        Assert ($script:writes -eq 0) 'Coerced wrong registry type'
    }
    Reset; $script:entries[$spec.name]=@{kind=[Microsoft.Win32.RegistryValueKind]::DWord;value=$spec.unsafe}
    Assert (PrivilegeRepairable $id (ReadControl $id)) 'Explicit unsafe original rejected'
    WriteControl $id $target
    Assert ((ReadControl $id).value -eq $spec.target -and $script:writes -eq 1) 'Repair did not round trip'
    Reject { WriteControl $id $target } 'explicitly unsafe'
    WriteControl $id $original
    Assert ((ReadControl $id).value -eq $spec.unsafe -and $script:writes -eq 2 -and $script:entries.Unrelated.value -ceq 'preserve') 'Exact original/unrelated value not preserved'
    Reject { WriteControl $id $original } 'changed before restore'
    WriteControl $id $target
    WriteControl $id @{present=$false;value=$null}
    Assert (!(ReadControl $id).present -and $script:keyExists -and $script:entries.Count -eq 1) 'Absence rollback deleted key or sibling value'
    Reject { WriteControl $id $original } 'changed before restore'
    foreach ($request in @($target,$original,@{present=$false;value=$null})) {
        Reset; $script:blocked=$true
        Reject { WriteControl $id $request } 'management gate'
        Assert ($script:writes -eq 0 -and $script:gates -eq 1) 'Mutation bypassed gate'
    }
    foreach ($bad in @(@{present=$true;value='1'},@{present=$true;value=2},@{present=$true;value=$null},@{present=$false;value=0},@{present=$true;value=1;path='HKCU'},@{present=$false},@{present=1;value=1})) {
        Reset
        Reject { WriteControl $id $bad } ''
        Assert ($script:writes -eq 0) 'Invalid request reached registry'
    }
    foreach ($request in @($target,$original,@{present=$false;value=$null})) {
        Reset; $script:ignoreWrite=$true
        $before=if ($request.present -and $request.value -eq $spec.target) {$spec.unsafe} else {$spec.target}
        $script:entries[$spec.name]=@{kind=[Microsoft.Win32.RegistryValueKind]::DWord;value=$before}
        Reject { WriteControl $id $request } 'readback did not match'
        Assert ($script:writes -eq 1) 'Readback fixture did not reach setter'
    }
    Reset; $script:readDenied=$true
    Reject { ReadControl $id } 'access denied'
    Reject { WriteControl $id $target } 'access denied'
    Assert ($script:writes -eq 0) 'Registry probe failure became absence'
}
# Own installer value must not become a management veto after repair. Relevant
# other policy is preserved. PolicyManager probes are isolated in this fixture.
function PolicyValues($path) { return @{} }
Reset; $script:spec=PrivilegeRegistrySpec 'installer.always_install_elevated'
function Test-Path { param($LiteralPath,$ErrorAction); return ($LiteralPath -ceq $script:spec.path) }
foreach ($v in @(0,1)) {
    $script:entries=@{AlwaysInstallElevated=@{kind=[Microsoft.Win32.RegistryValueKind]::DWord;value=$v}}
    CheckScopedPolicy 'installer.always_install_elevated'
    Assert $true 'Own Installer preference vetoed management gate'
    $script:entries.DisableMSI=@{kind=[Microsoft.Win32.RegistryValueKind]::DWord;value=0}
    Reject { CheckScopedPolicy 'installer.always_install_elevated' } 'Other machine Installer policy'
}
Reset
function Get-Item {
    param($LiteralPath,$ErrorAction)
    Assert ($LiteralPath -ceq 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon') 'Autologon read used an unexpected path'
    return $script:key
}
$script:key | Add-Member -Force ScriptMethod GetValue {
    param($name)
    if ($name -cne 'AutoAdminLogon') { throw 'Credential or identity value data was read' }
    return $script:entries[$name].value
}
$script:entries.DefaultPassword=@{kind=[Microsoft.Win32.RegistryValueKind]::String;value='must never be read'}
$script:entries.DefaultUserName=@{kind=[Microsoft.Win32.RegistryValueKind]::String;value='identity must never be read'}
$script:entries.DefaultDomainName=@{kind=[Microsoft.Win32.RegistryValueKind]::String;value='domain must never be read'}
$finding=AutoLogonFinding
Assert ($finding.status -eq 'attention' -and $finding.detail -notlike '*must never be read*') 'Autologon secret leaked'
$script:entries.Remove('DefaultPassword')
$script:entries.AutoAdminLogon=@{kind=[Microsoft.Win32.RegistryValueKind]::String;value='1'}
Assert ((AutoLogonFinding).status -eq 'attention') 'Autologon flag was ignored'
$script:entries.AutoAdminLogon.value='0'
Assert ((AutoLogonFinding).status -eq 'info') 'Disabled autologon overstated security status'
$script:entries.Remove('AutoAdminLogon')
Assert ((AutoLogonFinding).status -eq 'info') 'Absent autologon overstated security status'
foreach ($bad in @(@{kind=[Microsoft.Win32.RegistryValueKind]::String;value='yes'},@{kind=[Microsoft.Win32.RegistryValueKind]::DWord;value=1})) {
    $script:entries.AutoAdminLogon=$bad
    Assert ((Finding 'Automatic logon' { AutoLogonFinding }).status -eq 'unknown') 'Malformed autologon configuration was not unknown'
}
Write-Output "Privilege registry fixtures passed: $script:checks checks"
