# Host-independent syntax/helper tests, NOT native Windows probe evidence.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$count = 0
foreach ($name in @('common.ps1','probes.ps1','browsers.ps1')) {
    $tokens = $null; $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $root $name),[ref]$tokens,[ref]$errors)
    if ($errors.Count -ne 0) { throw ($errors | Out-String) }
    $count++
    if ($name -eq 'common.ps1') {
        foreach ($function in $ast.FindAll({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst]},$false)) {
            if ($function.Name -in @('Unknown','Known','Fact','Prop','Code','Items','Text','UnixTime')) {
                . ([scriptblock]::Create($function.Extent.Text))
            }
        }
    }
}
function Assert($test,[string]$label) { if (!$test) { throw "Assertion failed: $label" }; $script:count++ }
Assert ((Known $false).state -ceq 'Known' -and (Known $false).value -eq $false) 'false is known false'
Assert ((Known $null).state -ceq 'Unknown') 'null is not known'
Assert ((Fact { throw 'test private-path' }).state -ceq 'Unknown') 'errors are unknown'
Assert (!((Fact { throw 'test private-path' }) | ConvertTo-Json -Compress).Contains('private-path')) 'error text redacted'
Assert ((Prop ([pscustomobject]@{a=$true}) 'missing').state -ceq 'Unknown') 'missing property'
Assert ((Prop ([pscustomobject]@{a='false'}) 'a').value -is [string]) 'no boolean coercion'
Assert ((Code ([pscustomobject]@{a='1'}) 'a').state -ceq 'Unknown') 'no numeric string coercion'
Assert ((Code ([pscustomobject]@{a=[uint32]1}) 'a').value -eq 1) 'native numeric enum'
$empty = Items @() $false
Assert ($empty.items -is [array] -and $empty.items.Count -eq 0 -and !$empty.truncated) 'empty inventory array'
Assert ((Known $empty | ConvertTo-Json -Depth 8 -Compress).Contains('"items":[]')) 'empty inventory serialization'
Assert ((Prop ([pscustomobject]@{a=[uint32[]]@(2)}) 'a' | ConvertTo-Json -Compress).Contains('"value":[2]')) 'singleton numeric array retained'
Assert ((Prop ([pscustomobject]@{a=[uint32[]]@()}) 'a' | ConvertTo-Json -Compress).Contains('"value":[]')) 'empty native array retained'
Assert ((UnixTime ([DateTime]::Parse('2023-11-14T22:13:20Z').ToUniversalTime())) -eq 1700000000) 'UTC timestamp'
$rejected=$false
try { $null=Text "bad`ntext" } catch { $rejected=$true }
Assert $rejected 'control characters rejected'
Write-Output "$count syntax/helper assertions passed; no native probes executed."
