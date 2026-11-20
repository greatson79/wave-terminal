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
$start = Join-Path $env:SCROLL_ATTACH_DIR start
$deadline = [DateTime]::UtcNow.AddSeconds(30)
while (-not (Test-Path -LiteralPath $start)) {
  if ([DateTime]::UtcNow -gt $deadline) { throw 'Start gate timeout' }
  Start-Sleep -Milliseconds 25
}
$o = [Console]::OpenStandardOutput()
for ($i=0; $i -lt $b.Length; $i+=512) {
  $n = [Math]::Min(512, $b.Length-$i)
  $o.Write($b,$i,$n)
  $o.Flush()
  @{bytes=($i+$n);complete=$false} | ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path $env:SCROLL_ATTACH_DIR progress.json) -Encoding utf8
  Start-Sleep -Milliseconds 40
}
$hasher = [Security.Cryptography.SHA256]::Create()
try { $sha = [BitConverter]::ToString($hasher.ComputeHash($b)).Replace('-','').ToLowerInvariant() } finally { $hasher.Dispose() }
@{bytes=$b.Length;sha256=$sha;complete=$true;ps=$PSVersionTable.PSVersion.ToString();window_cols=[Console]::WindowWidth;window_rows=[Console]::WindowHeight} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $env:SCROLL_ATTACH_DIR complete.json) -Encoding utf8
Start-Sleep -Milliseconds 2000
