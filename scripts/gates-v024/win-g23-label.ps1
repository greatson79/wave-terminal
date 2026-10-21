$ErrorActionPreference = 'Stop'
$cys = Join-Path $env:LOCALAPPDATA 'Wave Terminal\cys.exe'
if (-not (Test-Path -LiteralPath $cys)) { throw 'installed cys missing' }
$role = 'g23-label-proof'
$ref = (& $cys new-surface --title g23-label --cmd cmd.exe --role $role | Select-Object -Last 1).Trim()
if ($LASTEXITCODE -ne 0 -or $ref -notmatch '^surface:\d+$') { throw "new-surface failed: $ref" }

& $cys rename --surface $ref '한글이름테스트' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'rename failed' }
$line = @(& $cys list | Where-Object { $_.StartsWith("$ref`t") })
if ($LASTEXITCODE -ne 0 -or $line.Count -ne 1 -or -not $line[0].Contains('label=한글이름테스트') -or -not $line[0].Contains("role=$role")) { throw 'list label or role mismatch' }

function Assert-Screen([string]$marker) {
  $deadline = (Get-Date).AddSeconds(15)
  do {
    $screen = (& $cys read-screen --surface $ref --lines 100 2>&1) -join "`n"
    if ($LASTEXITCODE -ne 0) { Write-Host "G23 screen dump (read failed):`n$screen"; throw 'read-screen failed' }
    if ($screen.Contains($marker)) { return }
    Start-Sleep -Milliseconds 300
  } while ((Get-Date) -lt $deadline)
  Write-Host "G23 screen dump (missing $marker):`n$screen"
  throw "message not on screen: $marker"
}

& $cys send --surface $ref 'echo G23_SURFACE^_DELIVERED' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'surface send failed' }
& $cys send-key --surface $ref Return | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'surface Return failed' }
Assert-Screen 'G23_SURFACE_DELIVERED'

& $cys send --to $role 'echo G23_ROLE^_DELIVERED' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'role send failed' }
& $cys send-key --to $role Return | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'role Return failed' }
Assert-Screen 'G23_ROLE_DELIVERED'

$long = '가' * 21
& $cys rename --surface $ref $long | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'long rename failed' }
$line = @(& $cys list | Where-Object { $_.StartsWith("$ref`t") })
if ($LASTEXITCODE -ne 0 -or $line.Count -ne 1 -or -not $line[0].Contains('label=' + ('가' * 20)) -or $line[0].Contains('label=' + $long)) { throw '20-character limit mismatch' }

$helpText = (& $cys rename --help 2>&1) -join "`n"
if ($LASTEXITCODE -ne 0 -or -not $helpText.Contains('사용법')) { throw 'Korean rename help missing' }
Write-Host 'GUI 항목 = 맥 VM 담당'
Write-Host 'G23 windows label PASS'
exit 0
