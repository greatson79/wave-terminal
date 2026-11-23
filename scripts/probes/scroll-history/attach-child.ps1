$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AttachReplayConsole {
  [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr GetStdHandle(int n);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool GetConsoleMode(IntPtr h, out uint mode);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool SetConsoleMode(IntPtr h, uint mode);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool SetConsoleOutputCP(uint codepage);
}
'@
if (-not [AttachReplayConsole]::SetConsoleOutputCP(65001)) { throw 'UTF8 console failed' }
$handle = [AttachReplayConsole]::GetStdHandle(-11)
[uint32]$mode = 0
if (-not [AttachReplayConsole]::GetConsoleMode($handle, [ref]$mode)) { throw 'GetConsoleMode failed' }
if (-not [AttachReplayConsole]::SetConsoleMode($handle, ($mode -bor 4))) { throw 'VT output failed' }
$b = [IO.File]::ReadAllBytes($env:SCROLL_REPLAY)
function Wait-Gate([string]$name) {
  $gate = Join-Path $env:SCROLL_ATTACH_DIR $name
  $deadline = [DateTime]::UtcNow.AddSeconds(120)
  while (-not (Test-Path -LiteralPath $gate)) {
    if ([DateTime]::UtcNow -gt $deadline) { throw "Gate timeout: $name" }
    Start-Sleep -Milliseconds 25
  }
}
Wait-Gate start
$o = [Console]::OpenStandardOutput()
$chunks = 0
$payloadBytes = 0
$repeats = [int]$env:SCROLL_STRESS_REPEATS
if ($repeats -lt 1 -or $repeats -gt 256) { throw 'Invalid stress repetitions' }
$begin = [Text.Encoding]::UTF8.GetBytes("`r`n$env:SCROLL_BEGIN`r`n")
$end = [Text.Encoding]::UTF8.GetBytes("`r`n$env:SCROLL_END`r`n")
$hasher = [Security.Cryptography.SHA256]::Create()
$o.Write($begin,0,$begin.Length)
$o.Flush()
Wait-Gate stress
for ($repetition=0; $repetition -lt $repeats; $repetition++) {
  for ($i=0; $i -lt $b.Length; $i+=512) {
    $n = [Math]::Min(512, $b.Length-$i)
    $o.Write($b,$i,$n)
    $o.Flush()
    [void]$hasher.TransformBlock($b,$i,$n,$b,$i)
    $chunks++
    $payloadBytes += $n
  }
  @{bytes=$payloadBytes;chunks=$chunks;phase='stress';repetition=($repetition+1);complete=$false} | ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path $env:SCROLL_ATTACH_DIR progress.json) -Encoding utf8
}
$o.Write($end,0,$end.Length)
$o.Flush()
@{bytes=$payloadBytes;chunks=$chunks;phase='await-tail';complete=$false} | ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path $env:SCROLL_ATTACH_DIR progress.json) -Encoding utf8
Wait-Gate tail
for ($i=0; $i -lt $b.Length; $i+=512) {
  $n = [Math]::Min(512, $b.Length-$i)
  $o.Write($b,$i,$n)
  $o.Flush()
  [void]$hasher.TransformBlock($b,$i,$n,$b,$i)
  $chunks++
  $payloadBytes += $n
  @{bytes=$payloadBytes;chunks=$chunks;phase='tail';tailBytes=($i+$n);complete=$false} | ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path $env:SCROLL_ATTACH_DIR progress.json) -Encoding utf8
  Start-Sleep -Milliseconds 40
}
[void]$hasher.TransformFinalBlock([byte[]]@(),0,0)
$sha = [BitConverter]::ToString($hasher.Hash).Replace('-','').ToLowerInvariant()
$hasher.Dispose()
$fixtureHasher = [Security.Cryptography.SHA256]::Create()
try { $fixtureSha = [BitConverter]::ToString($fixtureHasher.ComputeHash($b)).Replace('-','').ToLowerInvariant() } finally { $fixtureHasher.Dispose() }
@{bytes=$payloadBytes;chunks=$chunks;sha256=$sha;fixture_sha256=$fixtureSha;marker_bytes=($begin.Length+$end.Length);complete=$true;ps=$PSVersionTable.PSVersion.ToString();window_cols=[Console]::WindowWidth;window_rows=[Console]::WindowHeight} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $env:SCROLL_ATTACH_DIR complete.json) -Encoding utf8
Start-Sleep -Milliseconds 2000
