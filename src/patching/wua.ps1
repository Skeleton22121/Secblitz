# Fixed WUA implementation. Data comes only from typed, protected native state.
# Functions are isolated by the parser-only/mock test harness; dispatch never runs
# in those tests. No installer path, arguments, service registration or reboot API.
function PatchNow { return [uint64]([DateTimeOffset]::UtcNow.ToUnixTimeSeconds()) }
function PatchDeadline {
    $t = PatchNow
    if ($t -lt $request.not_before -or $t -ge $request.expires_at) { throw 'Exact approval expired or clock rollback' }
}
function PatchSource {
    $manager = New-Object -ComObject Microsoft.Update.ServiceManager
    $services = @($manager.Services)
    if ($services.Count -eq 0 -or $services.Count -gt 64) { throw 'Update source enumeration is unknown' }
    $ids = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $defaults = @()
    foreach ($service in $services) {
        $id = ([guid]$service.ServiceID).ToString('D')
        if ($id -eq ([guid]::Empty).ToString('D') -or !$ids.Add($id) -or
            $service.IsManaged -isnot [bool] -or $service.IsManaged -or
            $service.IsDefaultAUService -isnot [bool] -or $service.IsRegisteredWithAU -isnot [bool] -or
            ($service.IsDefaultAUService -and !$service.IsRegisteredWithAU)) { throw 'Managed, duplicate or unknown update source' }
        if ($service.IsDefaultAUService) { $defaults += ,$id }
    }
    if ($defaults.Count -ne 1 -or $defaults[0] -cne '9482f4b4-e343-43b6-b170-9a65bc822c77') { throw 'Default update source is not unmanaged Windows Update' }
}
function PatchManagement {
    # Shared compiled domain/MDM/cloud/RSOP/Update-policy checks; no dispatcher.
    Gate 'permissions.service.wuauserv'
    PatchSource
    # Also veto per-user update policy; same-user token is validated natively.
    if (HasValues 'HKCU:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate') { throw 'User update policy configured' }
}
function PatchReboot {
    $system = New-Object -ComObject Microsoft.Update.SystemInfo
    if ($system.RebootRequired -isnot [bool]) { throw 'Unknown WUA reboot state' }
    $pending = $system.RebootRequired
    foreach ($p in @('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending',
        'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootInProgress',
        'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired')) {
        if (Test-Path -LiteralPath $p) { $pending = $true }
    }
    $key = Get-Item -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager'
    foreach ($name in @('PendingFileRenameOperations','PendingFileRenameOperations2')) {
        if ($null -ne $key.GetValue($name)) { $pending = $true }
    }
    return [bool]$pending
}
function PatchPower {
    # Reflection.Emit creates a fixed kernel32 P/Invoke, with no compiler child.
    if ($null -eq ('Secblitz.PatchPower' -as [type])) {
        $a = [AppDomain]::CurrentDomain.DefineDynamicAssembly([Reflection.AssemblyName]::new('Secblitz.PatchPower'), [Reflection.Emit.AssemblyBuilderAccess]::Run)
        $m = $a.DefineDynamicModule('Secblitz.PatchPower')
        $t = $m.DefineType('Secblitz.PatchPower', [Reflection.TypeAttributes]'Public, Abstract, Sealed')
        $f = $t.DefinePInvokeMethod('GetSystemPowerStatus', [IO.Path]::Combine($env:SystemRoot,'System32\kernel32.dll'), 'GetSystemPowerStatus', [Reflection.MethodAttributes]'Public, Static, PinvokeImpl', [Reflection.CallingConventions]::Standard, [int], [Type[]]@([IntPtr]), [Runtime.InteropServices.CallingConvention]::Winapi, [Runtime.InteropServices.CharSet]::Unicode)
        $f.SetImplementationFlags($f.GetMethodImplementationFlags() -bor [Reflection.MethodImplAttributes]::PreserveSig)
        $null = $t.CreateType()
    }
    $p = [Runtime.InteropServices.Marshal]::AllocHGlobal(12)
    try {
        if ([Secblitz.PatchPower]::GetSystemPowerStatus($p) -eq 0 -or [Runtime.InteropServices.Marshal]::ReadByte($p,0) -ne 1) { throw 'AC power not confirmed' }
    } finally { [Runtime.InteropServices.Marshal]::FreeHGlobal($p) }
}
function PatchQuiescent($session) {
    $installer = $session.CreateUpdateInstaller()
    if ($installer.IsBusy -isnot [bool] -or $installer.IsBusy) { throw 'Servicing busy or unknown' }
    # Process/service probes fail closed. Never stop an external servicing worker.
    $workers = @(Get-Process -ErrorAction Stop | Where-Object { $_.ProcessName -in @('dism','dismhost','sfc','TiWorker','MoUsoCoreWorker','UsoClient','msiexec','setuphost') })
    if ($workers.Count -gt 0) { throw 'Another servicing worker is active' }
}
function PatchReady($session, [uint64]$requiredBytes) {
    PatchManagement
    PatchPower
    if (PatchReboot) { throw 'Pending reboot; owner action required' }
    PatchQuiescent $session
    $profile = [Windows.Networking.Connectivity.NetworkInformation, Windows.Networking.Connectivity, ContentType=WindowsRuntime]::GetInternetConnectionProfile()
    if ($null -eq $profile) { throw 'Network cost unavailable' }
    $cost = $profile.GetConnectionCost()
    if ($null -eq $cost -or [string]$cost.NetworkCostType -cne 'Unrestricted' -or $cost.Roaming -isnot [bool] -or
        $cost.OverDataLimit -isnot [bool] -or $cost.ApproachingDataLimit -isnot [bool] -or
        $cost.Roaming -or $cost.OverDataLimit -or $cost.ApproachingDataLimit) { throw 'Metered/unknown network' }
    $drive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($env:SystemRoot))
    if (!$drive.IsReady -or [uint64]$drive.AvailableFreeSpace -lt (5GB + $requiredBytes)) { throw 'Insufficient system storage' }
}
function PatchSession {
    $s = New-Object -ComObject Microsoft.Update.Session
    $s.ClientApplicationID = 'Secblitz exact reviewed quality updates'
    $s.UserLocale = 1033 # invariant English metadata for conservative title veto
    return $s
}
function PatchSearcher($session, [bool]$online) {
    $searcher = $session.CreateUpdateSearcher()
    $searcher.ServerSelection = 2 # ssWindowsUpdate, never a supplied service/source
    $searcher.Online = $online
    $searcher.CanAutomaticallyUpgradeService = $false
    $searcher.IncludePotentiallySupersededUpdates = $false
    return $searcher
}
function PatchSearch($searcher, [string]$criteria) {
    $result = $searcher.Search($criteria)
    if ($result.ResultCode -isnot [int] -or $result.ResultCode -ne 2 -or
        $result.Warnings.Count -ne 0 -or $result.Updates.Count -lt 0 -or $result.Updates.Count -gt 512) { throw 'Incomplete/oversized WUA search' }
    return ,$result.Updates
}
function PatchCategories($update) {
    $ids = [Collections.Generic.SortedSet[string]]::new([StringComparer]::Ordinal)
    foreach ($category in $update.Categories) {
        $c = $category; $depth = 0
        while ($null -ne $c) {
            $depth++
            if ($depth -gt 8 -or $ids.Count -gt 128) { throw 'Category cap' }
            $null = $ids.Add(([guid]$c.CategoryID).ToString('D'))
            $c = $c.Parent
        }
    }
    return ,@($ids)
}
function PatchEligible($u, [bool]$root) {
    if ($u.Type -isnot [int] -or $u.Type -ne 1 -or $u.IsBeta -isnot [bool] -or $u.IsBeta -or $u.IsHidden -isnot [bool] -or $u.IsHidden) { return $false }
    if ($u.IsInstalled -isnot [bool] -or $u.BrowseOnly -isnot [bool] -or $u.BrowseOnly) { return $false }
    if ($u.InstallationBehavior.CanRequestUserInput -isnot [bool] -or $u.InstallationBehavior.CanRequestUserInput -or
        $u.InstallationBehavior.Impact -isnot [int] -or $u.InstallationBehavior.Impact -ne 0 -or
        $u.InstallationBehavior.RebootBehavior -isnot [int] -or $u.InstallationBehavior.RebootBehavior -notin @(0,1,2)) { return $false }
    if ([string]$u.Title -match '(?i)preview|insider|\bbeta\b|feature update|enablement package|upgrade to') { return $false }
    $cats = PatchCategories $u
    # Every bundle member must positively identify the Windows family too; an
    # unclassified/third-party child cannot borrow its parent's classification.
    if ($cats -notcontains '6964aab4-c5b5-43bd-a17d-ffb4346a8e1d') { return $false }
    if ($cats -contains 'ebfc1fc5-71a4-4f7b-9aca-3b9a503104a0' -or # Drivers
        $cats -contains '3689bdc8-b205-4af4-8d4a-a63924c5e9d5' -or # Upgrades
        $cats -contains 'b54e7d24-7add-428f-8b75-90a396fa584f') { return $false } # Feature packs
    if ($root) {
        if ($u.IsInstalled -or
            $u.AutoSelectOnWebSites -isnot [bool] -or !$u.AutoSelectOnWebSites) { return $false }
        if ($cats -notcontains '6964aab4-c5b5-43bd-a17d-ffb4346a8e1d' -or
            ($cats -notcontains '0fa1201d-4330-4fa8-8ae9-b877473b6441' -and $cats -notcontains 'e6cf1350-c01b-414d-a61f-263d14d133b4')) { return $false }
    }
    return $true
}
function PatchMetadata($u, [int]$depth, $seen) {
    if ($depth -gt 4 -or $seen.Count -ge 128 -or $u.BundledUpdates.Count -gt 32) { throw 'Bundle cap' }
    $id = ([guid]$u.Identity.UpdateID).ToString('D')
    if (!$seen.Add($id) -or $u.Identity.RevisionNumber -isnot [int] -or $u.Identity.RevisionNumber -le 0 -or
        !(PatchEligible $u ($depth -eq 0))) { throw 'Unreviewable/duplicate update or bundle' }
    if (($u.MaxDownloadSize -isnot [decimal] -and $u.MaxDownloadSize -isnot [long] -and
         $u.MaxDownloadSize -isnot [uint64] -and $u.MaxDownloadSize -isnot [int]) -or
        $u.MaxDownloadSize -lt 0 -or $u.MaxDownloadSize -gt 64GB -or
        [decimal]::Truncate([decimal]$u.MaxDownloadSize) -ne $u.MaxDownloadSize) { throw 'Unknown download size' }
    if ($u.EulaText -isnot [string] -or $u.LastDeploymentChangeTime -isnot [DateTime]) { throw 'Unknown EULA or deployment metadata' }
    $children = @()
    foreach ($child in $u.BundledUpdates) { $children += ,(PatchMetadata $child ($depth + 1) $seen) }
    # BundledUpdates is an ORDERED install list. Preserve/revalidate that order.
    $eula = [string]$u.EulaText
    if ([string]$u.Title -eq '' -or $u.Title.Length -gt 4096 -or $u.Description.Length -gt 32768 -or $eula.Length -gt 65536) { throw 'Metadata cap' }
    return [ordered]@{
        identity=[ordered]@{update_id=$id; revision=[uint32]$u.Identity.RevisionNumber}
        title=[string]$u.Title; description=[string]$u.Description
        kb_articles=@($u.KBArticleIDs | Sort-Object); categories=(PatchCategories $u)
        max_download_bytes=[uint64]$u.MaxDownloadSize
        # Tick string avoids ConvertFrom-Json's version-dependent ISO date coercion.
        last_changed=[DateTime]::SpecifyKind($u.LastDeploymentChangeTime,[DateTimeKind]::Utc).Ticks.ToString([Globalization.CultureInfo]::InvariantCulture)
        severity=[string]$u.MsrcSeverity; handler=[string]$u.HandlerID
        reboot_behavior=[uint32]$u.InstallationBehavior.RebootBehavior
        eula=$eula; bundled=$children
    }
}
function PatchJson($value) { return ConvertTo-Json -InputObject $value -Depth 32 -Compress }
function PatchExact($session) {
    $searcher = PatchSearcher $session $true
    $collection = New-Object -ComObject Microsoft.Update.UpdateColl
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($expected in $request.plan.updates) {
        # Validate/format before query construction; title/EULA/etc never enter it.
        $id = ([guid]$expected.identity.update_id).ToString('D')
        $revision = [uint32]$expected.identity.revision
        # Do not request a historical revision from the server. Resolve the
        # currently offered applicable revision, then require the reviewed pair.
        PatchManagement
        PatchDeadline
        $matches = PatchSearch $searcher "IsInstalled=0 and IsHidden=0 and Type='Software' and DeploymentAction='Installation' and UpdateID='$id'"
        if ($matches.Count -ne 1) { throw 'Exact update/revision no longer applicable' }
        $u = $matches.Item(0)
        if (([guid]$u.Identity.UpdateID).ToString('D') -cne $id -or [uint32]$u.Identity.RevisionNumber -ne $revision) { throw 'Current update identity/revision changed' }
        $metadata = PatchMetadata $u 0 $seen
        if ((PatchJson $metadata) -cne (PatchJson $expected)) { throw 'Reviewed metadata or bundle closure changed' }
        $null = $collection.Add($u)
    }
    if ($collection.Count -ne $request.plan.updates.Count -or $collection.Count -eq 0) { throw 'Exact collection mismatch' }
    return ,$collection
}
function PatchBytes($updates) {
    $sum = [uint64]0
    foreach ($u in $updates) {
        $required = [Math]::Max([uint64]$u.max_download_bytes, [uint64](PatchBytes $u.bundled))
        if ($required -gt 64GB -or $sum -gt (64GB - $required)) { throw 'Download cap' }
        $sum += $required
    }
    return $sum
}
function PatchMatch($u, $expected, [int]$depth) {
    $metadata = PatchMetadata $u $depth ([Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal))
    if ((PatchJson $metadata) -cne (PatchJson $expected)) { throw 'Reviewed metadata changed at submission boundary' }
}
function PatchBoundCollection($collection) {
    if ($collection.Count -ne $request.plan.updates.Count) { throw 'Submitted collection changed' }
    for ($i=0; $i -lt $collection.Count; $i++) { PatchMatch $collection.Item($i) $request.plan.updates[$i] 0 }
}
function PatchAccept($u, $expected, $session, [uint64]$bytes, [int]$depth) {
    PatchMatch $u $expected $depth
    # Iterate the reviewed closure, never an expanded live child list. Each
    # object must still match its exact reviewed identity, order and EULA.
    for ($i=0; $i -lt $expected.bundled.Count; $i++) {
        PatchAccept $u.BundledUpdates.Item($i) $expected.bundled[$i] $session $bytes ($depth + 1)
    }
    if ($u.EulaAccepted -isnot [bool]) { throw 'Unknown EULA state' }
    if (!$u.EulaAccepted) {
        PatchReady $session $bytes
        PatchMatch $u $expected $depth
        PatchSource
        PatchDeadline
        $u.AcceptEula() # explicit consent covers precisely the digested EulaText
    }
}
function PatchInstalledTree($u, $expected, [int]$depth) {
    if ($depth -gt 4 -or ([guid]$u.Identity.UpdateID).ToString('D') -cne $expected.identity.update_id -or
        $u.Identity.RevisionNumber -isnot [int] -or [uint32]$u.Identity.RevisionNumber -ne $expected.identity.revision -or $u.IsInstalled -isnot [bool] -or
        $u.BundledUpdates.Count -ne $expected.bundled.Count) { throw 'Independent installed identity/bundle mismatch' }
    if ($u.IsInstalled) { [ordered]@{update_id=$expected.identity.update_id;revision=[uint32]$expected.identity.revision} }
    for ($i=0; $i -lt $u.BundledUpdates.Count; $i++) {
        PatchInstalledTree $u.BundledUpdates.Item($i) $expected.bundled[$i] ($depth + 1)
    }
}
function PatchVerify($session) {
    # Independent NEW local installed-state searches; no use of installation
    # return codes/history as proof, no online download, no source changes.
    $searcher = PatchSearcher $session $false
    $searcher.IncludePotentiallySupersededUpdates = $true
    $installed = @()
    foreach ($expected in $request.plan.updates) {
        $id = ([guid]$expected.identity.update_id).ToString('D'); $rev = [uint32]$expected.identity.revision
        $matches = PatchSearch $searcher "IsInstalled=1 and UpdateID='$id' and RevisionNumber=$rev"
        if ($matches.Count -gt 1) { throw 'Ambiguous installed identity' }
        if ($matches.Count -eq 1) {
            $u = $matches.Item(0)
            if ($u.IsInstalled -isnot [bool] -or !$u.IsInstalled -or ([guid]$u.Identity.UpdateID).ToString('D') -cne $id -or [uint32]$u.Identity.RevisionNumber -ne $rev) { throw 'Installed identity mismatch' }
            # Non-top-level bundle members are not necessarily searchable as
            # standalone deployments. Verify them from this NEW installed-state
            # result, with individual IsInstalled and exact ordered identities.
            $installed += @(PatchInstalledTree $u $expected 0)
        }
    }
    return [ordered]@{installed=$installed;reboot_pending=(PatchReboot);checked_at=(PatchNow)}
}

