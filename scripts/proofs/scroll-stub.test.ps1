$ErrorActionPreference='Stop'
$source=Get-Content (Join-Path $PSScriptRoot 'scroll-stub.ps1') -Raw
$code=[regex]::Match($source,"(?s)@'\r?\n(.*?)\r?\n'@").Groups[1].Value
Add-Type -TypeDefinition $code
foreach($report in @("$([char]27)[5~","$([char]27)[<64;11;11M",([string][char]27+'[M`++'))) {
 $expected=if($report.EndsWith('5~')){-10}else{-3}
 for($i=0;$i -le $report.Length;$i++) {
  $tail='';$a=[ScrollStub]::Parse([ref]$tail,$report.Substring(0,$i));$b=[ScrollStub]::Parse([ref]$tail,$report.Substring($i))
  if(($a+$b) -ne $expected){throw "split report failed at $i"}
  if([ScrollStub]::Parse([ref]$tail,'') -ne 0){throw 'duplicate delivery'}
 }
}
Write-Output 'STUB_SPLIT_REPORT_CONTROLS_PASS'

$line=300;$burst=[ScrollStub]::Burst([ref]$line)
if($line -ne 305 -or ([regex]::Matches($burst,"`r`n").Count-3) -ne 5 -or [regex]::Matches($burst,[regex]::Escape([char]27+'[2K')).Count -ne 3 -or -not $burst.Contains([char]27+'[3A')){throw 'burst cursor/net-line contract failed'}
if($burst.Contains([char]27+'[2J')){throw 'full-screen erase forbidden during stream'}
'BURST_REPAINT_CONTROLS_PASS'

. (Join-Path $PSScriptRoot 'scroll-ocr-consensus.ps1')
$old=$env:SCROLL_FIX_LIVE
try {
 $env:SCROLL_FIX_LIVE='1'
 foreach($n in @(1000,1001,1178,1190,1200,1281,1282,1300)) {
  $marker=[ScrollStub]::Marker($n)
  if($marker -cnotmatch '^WR[1234579]{5}$' -or (@(Read-ScrollMarkers -Text $marker -Encoded $true) -join ',') -ne "$n"){throw 'producer/observer encoded marker roundtrip failed'}
 }
 if([ScrollStub]::Marker(1300) -cne 'WR47471'){throw 'producer checksum vector mismatch'}
} finally {$env:SCROLL_FIX_LIVE=$old}
'PRODUCER_OBSERVER_MARKER_ROUNDTRIP_PASS'
