# The visible RC header uses role+surface id, not the CLI's stored --title.
function Wait-ScrollPage {
 param([Parameter(Mandatory=$true)][scriptblock]$ReadTree,
       [Parameter(Mandatory=$true)][string]$Role,
       [ValidateRange(1,2147483647)][int]$SurfaceId,
       [ValidateRange(1,20000)][int]$TimeoutMs=20000,
       [ValidateRange(1,200)][int]$PollMs=200)
 $prefix=$Role+[char]0xB7+'s'+$SurfaceId
 $pattern='^'+[regex]::Escape($prefix)+'(?:\s+'+[char]0xB7+'(?:\s|$)|$)'
 $clock=[Diagnostics.Stopwatch]::StartNew();$attempt=0
 do {
  $nodes=@(& $ReadTree);$attempt++
  $labels=@($nodes | Where-Object { $_.type -eq 'ControlType.Text' -and -not $_.offscreen -and $_.name -cmatch $pattern })
  $inDocument=$false
  if($labels.Count -eq 1){
   $index=$labels[0].parent
   for($n=0;$n -lt $nodes.Count -and $index -ge 0;$n++){
    $parent=@($nodes | Where-Object { $_.index -eq $index })
    if($parent.Count -ne 1){break}
    if($parent[0].type -eq 'ControlType.Document' -and -not $parent[0].offscreen){$inDocument=$true;break}
    $index=$parent[0].parent
   }
  }
  if($inDocument -and $clock.ElapsedMilliseconds -lt $TimeoutMs){
   return @{attempts=$attempt;elapsedMs=$clock.ElapsedMilliseconds;labelIndex=$labels[0].index;nodes=$nodes;headingPrefix=$prefix}
  }
  if($clock.ElapsedMilliseconds -ge $TimeoutMs){throw "SCROLL_READINESS_TIMEOUT: page/unique visible $prefix unavailable; attempts=$attempt nodes=$($nodes.Count)"}
  Start-Sleep -Milliseconds ([Math]::Min($PollMs,[Math]::Max(1,$TimeoutMs-$clock.ElapsedMilliseconds)))
 }while($true)
}

function Find-ScrollPane {
 param([object[]]$Nodes,[int]$HeaderIndex)
 # Preserve the original positive pane-class boundary; partial trees must not widen scope.
 $pane=$Nodes[$HeaderIndex].parent
 for($n=0;$n -lt $Nodes.Count -and $pane -ge 0;$n++){
  if($Nodes[$pane].type -eq 'ControlType.Document'){break}
  if($Nodes[$pane].class -match '(^|\s)pane(\s|$)'){break}
  $pane=$Nodes[$pane].parent
 }
 if($pane -lt 0 -or $Nodes[$pane].class -notmatch '(^|\s)pane(\s|$)'){throw 'positive terminal pane boundary unavailable'}
 $descendants=@(for($i=0;$i -lt $Nodes.Count;$i++){
  $parent=$Nodes[$i].parent
  for($j=0;$j -lt $Nodes.Count -and $parent -ge 0;$j++){
   if($parent -eq $pane){$i;break};$parent=$Nodes[$parent].parent
  }
 })
 if(@($descendants | Where-Object { $Nodes[$_].class -match '(^|\s)pane(\s|$)' }).Count){throw 'nested pane boundary ambiguous'}
 $inputs=@($descendants | Where-Object { $Nodes[$_].type -eq 'ControlType.Edit' -and $Nodes[$_].name -ceq 'Terminal input' -and -not $Nodes[$_].offscreen })
 if($inputs.Count -ne 1){throw 'unique input inside bound pane unavailable'}
 return @{paneIndex=$pane;inputIndex=$inputs[0];descendants=$descendants}
}
