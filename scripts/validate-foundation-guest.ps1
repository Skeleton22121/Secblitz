param(
    [ValidateSet('prepare','stage','suite','suite-fast','remaining','inspect','native','stop-fixture','diagnostics','check-health','restore','export','shutdown')][string]$Phase,
    [ValidatePattern('^candidate-[a-z0-9-]+$')][string]$Candidate = 'candidate-b'
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version 2
$root='C:\Windows\Temp\SecblitzFoundationValidation20261003'
$results="$root\Results"
$journal='C:\ProgramData\Secblitz'
$utf8=[Text.UTF8Encoding]::new($false)
trap {
    if(Test-Path -LiteralPath $results){[IO.File]::WriteAllText("$results\$Candidate-$Phase.FAILURE.txt",($_|Out-String),$utf8)}
    exit 1
}
function JsonFile($path,$value) { [IO.File]::WriteAllText($path,(ConvertTo-Json -InputObject $value -Depth 32),$utf8) }
function Tree($path) {
    if (!(Test-Path -LiteralPath $path)) { return @() }
    $entries=@(Get-Item -LiteralPath $path)+@(Get-ChildItem -LiteralPath $path -Recurse -Force)
    $records=@()
    foreach($entry in @($entries | Sort-Object FullName)) {
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Baseline contains a reparse point; manual inspection required' }
        $relative=$entry.FullName.Substring($path.Length)
        $records+= [ordered]@{Path=$relative;Directory=$entry.PSIsContainer;Attributes=[int]$entry.Attributes;Sddl=(Get-Acl -LiteralPath $entry.FullName).Sddl;SHA256=$(if(!$entry.PSIsContainer){(Get-FileHash -LiteralPath $entry.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}else{$null});Length=$(if(!$entry.PSIsContainer){$entry.Length}else{$null})}
    }
    return $records
}
function CopyTree($from,$to,$log) {
    & "$env:SystemRoot\System32\robocopy.exe" $from $to /E /B /COPYALL /DCOPY:DAT /R:0 /W:0 /XJ /NP /NFL /NDL "/LOG:$log" | Out-Null
    if($LASTEXITCODE -ge 8){throw "Exact journal copy failed: $LASTEXITCODE"}
}
function Controls($path) {
    if ($null -eq ('ServiceFixture' -as [type])) { . 'C:\Windows\Temp\secblitz-v020-native.ps1' }
    $null=Capture-All $path
}
function Run($name,$exe,$arguments,$seconds=300) {
    $start=[DateTime]::UtcNow
    if(!(Test-Path "$root\empty.stdin")){[IO.File]::WriteAllBytes("$root\empty.stdin",[byte[]]@())}
    $p=Start-Process -FilePath $exe -ArgumentList $arguments -RedirectStandardInput "$root\empty.stdin" -RedirectStandardOutput "$results\$name.out" -RedirectStandardError "$results\$name.err" -PassThru
    try {
        $null=$p.Handle
        if(!$p.WaitForExit($seconds*1000)) {
            JsonFile "$results\$name.status.json" @{Name=$name;Timeout=$true;Pid=$p.Id;Started=$start.ToString('o')}
            throw "$name remains running; no process was killed"
        }
        $code=$p.ExitCode
        JsonFile "$results\$name.status.json" @{Name=$name;ExitCode=$code;Started=$start.ToString('o');ElapsedSeconds=([DateTime]::UtcNow-$start).TotalSeconds}
        return $code
    } finally { $p.Dispose() }
}
if($Phase -eq 'prepare') {
    if(Test-Path -LiteralPath $root){throw 'Validation root already exists; never replace prior evidence'}
    New-Item -ItemType Directory -Path $results | Out-Null
    New-Item -ItemType Directory -Path "$root\Baseline" | Out-Null
    $admin=([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if(!$admin){throw 'Elevated native validator required'}
    if((Get-Service SecblitzMonitor -ErrorAction SilentlyContinue) -or (Get-Process secblitz -ErrorAction SilentlyContinue)){throw 'Existing Secblitz service/process; inspect before proceeding'}
    $os=Get-CimInstance Win32_OperatingSystem
    JsonFile "$results\environment.json" @{Caption=$os.Caption;Build=$os.BuildNumber;Architecture=$os.OSArchitecture;PowerShell=$PSVersionTable.PSVersion.ToString();Elevated=$admin;JournalExisted=(Test-Path -LiteralPath $journal);Installed=(Test-Path 'C:\Program Files\Secblitz\secblitz.exe')}
    Controls "$results\controls-before.json"
    $before=@(Tree $journal)
    JsonFile "$results\journal-before.json" $before
    if(Test-Path -LiteralPath $journal) {
        CopyTree $journal "$root\Baseline\Journal" "$results\journal-backup.log"
        $backup=@(Tree "$root\Baseline\Journal")
        JsonFile "$results\journal-backup.json" $backup
        if((ConvertTo-Json -InputObject $before -Depth 12 -Compress) -cne (ConvertTo-Json -InputObject $backup -Depth 12 -Compress)){throw 'Backup bytes/ACLs/attributes do not match original tree'}
    }
    'VERIFIED' | Set-Content "$root\Baseline\verified.txt"
} elseif($Phase -eq 'shutdown') {
    $proof=Get-Content "$results\preservation.json" -Raw | ConvertFrom-Json
    if(!$proof.All18ControlsUnchanged -or !$proof.JournalBytesAclsAttributesRestored){throw 'Preservation not established'}
    & "$env:SystemRoot\System32\shutdown.exe" /s /t 0 /d p:0:0 /c 'Secblitz foundation validation complete; baseline restored'
} elseif($Phase -eq 'stage') {
    if(!(Test-Path "$root\Baseline\verified.txt")){throw 'Verified backup required'}
    New-Item -ItemType Directory -Path "$root\$Candidate" | Out-Null
} elseif($Phase -eq 'export') {
    Compress-Archive -Path "$results\*" -DestinationPath "$root\results.zip" -Force
} else {
    if(!(Test-Path "$root\Baseline\verified.txt")){throw 'Verified journal backup is required before tests'}
    $bin="$root\$Candidate"
    if($Phase -ne 'restore') {
        $hashes=Get-Content "$bin\binary-hashes.json" -Raw | ConvertFrom-Json
        foreach($name in @('secblitz.exe','native-lib.exe','native-cli.exe')) {
            if((Get-FileHash "$bin\$name").Hash.ToLowerInvariant() -cne $hashes.$name){throw "Frozen image hash mismatch: $name"}
        }
    }
    if($Phase -eq 'stop-fixture') {
        foreach($suffix in @('lib','lib-bounded')) {
        if(!(Test-Path "$results\$Candidate-$suffix.status.json")){continue}
        $status=Get-Content "$results\$Candidate-$suffix.status.json" -Raw | ConvertFrom-Json
        if($null -eq $status.PSObject.Properties['Timeout']){continue}
        if(!$status.Timeout){throw 'No timed-out fixture to inspect'}
        $process=Get-CimInstance Win32_Process -Filter "ProcessId=$($status.Pid)"
        if($process) {
            if($process.ExecutablePath -cne "$bin\native-lib.exe" -or $process.CommandLine -notmatch '--test-threads=1'){throw 'PID identity changed; refuse termination'}
            JsonFile "$results\$Candidate-stopped-$suffix-fixture.json" @{Pid=$process.ProcessId;Path=$process.ExecutablePath;CommandLine=$process.CommandLine;Reason='Reviewed pure-memory-backend TempDir fixture exceeded native budget; no servicing command in this test'}
            Stop-Process -Id $process.ProcessId -Force
        }
        }
    } elseif($Phase -eq 'remaining') {
        $null=Run "$Candidate-cli" "$bin\native-cli.exe" @('--test-threads=1') 180
        $null=Run "$Candidate-lib-nonengine" "$bin\native-lib.exe" @('--test-threads=1','--skip','engine::tests::') 180
    } elseif($Phase -eq 'inspect') {
        $modules=@(Get-Module -ListAvailable Microsoft.PowerShell.LocalAccounts | Select-Object Name,Version,Path)
        $disks=@(Get-PhysicalDisk | ForEach-Object {@{Health=$_.HealthStatus;Type=$_.HealthStatus.GetType().FullName;RawHealth=$_.CimInstanceProperties['HealthStatus'].Value;RawType=$_.CimInstanceProperties['HealthStatus'].Value.GetType().FullName}})
        JsonFile "$results\native-provider-shapes.json" @{LocalAccounts=$modules;Disks=$disks}
    } elseif($Phase -in @('suite','suite-fast','native')) {
        if($Phase -eq 'suite') {
        $null=Run "$Candidate-lib-list" "$bin\native-lib.exe" @('--list')
        $null=Run "$Candidate-lib" "$bin\native-lib.exe" @('--test-threads=1') 900
        }
        if($Phase -eq 'suite-fast') {
            $null=Run "$Candidate-lib-bounded" "$bin\native-lib.exe" @('--test-threads=1','--skip','every_cow_snapshot_byte_prefix','--skip','every_header_creation_prefix','--skip','every_prepare_record_prefix') 600
        }
        if($Phase -ne 'native') {
        $null=Run "$Candidate-cli" "$bin\native-cli.exe" @('--test-threads=1') 600
        }
        $tests=@(
            'operations::storage::tests::atomic_publication_failure_at_every_native_boundary',
            'operations::storage::tests::shared_fs2_lock_and_links_reject_without_repair',
            'operations::storage::tests::foreign_dacl_is_rejected_without_adoption',
            'operations::storage::tests::module_payload_is_checked_and_pinned_not_just_its_manifest',
            'operations::process::tests::direct_exit_does_not_release_descendant_supervision',
            'operations::process::tests::only_read_only_jobs_terminate_on_drop',
            'diagnostics::windows::tests::elevated_process_cannot_substitute_its_browser_profile',
            'readiness::windows::tests::native_readonly_smoke',
            'updater::windows::tests::interlock_inspection_is_read_only_and_outlives_a_lost_lock_owner',
            'updater::windows::tests::uncertain_installation_blocks_before_health_and_failed_reset_retains_intent',
            'updater::windows::tests::probe_output_exit_size_and_timeout_are_enforced'
        )
        foreach($test in $tests){$name=$test.Replace('::','-');$null=Run "$Candidate-$name" "$bin\native-lib.exe" @('--exact',$test,'--ignored','--nocapture','--test-threads=1') 180}
        $null=Run "$Candidate-help" "$bin\secblitz.exe" @('--help')
        $null=Run "$Candidate-health" "$bin\secblitz.exe" @('update','health','--json')
        Controls "$results\controls-after-suite.json"
        JsonFile "$results\journal-after-suite.json" @(Tree $journal)
    } elseif($Phase -eq 'diagnostics') {
        $null=Run "$Candidate-diagnostics" "$bin\secblitz.exe" @('diagnostics','run','--profile','everyday','--json','--details') 300
        $null=Run "$Candidate-original-user-cli" "$bin\secblitz.exe" @('diagnostics','run','--original-user','--json','--details') 300
    } elseif($Phase -eq 'check-health') {
        $null=Run "$Candidate-policy" "$bin\secblitz.exe" @('operations','policy','show','--json','--details')
        $code=Run "$Candidate-check-plan" "$bin\secblitz.exe" @('operations','plan','dism_check_health','--valid-for','600','--json','--details')
        if($code -eq 0) {
            $plan=Get-Content "$results\$Candidate-check-plan.out" -Raw | ConvertFrom-Json
            if($plan.steps.Count -ne 1 -or $plan.steps[0].operation.kind -cne 'dism_check_health'){throw 'Unexpected check-health plan shape'}
            $code=Run "$Candidate-check-approve" "$bin\secblitz.exe" @('operations','approve',$plan.id,'--digest',$plan.digest,'--valid-for','600','--yes','--json','--details')
            if($code -eq 0){$null=Run "$Candidate-check-run" "$bin\secblitz.exe" @('operations','run',$plan.id,'--digest',$plan.digest,'--yes','--json','--details') 300}
        }
    } elseif($Phase -eq 'restore') {
        $active=@(Get-CimInstance Win32_Process | Where-Object {$_.Name -in @('secblitz.exe','native-lib.exe','native-cli.exe') -and $_.ExecutablePath -like "$root\*"} | Select-Object Name,ProcessId,ExecutablePath)
        JsonFile "$results\validation-processes-final.json" $active
        if($active.Count -ne 0){throw 'Validation process remains alive; do not restore under a live owner'}
        if(Test-Path "$journal\operations\state.json") {
            $state=Get-Content "$journal\operations\state.json" -Raw | ConvertFrom-Json
            JsonFile "$results\check-health-durable-evidence.json" $state
        }
        Controls "$results\controls-final.json"
        $before=Get-Content "$results\controls-before.json" -Raw | ConvertFrom-Json
        $after=Get-Content "$results\controls-final.json" -Raw | ConvertFrom-Json
        if(($before|ConvertTo-Json -Depth 12 -Compress) -cne ($after|ConvertTo-Json -Depth 12 -Compress)){throw 'Control drift; stop and inspect, do not invent a restoration'}
        JsonFile "$results\journal-post-test.json" @(Tree $journal)
        if(Test-Path -LiteralPath $journal) { Move-Item -LiteralPath $journal -Destination "$root\PostTestJournal" }
        if(Test-Path "$root\Baseline\Journal") { CopyTree "$root\Baseline\Journal" $journal "$results\journal-restore.log" }
        $restored=@(Tree $journal)
        JsonFile "$results\journal-restored.json" $restored
        $original=Get-Content "$results\journal-before.json" -Raw | ConvertFrom-Json
        if((ConvertTo-Json -InputObject @($original) -Depth 12 -Compress) -cne (ConvertTo-Json -InputObject $restored -Depth 12 -Compress)){throw 'Journal restoration mismatch'}
        JsonFile "$results\preservation.json" @{All18ControlsUnchanged=$true;JournalBytesAclsAttributesRestored=$true;PostTestTreePreserved=$true;NoInstallPerformed=$true;Completed=[DateTime]::UtcNow.ToString('o')}
    }
}
