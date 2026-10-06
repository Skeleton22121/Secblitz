param([ValidateSet('real','paging','mock','scene','static','unwind')][string]$Mode='real',[int]$Columns=80,[int]$Rows=24,[switch]$NoColor,[switch]$PagingFallback)
$ErrorActionPreference='Stop'
Set-StrictMode -Version 2
$root='C:\Windows\Temp\SecblitzV060CandidateValidation'
$r="$root\Results"
$tag="conpty-$Mode-${Columns}x${Rows}-$(if($NoColor){'no-color'}else{'color'})"
if($PagingFallback){$tag+='-right-alias'}
$utf8=[Text.UTF8Encoding]::new($false)
Add-Type -Path "$root\native-conpty-driver.cs"
$esc=[string][char]27
$down=$esc+'[B';$up=$esc+'[A';$pgdn=$esc+'[6~'
if($PagingFallback){$pgdn=$esc+'[C'}
$s=$null
function WaitPage([string]$text,[int]$mark=0,[int]$seconds=30){$null=$s.WaitAny([string[]]@($text),$mark,$seconds)}
function Capture([string]$name){$s.Dump("$r\$tag-$name.vt");[IO.File]::WriteAllText("$r\$tag-$name.frame.json",(@{width=$s.Width;height=$s.Height;pid=$s.Pid;mode=$Mode;no_color=[bool]$NoColor;native_windows_conpty=$true;application_version='0.6.0'}|ConvertTo-Json),$utf8)}
function Keys([string]$value,[int]$count=1){for($i=0;$i -lt $count;$i++){$s.Key($value)}}
try {
    $hashes=Get-Content "$root\candidate-final\binary-hashes.json" -Raw|ConvertFrom-Json
    $name=if($Mode -in @('real','paging')){'secblitz.exe'}else{'native-cli.exe'}
    $exe="$root\candidate-final\$name"
    if((Get-FileHash $exe).Hash.ToLowerInvariant() -cne $hashes.$name){throw 'Frozen console image hash mismatch'}
    $test=switch($Mode){'mock'{'guided::tests::native_guided_flow_probe'}'scene'{'menu::tests::native_scene_probe'}'static'{'menu::tests::native_no_animation_probe'}'unwind'{'menu::tests::native_panic_probe'}}
    $arguments=if($Mode -in @('real','paging')){'--lang en --no-animation guide'}else{"--ignored --exact $test --nocapture --test-threads=1"}
    $s=[NativeConPty]::new($exe,$arguments,[int16]$Columns,[int16]$Rows,[bool]$NoColor)
    if($Mode -eq 'paging') {
        WaitPage 'Your next step' 0 120;Keys $down;Keys "`r";WaitPage 'Protection review';Capture 'before'
        $before=$s.Mark();Keys $pgdn;Start-Sleep -Milliseconds 300;$after=$s.Mark();Capture 'after-pgdn'
        Keys ($esc+'[C');Start-Sleep -Milliseconds 300;$right=$s.Mark();Capture 'after-right'
        $rightBeforeUp=$s.Mark();Keys ($esc+'[5~');Start-Sleep -Milliseconds 300;$pageUp=$s.Mark();Capture 'after-pgup'
        [IO.File]::WriteAllText("$r\$tag-key-proof.json",(@{ExplicitWin32VirtualKeys=$true;PageDownChangedOutput=($after -gt $before);RightChangedOutput=($right -gt $after);PageUpChangedOutput=($pageUp -gt $rightBeforeUp);KnownDependency='console 0.15.11 omits VK_PRIOR/VK_NEXT mapping'}|ConvertTo-Json),$utf8)
        $mark=$s.Mark();Keys $esc;WaitPage 'Your next step' $mark;Keys $down 4;Keys "`r"
    } elseif($Mode -eq 'real') {
        WaitPage 'Your next step' 0 120;Capture 'home'
        $mark=$s.Mark();Keys "`r"
        $next=$s.WaitAny([string[]]@('Confirm selected fixes','There are no recommended'),$mark,30)
        Capture 'recommended'
        if($next -eq 'Confirm selected fixes'){$mark=$s.Mark();Keys $esc;WaitPage 'Your next step' $mark}
        Keys $down;$mark=$s.Mark();Keys "`r"
        $next=$s.WaitAny([string[]]@('Review and choose fixes','Protection review'),$mark,30)
        Capture 'review';$mark=$s.Mark();Keys $esc;WaitPage 'Your next step' $mark
        Keys $down 2;$mark=$s.Mark();Keys "`r";WaitPage 'Check results' $mark 120;Capture 'check-again'
        $mark=$s.Mark();Keys $esc;WaitPage 'Your next step' $mark
        Keys $down 3;$mark=$s.Mark();Keys "`r";WaitPage 'Advanced' $mark;Capture 'advanced'
        $mark=$s.Mark();Keys $esc;WaitPage 'Your next step' $mark;Capture 'escaped-back'
        Keys $down 4;Capture 'exit-choice';Keys "`r"
    } elseif($Mode -eq 'mock') {
        WaitPage 'Your next step';Capture 'home';Keys $down;Keys "`r";WaitPage 'Review and choose fixes'
        Keys ' ';Keys $down;Keys ' ';Capture 'selected-two';Keys "`r";WaitPage 'Selected fixes: 2';Capture 'recap-two'
        Keys $pgdn 6;Keys "`r" # Default Back, no fake mutation.
        Keys $down;Keys "`r";Keys $down;Keys ' ';Keys "`r";WaitPage 'Selected fixes: 1';Capture 'recap-one'
        Keys $pgdn 6;Keys $up 2;Keys "`r"
        WaitPage 'Applying selected fixes';WaitPage 'Checking after your changes';WaitPage 'Fix results';Capture 'mock-results';Keys "`r"
        Keys $down 4;Keys "`r"
    } elseif($Mode -eq 'scene') {
        WaitPage 'PTY checklist';Keys ' ';Keys $down;Keys ' ';WaitPage 'Selected: 2 of 20';Capture 'checklist';Keys "`r"
        WaitPage 'PTY confirmation';Capture 'confirmation';Keys ' ';Keys "`r"
        WaitPage 'PTY keep selection';Keys ' ';Keys $down 2;Keys ' ';Keys "`r"
        WaitPage 'PTY results';Keys "`r";WaitPage 'PTY private view';WaitPage 'abcdefghijklmnopqrstuvwx';Keys "`r"
        WaitPage 'PTY escaped';Keys $esc
        WaitPage 'PTY long recap';Keys $up;Keys "`r";Capture 'long-recap-gated';Keys $pgdn 12;Keys "`r"
        WaitPage 'PTY resize';$s.Resize(20,3);WaitPage 'Enlarge';Capture 'too-small';Keys "`r";$s.Resize(40,24);Capture 'resized-narrow';Keys "`r"
    } elseif($Mode -eq 'static') {WaitPage 'PTY static done';Capture 'no-animation';Keys "`r"}
    $code=$s.Finish(30);$s.Dump("$r\$tag-complete.vt")
    if($code -ne 0){throw "Native console child failed with $code"}
    [IO.File]::WriteAllText("$r\$tag.status.json",(@{ExitCode=$code;Mode=$Mode;Columns=$Columns;Rows=$Rows;NoColor=[bool]$NoColor;ActualWindowsConPTY=$true;PagingFallback=[bool]$PagingFallback;Completed=[DateTime]::UtcNow.ToString('o')}|ConvertTo-Json),$utf8)
} catch {
    if($s){$s.Dump("$r\$tag-failure.vt")}
    [IO.File]::WriteAllText("$r\$tag.FAILURE.txt",($_|Out-String),$utf8)
} finally {if($s){$s.Dispose()}}
