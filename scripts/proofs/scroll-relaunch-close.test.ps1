$ErrorActionPreference='Stop'
if($env:OS -ne 'Windows_NT' -or $env:GITHUB_ACTIONS -ne 'true'){throw 'Windows runner controls only'}
# All process operations are mocks; no real process is stopped by these controls.
function Get-Process {
 param($Id,$ErrorAction)
 if($Id -ne 123){throw 'unexpected process target'}
 if($script:mock.exited){return $null}
 return $script:mock.process
}
function Get-CimInstance {
 param($ClassName,$Filter)
 if($ClassName -ne 'Win32_Process' -or $Filter -ne 'ProcessId=123'){throw 'unexpected identity target'}
 $script:mock.queries++
 return [pscustomobject]@{ExecutablePath='C:\Wave\wave-app.exe';CreationDate=if($script:mock.mode -eq 'changed' -and $script:mock.queries -gt 1){2}else{1}}
}
foreach($mode in @('graceful','hidden','changed','daemon')) {
 $script:mock=@{mode=$mode;queries=0;killed=$false;exited=$false}
 $process=[pscustomobject]@{Handle=42}
 $process | Add-Member ScriptMethod CloseMainWindow {return $script:mock.mode -eq 'graceful'}
 $process | Add-Member ScriptMethod WaitForExit {param($timeout);if($script:mock.mode -eq 'graceful' -or $script:mock.killed){$script:mock.exited=$true;return $true};return $false}
 $process | Add-Member ScriptMethod Kill {$script:mock.killed=$true}
 $process | Add-Member ScriptMethod Dispose {}
 $script:mock.process=$process
 $daemon=if($mode -eq 'daemon'){123}else{999}
 $result=$null;$caught=$null
 try {$result=. (Join-Path $PSScriptRoot 'scroll-relaunch-close.ps1') -GuiPid 123 -DaemonPid $daemon -ExpectedPath 'C:\Wave\wave-app.exe' | ConvertFrom-Json}
 catch {$caught=$_}
 if($mode -in @('changed','daemon')) {
  if(-not $caught -or $script:mock.killed){throw "unsafe stop control failed: $mode"}
 } else {
  if($caught -or -not $result.gone -or $script:mock.killed -ne ($mode -eq 'hidden')){throw "GUI exit control failed: $mode $caught"}
 }
}
Write-Output 'PASS: graceful exit, hide-only GUI stop, changed identity refusal, daemon refusal (mocked process operations)'
