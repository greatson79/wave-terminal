[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$InstallDirectory,
  [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force $OutputDirectory | Out-Null
Add-Type @'
using System.Runtime.InteropServices;
public static class StartupPipeProbe {
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern bool WaitNamedPipe(string name, uint timeout);
}
'@
$pipe = '\\.\pipe\cys'
$cys = Join-Path $InstallDirectory 'cys.exe'
$daemon = Join-Path $InstallDirectory 'cysd.exe'
foreach ($name in 'CYS_SOCKET','JAVIS_SOCKET','AITERM_SOCKET') {
  Write-Host "$name=$([Environment]::GetEnvironmentVariable($name))"
  if ([Environment]::GetEnvironmentVariable($name)) { throw 'Unexpected pipe override in diagnostic runner' }
}
function Observe-Startup([string]$label, [string]$exe, [string]$arguments) {
  Get-Process cys,cysd -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 300
  $out = Join-Path $OutputDirectory "$label.stdout.log"
  $err = Join-Path $OutputDirectory "$label.stderr.log"
  $timeline = Join-Path $OutputDirectory "$label.timeline.jsonl"
  $clock = [Diagnostics.Stopwatch]::StartNew()
  $started = [DateTime]::UtcNow
  $info = [Diagnostics.ProcessStartInfo]::new()
  $info.FileName = $exe; $info.Arguments = $arguments
  $info.UseShellExecute = $false; $info.CreateNoWindow = $true
  $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true
  $process = [Diagnostics.Process]::Start($info)
  $readOut = $process.StandardOutput.ReadToEndAsync()
  $readErr = $process.StandardError.ReadToEndAsync()
  $firstPipe = $null; $exitMs = $null; $lastPids = ''; $lastSecond = -1
  while ($clock.ElapsedMilliseconds -lt 90000) {
    $visible = [StartupPipeProbe]::WaitNamedPipe($pipe, 1)
    $pipeError = if ($visible) { 0 } else { [Runtime.InteropServices.Marshal]::GetLastWin32Error() }
    $processes = @(Get-Process cysd -ErrorAction SilentlyContinue | ForEach-Object {
      [ordered]@{ pid = $_.Id; start_utc = $_.StartTime.ToUniversalTime().ToString('o'); path = $_.Path; cpu_seconds = $_.CPU }
    })
    if ($null -eq $firstPipe -and ($visible -or $pipeError -eq 121 -or $pipeError -eq 231)) { $firstPipe = $clock.ElapsedMilliseconds }
    if ($process.HasExited -and $null -eq $exitMs) { $exitMs = $clock.ElapsedMilliseconds }
    $ids = ($processes | ForEach-Object { $_.pid }) -join ','
    $second = [int][Math]::Floor($clock.Elapsed.TotalSeconds)
    if ($second -ne $lastSecond -or $ids -ne $lastPids -or ($null -ne $firstPipe -and $clock.ElapsedMilliseconds -lt $firstPipe + 200)) {
      $row = [ordered]@{ utc=[DateTime]::UtcNow.ToString('o'); elapsed_ms=$clock.ElapsedMilliseconds; process_pid=$process.Id; cysd=$processes; pipe=$pipe; pipe_available=$visible; pipe_error=$pipeError; process_exited=$process.HasExited }
      $row | ConvertTo-Json -Compress -Depth 5 | Add-Content $timeline
      Write-Host ($row | ConvertTo-Json -Compress -Depth 5)
      $lastSecond=$second; $lastPids=$ids
    }
    if ($null -ne $firstPipe -and $clock.ElapsedMilliseconds -gt $firstPipe + 2000) { break }
    if ($label -eq 'direct-cold' -and $process.HasExited) { break }
    Start-Sleep -Milliseconds 50
  }
  $summary = [ordered]@{ label=$label; start_utc=$started.ToString('o'); process_pid=$process.Id; first_pipe_ms=$firstPipe; pipe_lateness_after_4000_ms=$(if($null -ne $firstPipe){$firstPipe-4000}else{$null}); observed_ms=$clock.ElapsedMilliseconds; process_exit_ms=$exitMs; exit_code=$(if($process.HasExited){$process.ExitCode}else{$null}) }
  $summary | ConvertTo-Json | Set-Content (Join-Path $OutputDirectory "$label.summary.json")
  Get-Process cys,cysd -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  $process.WaitForExit(5000) | Out-Null
  $readOut.GetAwaiter().GetResult() | Set-Content $out
  $readErr.GetAwaiter().GetResult() | Set-Content $err
  Write-Host ($summary | ConvertTo-Json -Compress)
  Get-Content $err | Write-Host
}
Observe-Startup 'cli-cold' $cys 'list'
# Separate direct launch captures daemon stderr, which CLI autostart discards.
# A fresh pack isolates cold initialization without deleting any installed user data.
$env:CYS_PACK_DIR = Join-Path $OutputDirectory 'fresh-pack'
Observe-Startup 'direct-cold' $daemon ''
Remove-Item Env:CYS_PACK_DIR
Write-Host 'Diagnostic observation complete; the CLI 4-second budget was not changed.'
