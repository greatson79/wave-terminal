function Resolve-ScrollOcr {
 param([object[]]$Candidates)
 $valid=@()
 foreach($candidate in $Candidates){
  $markers=@($candidate.markers)
  if($markers.Count -lt 3 -or @($markers | Where-Object {$_ -isnot [int] -or $_ -le 0}).Count){continue}
  $ordered=$true
  for($i=1;$i -lt $markers.Count;$i++){if($markers[$i] -ne $markers[$i-1]+1){$ordered=$false;break}}
  if($ordered){$valid+=,$candidate}
 }
 if($valid.Count -lt 2){throw 'fewer than two independently thresholded contiguous OCR reads'}
 $first=@($valid | ForEach-Object {$_.markers[0]} | Select-Object -Unique)
 if($first.Count -ne 1){throw 'contiguous OCR reads disagree on first marker'}
 $count=($valid | ForEach-Object {$_.markers.Count} | Measure-Object -Minimum).Minimum
 return @($valid[0].markers[0..([int]$count-1)])
}
