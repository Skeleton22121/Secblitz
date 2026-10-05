$ErrorActionPreference='Stop'
$root='C:\Windows\Temp\SecblitzV060CandidateValidation'
Add-Type -Path "$root\native-conpty-driver.cs"
$s=[NativeConPty]::new("$root\console-probe.exe",'',80,24,$false)
try {$null=$s.Finish(15);$s.Dump("$root\Results\console-probe.vt")}finally{$s.Dispose()}
