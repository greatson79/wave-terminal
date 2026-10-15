$ErrorActionPreference = 'Stop'
$dir = Join-Path $env:LOCALAPPDATA 'Wave Terminal'
$daemon = Join-Path $dir 'cysd.exe'
$cys = Join-Path $dir 'cys.exe'
if (-not (Test-Path -LiteralPath $daemon) -or -not (Test-Path -LiteralPath $cys)) { throw 'installed daemon or CLI missing' }

function Owners {
  @((Get-CimInstance Win32_Process -Filter "Name='cysd.exe'") | Where-Object { $_.ExecutablePath -eq $daemon })
}
if ((Owners).Count -eq 0) { Start-Process -FilePath $daemon -WindowStyle Hidden | Out-Null }
$deadline = (Get-Date).AddSeconds(15)
do {
  & $cys ping 2>&1 | Out-Null
  if ($LASTEXITCODE -eq 0) { break }
  Start-Sleep -Milliseconds 500
} while ((Get-Date) -lt $deadline)
if ($LASTEXITCODE -ne 0 -or (Owners).Count -ne 1) { throw 'installed daemon did not become sole owner' }

$loser = Start-Process -FilePath $daemon -WindowStyle Hidden -PassThru
if (-not $loser.WaitForExit(15000)) { Stop-Process -Id $loser.Id -Force; throw 'duplicate daemon did not exit' }
if ($loser.ExitCode -ne 0 -or (Owners).Count -ne 1) { throw "duplicate daemon result $($loser.ExitCode), owners $((Owners).Count)" }

& $cys daemon install 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'daemon install failed' }
$logName = 'Microsoft-Windows-TaskScheduler/Operational'
wevtutil sl $logName /e:true
if ($LASTEXITCODE -ne 0) { throw 'Task Scheduler history unavailable' }
$since = Get-Date
schtasks /Run /TN cysd 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'scheduled daemon launch failed' }
Start-Sleep -Seconds 60
$task = (New-Object -ComObject 'Schedule.Service')
$task.Connect()
$registered = $task.GetFolder('\').GetTask('cysd')
if ($registered.LastTaskResult -ne 0) { throw "scheduled task result $($registered.LastTaskResult)" }
$starts = @((Get-WinEvent -FilterHashtable @{ LogName = $logName; Id = 200; StartTime = $since } -ErrorAction Stop) |
  Where-Object { $_.Message -match '\\cysd\b' })
if ($starts.Count -ne 1 -or (Owners).Count -ne 1) { throw "scheduled starts $($starts.Count), owners $((Owners).Count)" }
Write-Host 'G9 win-single-owner PASS'
