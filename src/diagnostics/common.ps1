$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$WarningPreference = 'SilentlyContinue'
$InformationPreference = 'SilentlyContinue'
Set-StrictMode -Version 2
$moduleRoot = [IO.Path]::Combine($env:SystemRoot, 'System32\WindowsPowerShell\v1.0\Modules')
$env:PSModulePath = $moduleRoot
$PSModuleAutoLoadingPreference = 'None'
# Module names below are compiled constants; no module or command comes from data.
function Load([string]$name) {
    # The inbox LocalAccounts module uses a versioned directory on Windows.
    # Keep this exact path aligned with the native launcher's pinned manifest.
    $relative = if ($name -ceq 'Microsoft.PowerShell.LocalAccounts') { 'Microsoft.PowerShell.LocalAccounts\1.0.0.0\Microsoft.PowerShell.LocalAccounts.psd1' } else { "$name\$name.psd1" }
    $null = Import-Module ([IO.Path]::Combine($moduleRoot, $relative)) -ErrorAction Stop
}
Load 'Microsoft.PowerShell.Utility'
Load 'Microsoft.PowerShell.Management'
function Unknown { return @{state='Unknown';value='Unavailable'} }
function Known($value) {
    if ($null -eq $value) { return (Unknown) }
    return @{state='Known';value=$value}
}
function Fact([scriptblock]$read) {
    try { return (Known (& $read)) } catch { return (Unknown) }
}
function Prop($object, [string]$name) {
    if ($null -eq $object -or $null -eq $object.PSObject.Properties[$name]) { return (Unknown) }
    return (Known $object.$name)
}
function Code($object, [string]$name) {
    if ($null -eq $object -or $null -eq $object.PSObject.Properties[$name] -or $null -eq $object.$name) { return (Unknown) }
    # Explicit numeric conversion is limited to documented native enum properties.
    $value = $object.$name
    # Storage CDXML adapts HealthStatus to the string "Healthy". The actual
    # typed CIM value is UInt16 0; use it rather than parsing display strings.
    if ($null -ne $object.PSObject.Properties['CimInstanceProperties']) {
        $raw = $object.CimInstanceProperties[$name]
        if ($null -ne $raw) { $value = $raw.Value }
    }
    if ($value -is [Enum] -or $value -is [byte] -or $value -is [uint16] -or $value -is [uint32] -or $value -is [int]) { return (Known ([long]$value)) }
    return (Unknown)
}
function Items($items, [bool]$truncated = $false) { return @{items=@($items);truncated=$truncated} }
function Text($value) {
    if ($value -isnot [string] -or $value.Length -gt 160 -or $value -match '[\x00-\x1f\x7f]') { throw 'Invalid text' }
    return $value
}
function UnixTime($date) {
    if ($date -isnot [DateTime]) { throw 'Missing timestamp' }
    return ([DateTimeOffset]$date.ToUniversalTime()).ToUnixTimeSeconds()
}
function Cim([string]$class, [string]$namespace = 'root\cimv2') {
    Load 'CimCmdlets'
    return @(Get-CimInstance -Namespace $namespace -ClassName $class -OperationTimeoutSec 5)
}
function RegBool([string]$path, [string]$name, [int]$trueValue = 1) {
    return Fact {
        $key = Get-Item -LiteralPath $path
        if ($key.GetValueKind($name) -ne [Microsoft.Win32.RegistryValueKind]::DWord) { throw 'Wrong registry type' }
        $value = $key.GetValue($name)
        if ($value -notin @(0,1)) { throw 'Unknown registry value' }
        return ($value -eq $trueValue)
    }
}
function PolicyValues([string]$relative) {
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($relative, $false)
    if ($null -eq $key) { return $false }
    try { return ($key.ValueCount -gt 0) } finally { $key.Dispose() }
}
function ChildIndicator([string]$relative) {
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($relative, $false)
    if ($null -eq $key) { return $false }
    try { return ($key.SubKeyCount -gt 0) } finally { $key.Dispose() }
}
function MdmRegistered {
    # In-memory P/Invoke stub, no Add-Type/csc, no UPN/tenant collection.
    $assembly = [AppDomain]::CurrentDomain.DefineDynamicAssembly([Reflection.AssemblyName]::new('Secblitz.DiagnosticsMdm'), [Reflection.Emit.AssemblyBuilderAccess]::Run)
    $module = $assembly.DefineDynamicModule('Secblitz.DiagnosticsMdm')
    $type = $module.DefineType('Secblitz.DiagnosticsMdm', [Reflection.TypeAttributes]'Public, Abstract, Sealed')
    $method = $type.DefinePInvokeMethod('IsDeviceRegisteredWithManagement', [IO.Path]::Combine($env:SystemRoot, 'System32\MDMRegistration.dll'), 'IsDeviceRegisteredWithManagement', [Reflection.MethodAttributes]'Public, Static, PinvokeImpl', [Reflection.CallingConventions]::Standard, [int], [Type[]]@([int].MakeByRefType(), [uint32], [IntPtr]), [Runtime.InteropServices.CallingConvention]::Winapi, [Runtime.InteropServices.CharSet]::Unicode)
    $method.SetImplementationFlags($method.GetMethodImplementationFlags() -bor [Reflection.MethodImplAttributes]::PreserveSig)
    $null = $type.CreateType()
    $registered = [int]0
    $hr = [Secblitz.DiagnosticsMdm]::IsDeviceRegisteredWithManagement([ref]$registered, 0, [IntPtr]::Zero)
    if ($hr -ne 0 -or $registered -notin @(0,1)) { throw 'MDM state unavailable' }
    return ($registered -eq 1)
}
