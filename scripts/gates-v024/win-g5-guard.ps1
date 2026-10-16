$ErrorActionPreference = 'Stop'
$cys = Join-Path $env:LOCALAPPDATA 'Wave Terminal\cys.exe'
if (-not (Test-Path -LiteralPath $cys)) { throw 'installed cys missing' }
$ref = (& $cys new-surface --title g5-guard | Select-Object -Last 1).Trim()
if ($LASTEXITCODE -ne 0 -or $ref -notmatch '^surface:(\d+)$') { throw "new-surface failed: $ref" }
$sid = [uint64]$Matches[1]

function Send-Rpc([string]$text, [bool]$human) {
  $pipe = [IO.Pipes.NamedPipeClientStream]::new('.', 'cys', [IO.Pipes.PipeDirection]::InOut)
  try {
    $pipe.Connect(5000)
    $writer = [IO.StreamWriter]::new($pipe, [Text.UTF8Encoding]::new($false), 4096, $true)
    $reader = [IO.StreamReader]::new($pipe, [Text.UTF8Encoding]::new($false), $false, 4096, $true)
    $writer.AutoFlush = $true
    $request = @{ id = 1; method = 'surface.send_text'; params = @{ surface_id = $sid; text = $text; quiet = $true; human = $human } } | ConvertTo-Json -Compress -Depth 5
    $writer.WriteLine($request)
    $reply = $reader.ReadLine() | ConvertFrom-Json
    if (-not $reply.ok) { throw "RPC rejected: $($reply.error.code)" }
  } finally { $pipe.Dispose() }
}

& $cys send --queued --surface $ref 'G5_QUEUE_PROOF' 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'queue enqueue failed' }
$responses = @("`e[I", "`e[O", "`e[1;1R", "`e[?1;2c")
$direct = 0
$clock = [Diagnostics.Stopwatch]::StartNew()
for ($i = 0; $i -lt 50; $i++) {
  Send-Rpc $responses[$i % $responses.Count] $false
  if ($i % 2 -eq 0 -and $direct -lt 20) {
    & $cys send --surface $ref ' ' 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "direct send $direct failed" }
    $direct++
  }
  $wait = 200 * ($i + 1) - $clock.ElapsedMilliseconds
  if ($wait -gt 0) { Start-Sleep -Milliseconds $wait }
}
if ($direct -ne 20 -or $clock.ElapsedMilliseconds -gt 11000) { throw "10-second response stream or direct sends failed: $direct / $($clock.ElapsedMilliseconds)ms" }
$deadline = (Get-Date).AddSeconds(60)
do {
  $queue = (& $cys queue list --surface $ref 2>&1) -join "`n"
  if ($LASTEXITCODE -ne 0) { throw 'queue inspection failed' }
  $screen = (& $cys read-screen --surface $ref 2>&1) -join "`n"
  if ($LASTEXITCODE -ne 0) { throw 'screen inspection failed' }
  if ($queue -match '\(queue empty\)' -and $screen.Contains('G5_QUEUE_PROOF')) { break }
  Start-Sleep -Seconds 2
} while ((Get-Date) -lt $deadline)
if ($queue -notmatch '\(queue empty\)' -or -not $screen.Contains('G5_QUEUE_PROOF')) { throw 'queued message not delivered' }
Send-Rpc 'x' $true
$blocked = (& $cys send --surface $ref 'guard-proof' 2>&1) -join "`n"
if ($LASTEXITCODE -eq 0 -or $blocked -notmatch 'typing_guard') { throw "human input did not activate typing guard: $blocked" }
Write-Host 'G5 win-guard PASS'
exit 0
