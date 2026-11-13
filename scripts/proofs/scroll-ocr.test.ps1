$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing
$source=Get-Content (Join-Path $PSScriptRoot 'scroll-uia.ps1') -Raw
$code=[regex]::Match($source,"(?s)@'\r?\n(.*?)\r?\n'@").Groups[1].Value
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition $code
$image=New-Object Drawing.Bitmap(3,1)
try {
 $image.SetPixel(0,0,[Drawing.Color]::FromArgb(10,10,10))
 $image.SetPixel(1,0,[Drawing.Color]::FromArgb(154,154,154))
 $image.SetPixel(2,0,[Drawing.Color]::FromArgb(155,155,155))
 [ScrollInput]::PrepareOcr($image,155)
 if($image.GetPixel(0,0).R -ne 255 -or $image.GetPixel(1,0).R -ne 255 -or $image.GetPixel(2,0).R -ne 0){throw 'OCR inversion threshold boundary failed'}
} finally {$image.Dispose()}
'OCR_PIXEL_THRESHOLD_CONTROL_PASS'

. (Join-Path $PSScriptRoot 'scroll-ocr-consensus.ps1')
$good=@{markers=@(1258,1259,1260)}
$result=@(Resolve-ScrollOcr -Candidates @($good,$good,@{markers=@(1258,1259,1266)}))
if(($result -join ',') -ne '1258,1259,1260'){throw 'valid consensus not returned'}
foreach($bad in @(
 @{candidates=@($good,@{markers=@(1260,1261,1262)})},
 @{candidates=@($good,@{markers=@(1258,1259,1266)})},
 @{candidates=@(@{markers=@(1258,1258,1259)},@{markers=@(1258,1258,1259)})}
)) {
 $rejected=$false
 try {Resolve-ScrollOcr -Candidates $bad.candidates | Out-Null} catch {$rejected=$true}
 if(-not $rejected){throw 'ambiguous/invalid OCR consensus accepted'}
}
'OCR_CONSENSUS_CONTROLS_PASS'

$decoded=@(Read-ScrollMarkers -Text "WR44134`nWR44312`nWR44443" -Encoded $true)
if(($decoded -join ',') -ne '1178,1190,1200'){throw 'encoded marker/checksum vectors failed'}
foreach($bad in @('WR44302','WR44362','WR44382','WR44313','WR11111')) {
 if(@(Read-ScrollMarkers -Text $bad -Encoded $true).Count){throw 'ambiguous or bad-checksum marker accepted'}
}
'ENCODED_MARKER_NEGATIVE_CONTROLS_PASS'

$valid="WR39393`nWR39413`nWR39429`nWR39434"
if((@(Read-ScrollMarkers -Text $valid -Encoded $true) -join ',') -ne '1000,1001,1002,1003'){throw 'valid contiguous encoded run failed'}
foreach($poison in @('WR443 12','WR44312 junk','WR','R44312','WR44313')) {
 foreach($mixed in @(($poison+"`n"+$valid),($valid+"`n"+$poison))) {
  if(@(Read-ScrollMarkers -Text $mixed -Encoded $true).Count){throw 'malformed first/last line silently skipped'}
 }
}
'ENCODED_WHOLE_READ_REJECTION_PASS'
