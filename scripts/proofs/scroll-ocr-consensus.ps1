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

function Read-ScrollMarkers {
 param([string]$Text,[bool]$Encoded=$false)
 if(-not $Encoded){return @([regex]::Matches($Text,'(?m)^WR(\d{4})\s*$') | ForEach-Object {[int]$_.Groups[1].Value})}
 $alphabet='1234579';$markers=@()
 foreach($line in ($Text -split "`r?`n")) {
  $line=$line.Trim();if(-not $line){continue}
  if($line -cnotmatch '^WR-+[1234579](?:-+[1234579]){4}$'){return @()}
  $payload=$line.Substring(2).Replace('-','')
  $number=0;$sum=0
  for($i=0;$i -lt 4;$i++){$digit=$alphabet.IndexOf($payload[$i]);$number=$number*7+$digit;$sum+=($i+1)*$digit}
  if($alphabet.IndexOf($payload[4]) -ne ($sum%7) -or $number -le 0){return @()}
  $markers+=,$number
 }
 return $markers
}
