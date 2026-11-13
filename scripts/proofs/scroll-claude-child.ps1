param([Parameter(Mandatory=$true)][string]$Directory,[Parameter(Mandatory=$true)][string]$Claude)
$ErrorActionPreference='Stop'
$env:CLAUDE_CONFIG_DIR=Join-Path $Directory 'fresh-claude-profile'
New-Item -ItemType Directory $env:CLAUDE_CONFIG_DIR | Out-Null
Remove-Item Env:ANTHROPIC_API_KEY,Env:ANTHROPIC_AUTH_TOKEN,Env:CLAUDE_CODE_OAUTH_TOKEN -ErrorAction SilentlyContinue
@{os=[Environment]::OSVersion.VersionString;term=$env:TERM;profile=$env:CLAUDE_CONFIG_DIR;scope='fresh unauthenticated ConPTY startup only; no prompt/login selection'} | ConvertTo-Json | Set-Content (Join-Path $Directory 'child-environment.json')
$clock=[Diagnostics.Stopwatch]::StartNew()
while(-not(Test-Path (Join-Path $Directory 'start.flag'))){if($clock.ElapsedMilliseconds -gt 20000){throw 'raw attach gate timeout'};Start-Sleep -Milliseconds 50}
$token=Get-Content (Join-Path $Directory 'token.txt') -Raw
[Console]::Out.Write("F3_CHILD_START_$token`r`n")
& $Claude --safe-mode --strict-mcp-config --no-chrome --permission-mode plan
$code=$LASTEXITCODE
@{exitCode=$code;scope='startup child exited without login selection'} | ConvertTo-Json | Set-Content (Join-Path $Directory 'child-exit.json')
[Console]::Out.Write("F3_CHILD_END_$token`r`n")
