param([ValidateSet('keys','resize','navigation','real')][string]$Mode,[int]$Columns=80,[int]$Rows=24,[switch]$NoColor)
$ErrorActionPreference='Stop'
Set-StrictMode -Version 2
$root='C:\Windows\Temp\SecblitzV060UiFinal'
$r="$root\Results"
$tag="ui-$Mode-${Columns}x${Rows}-$(if($NoColor){'no-color'}else{'color'})"
$utf8=[Text.UTF8Encoding]::new($false)
Add-Type -Path "$root\native-conpty-driver.cs"
$s=$null;$events=[Collections.Generic.List[object]]::new()
$proof=[ordered]@{Mode=$Mode;Columns=$Columns;Rows=$Rows;NoColor=[bool]$NoColor;ActualWindowsConPTY=$true;RealVirtualKeys=$true;PagingAliasUsed=$false}
function WaitPage([string]$text,[int]$mark=0,[int]$seconds=30){$null=$s.WaitAny([string[]]@($text),$mark,$seconds)}
function Capture([string]$name){$s.Dump("$r\$tag-$name.vt");[IO.File]::WriteAllText("$r\$tag-$name.frame.json",(@{width=$s.Width;height=$s.Height;pid=$s.Pid;mode=$Mode;no_color=[bool]$NoColor;native_windows_conpty=$true;application_version='0.6.0'}|ConvertTo-Json),$utf8)}
function Key([string]$name,[int]$count=1) {
    $spec=switch($name){
        'PGDN' {@(0x22,0x51,0,0x100)} 'PGUP' {@(0x21,0x49,0,0x100)}
        'HOME' {@(0x24,0x47,0,0x100)} 'END' {@(0x23,0x4f,0,0x100)}
        'RIGHT' {@(0x27,0x4d,0,0x100)} 'LEFT' {@(0x25,0x4b,0,0x100)}
        'DOWN' {@(0x28,0x50,0,0x100)} 'UP' {@(0x26,0x48,0,0x100)}
        'ENTER' {@(0x0d,0x1c,0,0)} 'ESC' {@(0x1b,0x01,0,0)}
        'SPACE' {@(0x20,0x39,32,0)} 'CTRL-C' {@(0x43,0x2e,3,8)}
        'Z' {@(0x5a,0x2c,122,0)} default {throw 'Unexpected native key'}
    }
    for($i=0;$i -lt $count;$i++){
        $events.Add(@{Key=$name;VK=$spec[0];Scan=$spec[1];Unicode=$spec[2];Control=$spec[3];KeyDownCount=1;KeyUpCount=1;Repeat=1})
        $s.VirtualKey($spec[0],$spec[1],$spec[2],$spec[3])
    }
}
try {
    $hashes=Get-Content "$root\candidate-final\binary-hashes.json" -Raw|ConvertFrom-Json
    $name=if($Mode -eq 'real'){'secblitz.exe'}else{'native-cli.exe'}
    $exe="$root\candidate-final\$name"
    if((Get-FileHash $exe).Hash.ToLowerInvariant() -cne $hashes.$name){throw 'Frozen image hash mismatch'}
    $test=switch($Mode){'keys'{'menu::tests::native_windows_mode_and_page_keys'}'resize'{'menu::tests::native_resize_consent_probe'}'navigation'{'guided::tests::native_navigation_probe'}}
    $arguments=if($Mode -eq 'real'){'--lang en --no-animation guide'}else{"--ignored --exact $test --nocapture --test-threads=1"}
    $s=[NativeConPty]::new($exe,$arguments,[int16]$Columns,[int16]$Rows,[bool]$NoColor)
    if($Mode -eq 'keys') {
        foreach($key in @('PGDN','PGUP','HOME','END','RIGHT','LEFT','ENTER','ESC','SPACE','CTRL-C')) {
            WaitPage "KEY-$key";Capture "KEY-$key";Key $key
        }
        $proof.InternalModeAssertions='Pre-screen helpers plus normal/error/panic and every read restore exact captured modes'
    } elseif($Mode -eq 'resize') {
        WaitPage 'RESIZE-CONSENT';Capture 'initial-80x24'
        $s.ResizeNow(20,3);Key 'Z';WaitPage 'Enlarge';Capture 'too-small'
        $before=$s.Mark()
        $s.ResizeNow(80,80);Key 'ENTER'
        WaitPage 'LAST-RECAP-LINE' $before
        Start-Sleep -Milliseconds 300
        if($s.Text().Contains('RESIZE-APPROVED')){throw 'First resize Enter authorized without a fresh confirmation'}
        Capture 'first-enter-blocked'
        $proof.FirstResizeEnterBlocked=$true;$proof.LastRecapLineRenderedBeforeApproval=$true
        Key 'ENTER';WaitPage 'RESIZE-APPROVED';Capture 'fresh-enter-approved'
        $proof.FreshEnterRequired=$true
        Key 'ENTER'
    } elseif($Mode -eq 'navigation') {
        for($step=0;$step -le 8;$step++) {
            WaitPage "NAV-$step";Capture "NAV-$step"
            switch($step){
                0 {Key 'DOWN' 3;Key 'ENTER'}
                1 {Key 'DOWN' 5;Key 'ENTER'}
                2 {Key 'ESC'}
                3 {Key 'DOWN' 4;Key 'ENTER'}
                4 {Key 'DOWN' 4;Key 'ENTER'}
                5 {Key 'DOWN' 4;Key 'ENTER'}
                6 {Key 'ESC'}
                7 {Key 'ESC'}
                8 {Key 'DOWN' 4;Key 'ENTER'}
            }
        }
        $proof.NineNavigationMarkersDriven=$true;$proof.MockEngineOnly=$true
    } else {
        WaitPage 'Your next step' 0 120;Capture 'home'
        $mark=$s.Mark();Key 'ENTER'
        $next=$s.WaitAny([string[]]@('Confirm selected fixes','There are no recommended'),$mark,30)
        Capture 'recommended'
        if($next -eq 'Confirm selected fixes'){$mark=$s.Mark();Key 'ESC';WaitPage 'Your next step' $mark}
        Key 'DOWN';$mark=$s.Mark();Key 'ENTER'
        $next=$s.WaitAny([string[]]@('Review and choose fixes','Protection review'),$mark,30)
        Capture 'review'
        if($next -eq 'Protection review') {
            $before=$s.Mark();Key 'PGDN';Start-Sleep -Milliseconds 250;$after=$s.Mark();Capture 'real-pgdn'
            if($after -le $before){throw 'Real VK_NEXT did not scroll the document'}
            Key 'PGUP';Start-Sleep -Milliseconds 250
            if($s.Mark() -le $after){throw 'Real VK_PRIOR did not scroll the document'}
            Capture 'real-pgup';$proof.RealReleasePageKeysScroll=$true
        }
        $mark=$s.Mark();Key 'ESC';WaitPage 'Your next step' $mark
        Key 'DOWN' 2;$mark=$s.Mark();Key 'ENTER';WaitPage 'Check results' $mark 120;Capture 'check-again'
        $mark=$s.Mark();Key 'ESC';WaitPage 'Your next step' $mark
        Key 'DOWN' 3;$mark=$s.Mark();Key 'ENTER';WaitPage 'Advanced' $mark;Capture 'advanced'
        $mark=$s.Mark();Key 'ESC';WaitPage 'Your next step' $mark;Capture 'escaped-back'
        Key 'DOWN' 4;Capture 'exit-choice';Key 'ENTER'
        $proof.AllFiveRealRoutes=$true;$proof.ActualMutation=$false
    }
    $code=$s.Finish(30);$s.Dump("$r\$tag-complete.vt")
    if($code -ne 0){throw "Console child or mode restoration check failed: $code"}
    $proof.ExitCode=$code;$proof.Completed=[DateTime]::UtcNow.ToString('o');$proof.Events=$events.ToArray()
    [IO.File]::WriteAllText("$r\$tag.status.json",($proof|ConvertTo-Json -Depth 8),$utf8)
} catch {
    if($s){$s.Dump("$r\$tag-failure.vt")}
    [IO.File]::WriteAllText("$r\$tag.FAILURE.txt",($_|Out-String),$utf8)
} finally {if($s){$s.Dispose()}}
