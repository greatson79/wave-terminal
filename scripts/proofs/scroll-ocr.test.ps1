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
 [ScrollInput]::PrepareOcr($image)
 if($image.GetPixel(0,0).R -ne 255 -or $image.GetPixel(1,0).R -ne 255 -or $image.GetPixel(2,0).R -ne 0){throw 'OCR inversion threshold boundary failed'}
} finally {$image.Dispose()}
'OCR_PIXEL_THRESHOLD_CONTROL_PASS'
