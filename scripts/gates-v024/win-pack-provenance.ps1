param([ValidateSet('install', 'post-checks')][string]$Stage = 'post-checks')
$ErrorActionPreference = 'Stop'
$pack = Join-Path $env:USERPROFILE '.cys\pack'
$manifest = Join-Path $pack '.install-manifest.json'
$pristine = Join-Path $pack '.pristine'
$hasManifest = Test-Path -LiteralPath $manifest -PathType Leaf
$hasPristine = Test-Path -LiteralPath $pristine -PathType Container
$items = 0
$files = 0
if ($hasManifest) {
  $doc = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
  $items = @($doc.PSObject.Properties).Count
}
if ($hasPristine) { $files = @(Get-ChildItem -LiteralPath $pristine -Recurse -File -Force).Count }
Write-Host "stage=$Stage manifest_exists=$hasManifest manifest_items=$items pristine_exists=$hasPristine pristine_files=$files"
if (-not $hasManifest -and -not $hasPristine) { Write-Host 'PACK PROVENANCE MISSING' }
else { Write-Host 'PACK PROVENANCE PRESENT' }
exit 0