# PATCHING_DISPATCH: tests load only function ASTs, never this section.
try {
    if ($request.action -cnotin @('discover','download','install','verify')) { throw 'Invalid fixed patching action' }
    PatchManagement
    $session = PatchSession
    if ($request.action -ceq 'verify') {
        PatchQuiescent $session
        PatchJson (PatchVerify $session)
    } elseif ($request.action -ceq 'discover') {
        PatchReady $session 0
        $searcher = PatchSearcher $session $true
        $found = PatchSearch $searcher "IsInstalled=0 and IsHidden=0 and Type='Software' and DeploymentAction='Installation' and BrowseOnly=0 and AutoSelectOnWebSites=1"
        $updates = @(); $metadataBytes = [uint64]0
        foreach ($u in $found) {
            if (PatchEligible $u $true) {
                # Ineligible bundles omit the whole root; they never expand a plan.
                try { $meta = PatchMetadata $u 0 ([Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)) }
                catch { continue }
                $metadataBytes += [uint64][Text.Encoding]::UTF8.GetByteCount((PatchJson $meta))
                if ($updates.Count -ge 32 -or $metadataBytes -gt 4MB) { throw 'Discovery output cap; narrower workflow required' }
                $updates += ,$meta
            }
        }
        PatchJson @($updates | Sort-Object { $_.identity.update_id })
    } else {
        PatchDeadline
        $bytes = PatchBytes $request.plan.updates
        PatchReady $session $bytes
        $exact = PatchExact $session # CURRENT applicability and complete metadata
        PatchReady $session $bytes
        PatchDeadline
        if ($request.action -ceq 'download') {
            for ($i=0; $i -lt $exact.Count; $i++) { PatchAccept $exact.Item($i) $request.plan.updates[$i] $session $bytes 0 }
            $downloader = $session.CreateUpdateDownloader()
            $downloader.Updates = $exact
            $downloader.Priority = 2 # normal, never forced high priority
            PatchReady $session $bytes
            PatchBoundCollection $exact
            PatchSource
            PatchDeadline
            $result = $downloader.Download()
            if ($result.ResultCode -isnot [int] -or $result.ResultCode -ne 2) { throw 'Download incomplete/uncertain' }
            for ($i=0; $i -lt $exact.Count; $i++) {
                if ($result.GetUpdateResult($i).ResultCode -isnot [int] -or $result.GetUpdateResult($i).ResultCode -ne 2 -or
                    $exact.Item($i).IsDownloaded -isnot [bool] -or !$exact.Item($i).IsDownloaded) { throw 'Exact payload incomplete' }
            }
        } else {
            foreach ($u in $exact) {
                if ($u.IsDownloaded -isnot [bool] -or !$u.IsDownloaded -or $u.EulaAccepted -isnot [bool] -or !$u.EulaAccepted) { throw 'Exact payload/EULA not ready' }
            }
            $installer = $session.CreateUpdateInstaller()
            $installer.Updates = $exact
            $installer.AllowSourcePrompts = $false
            $installer.ForceQuiet = $true
            if ($installer.RebootRequiredBeforeInstallation -isnot [bool] -or $installer.RebootRequiredBeforeInstallation) { throw 'WUA reboot prerequisite is pending or unknown' }
            PatchReady $session $bytes
            PatchBoundCollection $exact
            PatchSource
            PatchDeadline
            $result = $installer.Install() # WUA does not itself initiate a reboot
            if ($result.ResultCode -isnot [int] -or $result.ResultCode -ne 2) { throw 'Installation partial/uncertain; verification only' }
            for ($i=0; $i -lt $exact.Count; $i++) {
                if ($result.GetUpdateResult($i).ResultCode -isnot [int] -or $result.GetUpdateResult($i).ResultCode -ne 2) { throw 'Exact installation incomplete' }
            }
        }
        # Do not infer installed/reboot state from this acknowledgement.
        PatchJson @{acknowledged=$true}
    }
} catch { [Console]::Error.WriteLine('Exact patching failed or deferred; inspect protected record and verify, never replay'); exit 1 }
