$ErrorActionPreference='Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Windows disposable runner only' }
New-Item -ItemType Directory scroll-evidence | Out-Null
$version=$env:SCROLL_VERSION
$pins=@{
 '0.3.5'=@{sha='97ae76d26960975678ebb15c0741918793de162abba351a77df8d404e51a4386';source='bcfa8bc6396035885fc66ed79e5507afedb2a2bd'}
 '0.3.4'=@{sha='e5e8c61ef19c7b7bbdf5022d2f68ae0736a8b944a956352da7973a91225128f9';source='3ae14fb10e5c33b51a37727e7ad4c1fe496ff265'}
}
if(-not $pins.ContainsKey($version)){throw 'unapproved version'}
$changes=@(git diff --name-only bcfa8bc6396035885fc66ed79e5507afedb2a2bd HEAD)
foreach($f in $changes){if($f -notmatch '^(scripts/proofs/scroll-[^/]+|\.github/workflows/windows-scroll-red\.yml)$'){throw "product path changed: $f"}}
Invoke-WebRequest https://github.com/greatson79/wave-terminal/releases/latest/download/latest.json -OutFile scroll-evidence/channel-before.json
$url="https://github.com/greatson79/wave-terminal/releases/download/v$version/wave-terminal-$version-windows-x64-setup.exe"
Invoke-WebRequest $url -OutFile scroll-evidence/setup.exe
$sha=(Get-FileHash scroll-evidence/setup.exe -Algorithm SHA256).Hash
if($sha -ne $pins[$version].sha){throw 'installer hash mismatch'}
@{version=$version;sha256=$sha;source=$pins[$version].source;url=$url} | ConvertTo-Json | Set-Content scroll-evidence/identity.json
$install=Start-Process (Resolve-Path scroll-evidence/setup.exe) -ArgumentList '/S' -PassThru
if(-not $install.WaitForExit(180000) -or $install.ExitCode -ne 0){throw 'installer failed or timed out'}
$app=Join-Path $env:LOCALAPPDATA 'Wave Terminal/wave-app.exe'
if((Get-Item $app).VersionInfo.ProductVersion -ne $version){throw 'installed version mismatch'}
Remove-Item scroll-evidence/setup.exe
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS,Env:CYS_AUTOTEST_PATCH_INSTALL,Env:WAVE_AUTOTEST_PATCH_INSTALL -ErrorAction SilentlyContinue
$env:WEBVIEW2_USER_DATA_FOLDER=Join-Path $env:RUNNER_TEMP 'scroll-only-profile'
$env:CYS_NO_AUTOSTART='1'
$p=Start-Process $app -PassThru -RedirectStandardOutput scroll-evidence/app.stdout.log -RedirectStandardError scroll-evidence/app.stderr.log
@{pid=$p.Id;time=[DateTime]::UtcNow.ToString('o');path=$app;version=$version;rc=$pins[$version].source} | ConvertTo-Json | Set-Content scroll-evidence/app-start.json
@{image=$env:ImageOS;imageVersion=$env:ImageVersion;interactive=[Environment]::UserInteractive;session=(Get-Process -Id $PID).SessionId;windows=[Environment]::OSVersion.VersionString} | ConvertTo-Json | Set-Content scroll-evidence/runner-session.json
$wave=Join-Path $env:LOCALAPPDATA 'Wave Terminal/wave.exe'
$ready=$false
for($i=0;$i -lt 60;$i++){& $wave ping *> $null;if($LASTEXITCODE -eq 0){$ready=$true;break};Start-Sleep -Milliseconds 500}
if(-not $ready){throw 'HQ daemon readiness failed'}
if($env:SCROLL_CAPTURE_ONLY -eq '1'){exit 0}
node scripts/proofs/scroll-run.mjs
if($LASTEXITCODE -ne 0){throw 'native matrix incomplete; retain evidence'}
