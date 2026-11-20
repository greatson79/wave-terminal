$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class ReplayConsole {
  [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr GetStdHandle(int n);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool GetConsoleMode(IntPtr h, out uint mode);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool SetConsoleMode(IntPtr h, uint mode);
  [DllImport("kernel32.dll")] public static extern uint GetConsoleOutputCP();
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool SetConsoleOutputCP(uint codepage);
}
'@
$codepageBefore = [ReplayConsole]::GetConsoleOutputCP()
if (-not [ReplayConsole]::SetConsoleOutputCP(65001)) { throw 'SetConsoleOutputCP UTF-8 failed' }
$handle = [ReplayConsole]::GetStdHandle(-11)
[uint32]$mode = 0
if (-not [ReplayConsole]::GetConsoleMode($handle, [ref]$mode)) { throw 'GetConsoleMode failed' }
# Claude emits VT output; replay it as VT, not as literal escape characters.
if (-not [ReplayConsole]::SetConsoleMode($handle, ($mode -bor 4))) { throw 'SetConsoleMode VT failed' }
$metadata = @{ codepage_before = $codepageBefore; codepage_after = [ReplayConsole]::GetConsoleOutputCP(); ps = $PSVersionTable.PSVersion.ToString(); output_mode_before = $mode; output_mode_after = ($mode -bor 4); window_cols = [Console]::WindowWidth; window_rows = [Console]::WindowHeight; buffer_cols = [Console]::BufferWidth; buffer_rows = [Console]::BufferHeight }
$metadata['payload_write_completed'] = $false
$metadata | ConvertTo-Json | Set-Content -LiteralPath $env:SCROLL_METADATA -Encoding utf8
$b = [IO.File]::ReadAllBytes($env:SCROLL_REPLAY)
$o = [Console]::OpenStandardOutput()
if ($env:SCROLL_MODE -eq 'paced') {
  for ($i=0; $i -lt $b.Length; $i+=512) {
    $n = [Math]::Min(512, $b.Length-$i)
    $o.Write($b, $i, $n)
    $o.Flush()
    Start-Sleep -Milliseconds 20
  }
} else {
  $o.Write($b, 0, $b.Length)
  $o.Flush()
}

$metadata['payload_bytes'] = $b.Length
$hasher = [Security.Cryptography.SHA256]::Create()
try { $metadata['payload_sha256'] = [BitConverter]::ToString($hasher.ComputeHash($b)).Replace('-','').ToLowerInvariant() } finally { $hasher.Dispose() }
$metadata['payload_write_completed'] = $true
$metadata | ConvertTo-Json | Set-Content -LiteralPath $env:SCROLL_METADATA -Encoding utf8
# Keep the synthetic child alive briefly for the final ConPTY repaint to drain.
Start-Sleep -Milliseconds 1000
