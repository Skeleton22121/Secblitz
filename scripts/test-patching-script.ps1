# Cross-platform parsing and mocked WUA boundaries ONLY. No Windows updates,
# metadata searches, COM activation, downloads, installs or registry probes run.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
$root = [IO.Path]::GetFullPath([IO.Path]::Combine($PSScriptRoot,'..'))
$source = [IO.File]::ReadAllText([IO.Path]::Combine($root,'src/patching/wua.ps1'))
$platform = [IO.File]::ReadAllText([IO.Path]::Combine($root,'src/platform/backend.ps1'))
$delimiter = "`ntry {`n    switch -CaseSensitive (`$action) {"
$parts = $platform.Split(@($delimiter), [StringSplitOptions]::None)
if ($parts.Count -ne 2) { throw 'Policy helper boundary changed' }
$tokens=$null; $errors=$null
$null = [Management.Automation.Language.Parser]::ParseInput("`$inputJson=`$null`n$($parts[0])`n$source",[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
$ast = [Management.Automation.Language.Parser]::ParseInput($source,[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($f in @($ast.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.FunctionDefinitionAst] })) {
    . ([scriptblock]::Create($f.Extent.Text))
}
function Assert($condition,[string]$message) { if (!$condition) { throw $message } }
function Reject([scriptblock]$body,[string]$message) { $failed=$false; try { & $body } catch { $failed=$true }; Assert $failed $message }
function New-Object {
    param([string]$ComObject)
    switch ($ComObject) {
        'Microsoft.Update.UpdateColl' { return ,([Collections.Generic.List[object]]::new()) }
        'Microsoft.Update.ServiceManager' { return $script:sourceManager }
        default { throw "Unexpected COM activation in test: $ComObject" }
    }
}
$script:sourceManager = [pscustomobject]@{Services=@()}
function FixtureSource {
    return [pscustomobject]@{ServiceID='9482f4b4-e343-43b6-b170-9a65bc822c77';IsManaged=$false;IsDefaultAUService=$true;IsRegisteredWithAU=$true}
}
foreach ($case in @('valid','wsus','third_party_default','managed_extra','duplicate','two_defaults','unknown_managed','unknown_default','not_registered','empty')) {
    $s=FixtureSource; $script:sourceManager.Services=@($s)
    switch ($case) {
        'wsus' { $s.IsManaged=$true }
        'third_party_default' { $s.ServiceID='11111111-1111-4111-8111-111111111111' }
        'managed_extra' { $extra=FixtureSource; $extra.ServiceID='11111111-1111-4111-8111-111111111111';$extra.IsManaged=$true;$extra.IsDefaultAUService=$false;$script:sourceManager.Services += $extra }
        'duplicate' { $script:sourceManager.Services += (FixtureSource) }
        'two_defaults' { $extra=FixtureSource;$extra.ServiceID='11111111-1111-4111-8111-111111111111';$script:sourceManager.Services += $extra }
        'unknown_managed' { $s.IsManaged=$null }
        'unknown_default' { $s.IsDefaultAUService='true' }
        'not_registered' { $s.IsRegisteredWithAU=$false }
        'empty' { $script:sourceManager.Services=@() }
    }
    if ($case -eq 'valid') { PatchSource }
    else { Reject { PatchSource } "Unsafe/unknown source accepted: $case" }
}
# Run the actual shared management functions with local OS/registry providers
# replaced. This is gate behavior coverage, not native RSOP/WSUS validation.
$policyAst=[Management.Automation.Language.Parser]::ParseInput($parts[0],[ref]$tokens,[ref]$errors)
foreach ($f in @($policyAst.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.FunctionDefinitionAst] })) { . ([scriptblock]::Create($f.Extent.Text)) }
function Load([string]$name) {}
function QueryMdmRegistration {
    return @{result=0;registered=$(if ($script:policyCase -eq 'mdm') { 1 } elseif ($script:policyCase -eq 'mdm_unknown') { $null } else { 0 })}
}
function PolicyValues([string]$path) { return @{} }
function HasValues([string]$path) {
    if ($script:policyCase -eq 'registry_error') { throw 'Unreadable policy' }
    return (($script:policyCase -eq 'wsus_policy' -and $path -eq 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate') -or
        ($script:policyCase -eq 'user_policy' -and $path -eq 'HKCU:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate'))
}
function Test-Path { param($LiteralPath) return ($script:policyCase -eq 'gpo_artifact' -and $LiteralPath -like '*Registry.pol') }
function Get-CimInstance {
    param($ClassName,$Namespace)
    if ($Namespace) {
        if ($script:policyCase -eq 'rsop_error') { throw 'Unknown resultant policy' }
        if ($script:policyCase -eq 'rsop_setting' -and $ClassName -eq 'RSOP_PolicySetting') { return [pscustomobject]@{id='configured'} }
        return @()
    }
    if ($ClassName -eq 'Win32_OperatingSystem') { return [pscustomobject]@{ProductType=1;BuildNumber='26100'} }
    if ($ClassName -eq 'Win32_ComputerSystem') { return [pscustomobject]@{PartOfDomain=$(if ($script:policyCase -eq 'domain') { $true } elseif ($script:policyCase -eq 'domain_unknown') { $null } else { $false })} }
    throw 'Unexpected CIM request in test'
}
$env:SystemRoot='C:\Windows'
$script:sourceManager.Services=@((FixtureSource))
foreach ($script:policyCase in @('valid','domain','domain_unknown','mdm','mdm_unknown','wsus_policy','user_policy','registry_error','rsop_error','rsop_setting','gpo_artifact')) {
    if ($script:policyCase -eq 'valid') { PatchManagement }
    else { Reject { PatchManagement } "Managed/unknown policy accepted: $script:policyCase" }
}
function FixtureUpdate {
    $family = [pscustomobject]@{CategoryID='6964aab4-c5b5-43bd-a17d-ffb4346a8e1d';Parent=$null}
    return [pscustomobject]@{
        Type=1; IsBeta=$false; IsHidden=$false; IsInstalled=$false; BrowseOnly=$false; AutoSelectOnWebSites=$true
        InstallationBehavior=[pscustomobject]@{CanRequestUserInput=$false;Impact=0;RebootBehavior=2}
        Identity=[pscustomobject]@{UpdateID='11111111-1111-4111-8111-111111111111';RevisionNumber=7}
        Title='Windows security update'; Description='Reviewed metadata'; KBArticleIDs=@('1234567')
        Categories=@([pscustomobject]@{CategoryID='0fa1201d-4330-4fa8-8ae9-b877473b6441';Parent=$family})
        BundledUpdates=@(); EulaText='Exact license'; EulaAccepted=$true; IsDownloaded=$true
        MaxDownloadSize=[uint64]4096; LastDeploymentChangeTime=[DateTime]::SpecifyKind([DateTime]'2026-10-01',[DateTimeKind]::Utc)
        MsrcSeverity='Critical'; HandlerID='WUA fixture'
    }
}
$u = FixtureUpdate
Assert (PatchEligible $u $true) 'Eligible fixture rejected'
foreach ($case in @('driver','beta','preview','feature','optional','not_auto','installed','interactive','exclusive','non_windows','upgrade','feature_pack','unknown_impact','unknown_reboot','unknown_type')) {
    $u = FixtureUpdate
    switch ($case) {
        'driver' { $u.Type=2 }
        'beta' { $u.IsBeta=$true }
        'preview' { $u.Title='Security Update Preview' }
        'feature' { $u.Title='Feature update to Windows' }
        'optional' { $u.BrowseOnly=$true }
        'not_auto' { $u.AutoSelectOnWebSites=$false }
        'installed' { $u.IsInstalled=$true }
        'interactive' { $u.InstallationBehavior.CanRequestUserInput=$true }
        'exclusive' { $u.InstallationBehavior.Impact=2 }
        'non_windows' { $u.Categories[0].Parent=$null }
        'upgrade' { $u.Categories += [pscustomobject]@{CategoryID='3689bdc8-b205-4af4-8d4a-a63924c5e9d5';Parent=$null} }
        'feature_pack' { $u.Categories += [pscustomobject]@{CategoryID='b54e7d24-7add-428f-8b75-90a396fa584f';Parent=$null} }
        'unknown_impact' { $u.InstallationBehavior.Impact=$null }
        'unknown_reboot' { $u.InstallationBehavior.RebootBehavior=$null }
        'unknown_type' { $u.Type=$true }
    }
    Assert (!(PatchEligible $u $true)) "Unsafe category accepted: $case"
}
foreach ($case in @('non_windows','driver','preview','exclusive','optional')) {
    $u=FixtureUpdate;$child=FixtureUpdate;$child.Identity.UpdateID='22222222-2222-4222-8222-222222222222'
    switch ($case) {
        'non_windows' { $child.Categories=@() }
        'driver' { $child.Type=2 }
        'preview' { $child.Title='Cumulative Update Preview' }
        'exclusive' { $child.InstallationBehavior.Impact=2 }
        'optional' { $child.BrowseOnly=$true }
    }
    $u.BundledUpdates=@($child)
    Reject { $null=PatchMetadata $u 0 ([Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)) } "Unsafe bundle accepted: $case"
}
$u=FixtureUpdate
$metadata=PatchMetadata $u 0 ([Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal))
Assert ($metadata.categories.Count -eq 2 -and $metadata.categories[0] -is [string]) 'Category array nested or lost'
$expected = PatchJson $metadata | ConvertFrom-Json
Assert ((PatchJson $metadata) -ceq (PatchJson $expected)) 'Typed JSON metadata unstable'
$script:currentUpdate = $u
$script:criteria = ''
function PatchSearcher($session,[bool]$online) { return [pscustomobject]@{Online=$online;IncludePotentiallySupersededUpdates=$false} }
function PatchSearch($searcher,[string]$criteria) {
    $script:criteria=$criteria
    $list=[Collections.Generic.List[object]]::new(); $list.Add($script:currentUpdate); return ,$list
}
$request=[pscustomobject]@{plan=[pscustomobject]@{updates=@($expected)};not_before=[uint64]1;expires_at=[uint64]2000}
$script:policyCase='valid'
$script:now=[uint64]1000
function PatchNow { return $script:now }
$exact = PatchExact ([pscustomobject]@{})
Assert ($exact.Count -eq 1 -and !$script:criteria.Contains('RevisionNumber=') -and $script:criteria.Contains("UpdateID='11111111-1111-4111-8111-111111111111'")) 'Current-revision query/exact collection mismatch'
foreach ($case in @('title','description','eula','revision','bundle','category','kb','size','timestamp','severity','handler','reboot_behavior','unknown_eula','unknown_date')) {
    $script:currentUpdate=FixtureUpdate
    switch ($case) {
        'title' { $script:currentUpdate.Title='Changed title' }
        'description' { $script:currentUpdate.Description='Changed description' }
        'eula' { $script:currentUpdate.EulaText='New license' }
        'revision' { $script:currentUpdate.Identity.RevisionNumber=8 }
        'bundle' { $child=FixtureUpdate; $child.Identity.UpdateID='22222222-2222-4222-8222-222222222222'; $script:currentUpdate.BundledUpdates=@($child) }
        'category' { $script:currentUpdate.Categories[0].Parent=$null }
        'kb' { $script:currentUpdate.KBArticleIDs=@('7654321') }
        'size' { $script:currentUpdate.MaxDownloadSize=[uint64]4097 }
        'timestamp' { $script:currentUpdate.LastDeploymentChangeTime=$script:currentUpdate.LastDeploymentChangeTime.AddSeconds(1) }
        'severity' { $script:currentUpdate.MsrcSeverity='Important' }
        'handler' { $script:currentUpdate.HandlerID='Different handler' }
        'reboot_behavior' { $script:currentUpdate.InstallationBehavior.RebootBehavior=1 }
        'unknown_eula' { $script:currentUpdate.EulaText=$null }
        'unknown_date' { $script:currentUpdate.LastDeploymentChangeTime='2026-10-01' }
    }
    Reject { $null=PatchExact ([pscustomobject]@{}) } "Changed $case accepted"
}
$script:now=[uint64]1000
function PatchNow { return $script:now }
foreach ($times in @(@(1000,1001),@(1000,1000),@(1001,1100),@(1,999))) {
    $request.not_before=[uint64]$times[0]; $request.expires_at=[uint64]$times[1]
    if ($times[0] -eq 1000 -and $times[1] -eq 1001) { PatchDeadline }
    else { Reject { PatchDeadline } 'Expired/future permit accepted' }
}
# Evaluate actual dispatcher body with mocked mutation objects. Guards and exact
# metadata remain real; no OS command can run through these provider replacements.
$try=@($ast.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.TryStatementAst] })
Assert ($try.Count -eq 1) 'Ambiguous dispatcher'
$bodyText=$try[0].Body.Extent.Text
$body=[scriptblock]::Create($bodyText.Substring(1,$bodyText.Length-2))
function PatchManagement { if ($script:managed) { throw 'Managed' } }
function PatchReady($session,[uint64]$bytes) {
    PatchManagement
    if ($script:blocked) { throw 'AC/metered/reboot readiness blocked' }
    if ($script:expireInGate) { $script:now=[uint64]2000 }
    $script:gates++
    if ($script:mutateAfterExact -and $script:gates -ge 3) { $script:currentUpdate.EulaText='Changed after exact selection' }
    if ($script:sourceLate -and $script:gates -ge 3) { $script:sourceManager.Services[0].IsManaged=$true }
}
function PatchSession { return $script:session }
function PatchReboot { return $script:reboot }
$script:session=[pscustomobject]@{}
$script:session | Add-Member ScriptMethod CreateUpdateDownloader { return $script:downloader }
$script:session | Add-Member ScriptMethod CreateUpdateInstaller { return $script:installer }
$script:downloader=[pscustomobject]@{Updates=$null;Priority=0}
$script:downloader | Add-Member ScriptMethod Download { $script:downloads++; return $script:result }
$script:installer=[pscustomobject]@{Updates=$null;AllowSourcePrompts=$true;ForceQuiet=$false;RebootRequiredBeforeInstallation=$false;IsBusy=$false}
$script:installer | Add-Member ScriptMethod Install { $script:installs++; return $script:result }
$script:result=[pscustomobject]@{ResultCode=2}
$script:result | Add-Member ScriptMethod GetUpdateResult { param($i); return [pscustomobject]@{ResultCode=2} }
foreach ($action in @('download','install')) {
    foreach ($case in @('valid','expired','expired_after_gate','managed','blocked','metadata_changed','metadata_late','eula_late','source_late','missing_payload','missing_eula','partial','unknown_result','unknown_reboot_before')) {
        $request=[pscustomobject]@{action=$action;plan=[pscustomobject]@{updates=@($expected)};not_before=[uint64]1;expires_at=[uint64]2000}
        $script:currentUpdate=FixtureUpdate
        $script:currentUpdate | Add-Member ScriptMethod AcceptEula { $script:eulas++; $this.EulaAccepted=$true }
        $script:downloads=0; $script:installs=0; $script:managed=$false; $script:blocked=$false; $script:reboot=$false
        $script:eulas=0; $script:expireInGate=$false; $script:now=[uint64]1000
        $script:gates=0; $script:mutateAfterExact=$false
        $script:sourceLate=$false;$script:sourceManager.Services=@((FixtureSource))
        $script:result.ResultCode=2
        $script:installer.RebootRequiredBeforeInstallation=$false
        switch ($case) {
            'expired' { $request.expires_at=[uint64]1000 }
            'expired_after_gate' { $script:expireInGate=$true }
            'managed' { $script:managed=$true }
            'blocked' { $script:blocked=$true }
            'metadata_changed' { $script:currentUpdate.Description='Unexpected change' }
            'metadata_late' { $script:mutateAfterExact=$true }
            'eula_late' { $script:mutateAfterExact=$true;$script:currentUpdate.EulaAccepted=$false }
            'source_late' { $script:sourceLate=$true }
            'missing_payload' { $script:currentUpdate.IsDownloaded=$false }
            'missing_eula' { $script:currentUpdate.EulaAccepted=$false }
            'partial' { $script:result.ResultCode=3 }
            'unknown_result' { $script:result.ResultCode='2' }
            'unknown_reboot_before' { $script:installer.RebootRequiredBeforeInstallation=$null }
        }
        $failed=$false; try { $null=& $body } catch { $failed=$true }
        $submitted=($script:downloads + $script:installs)
        if ($case -eq 'eula_late') { Assert ($script:eulas -eq 0) 'Changed/unreviewed EULA was accepted after readiness' }
        if ($case -eq 'valid' -or ($case -in @('missing_eula','unknown_reboot_before') -and $action -eq 'download')) {
            Assert (!$failed -and $submitted -eq 1) "Valid $action was not submitted: $Error"
            Assert ($script:eulas -eq [int]($case -eq 'missing_eula')) 'Reviewed EULA acceptance mismatch'
        }
        elseif ($case -in @('partial','unknown_result') -or ($case -eq 'missing_payload' -and $action -eq 'download')) { Assert ($failed -and $submitted -eq 1) 'Incomplete result accepted' }
        else { Assert ($failed -and $submitted -eq 0) "Unsafe submission: $action/$case" }
    }
}
# Independent verification uses installed search and exact returned identity.
$script:currentUpdate=FixtureUpdate; $script:currentUpdate.IsInstalled=$true; $script:reboot=$true
$verification=PatchVerify $script:session
Assert ($verification.installed.Count -eq 1 -and $verification.reboot_pending -and $script:criteria.Contains('IsInstalled=1')) 'Independent verification failed'
$script:currentUpdate.Identity.RevisionNumber=8
Reject { $null=PatchVerify $script:session } 'Wrong installed revision accepted'
$script:currentUpdate=FixtureUpdate; $script:currentUpdate.IsInstalled=$true
$child=FixtureUpdate; $child.Identity.UpdateID='22222222-2222-4222-8222-222222222222'; $child.IsInstalled=$false
$children=[Collections.Generic.List[object]]::new(); $children.Add($child)
$script:currentUpdate.BundledUpdates=$children
$script:currentUpdate.IsInstalled=$false
$bundleExpected=PatchMetadata $script:currentUpdate 0 ([Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal))
$script:currentUpdate.IsInstalled=$true
$request.plan.updates=@((PatchJson $bundleExpected | ConvertFrom-Json))
Assert ((PatchVerify $script:session).installed.Count -eq 1) 'Parent success incorrectly proved bundle installation'
$child.IsInstalled=$true
Assert ((PatchVerify $script:session).installed.Count -eq 2) 'Independent bundle evidence missing'
$child.Identity.RevisionNumber=8
Reject { $null=PatchVerify $script:session } 'Changed installed bundle revision accepted'
'Patching PowerShell parse, eligibility, metadata, permit, submission and verification tests passed (no Windows operations).'
