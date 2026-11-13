param([Parameter(Mandatory=$true)][int]$GuiPid,
 [Parameter(Mandatory=$true)][int]$DaemonPid,
 [Parameter(Mandatory=$true)][string]$ExpectedPath)
$ErrorActionPreference='Stop'
if($env:OS -ne 'Windows_NT' -or $env:GITHUB_ACTIONS -ne 'true'){throw 'disposable Windows runner only'}
if($GuiPid -le 0 -or $DaemonPid -le 0 -or $GuiPid -eq $DaemonPid){throw 'GUI/daemon PID boundary invalid'}
if((Split-Path -Leaf $ExpectedPath) -ne 'wave-app.exe'){throw 'expected GUI executable name invalid'}
$process=Get-Process -Id $GuiPid
try {
 # Keep the original process handle; a reused PID must never become the stop target.
 $null=$process.Handle
 $identity=Get-CimInstance Win32_Process -Filter "ProcessId=$GuiPid"
 if(-not $identity -or $identity.ExecutablePath -ne $ExpectedPath){throw 'GUI executable identity mismatch'}
 $closed=$process.CloseMainWindow()
 $forced=$false
 if(-not $process.WaitForExit(15000)) {
  $current=Get-CimInstance Win32_Process -Filter "ProcessId=$GuiPid"
  if(-not $current -or $current.ExecutablePath -ne $ExpectedPath -or $current.CreationDate -ne $identity.CreationDate){throw 'lingering GUI identity changed; refuse stop'}
  $process.Kill()
  $forced=$true
  if(-not $process.WaitForExit(15000)){throw 'GUI process survived targeted stop'}
 }
 if(Get-Process -Id $GuiPid -ErrorAction SilentlyContinue){throw 'old GUI PID still exists; no relaunch'}
 @{pid=$GuiPid;path=$identity.ExecutablePath;creationDate=$identity.CreationDate;
   closeRequested=$closed;forcedGuiStop=$forced;gone=$true;daemonPid=$DaemonPid} | ConvertTo-Json -Compress
} finally { $process.Dispose() }
