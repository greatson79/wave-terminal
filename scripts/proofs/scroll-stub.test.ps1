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
