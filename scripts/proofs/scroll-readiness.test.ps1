param([switch]$NegativeControl)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'scroll-readiness.ps1')
$prefix='scroll-plain'+[char]0xB7+'s2'
$hostOnly=@([pscustomobject]@{index=0;parent=-1;type='ControlType.Pane';name='Wave Terminal';offscreen=$false})
$ready=@([pscustomobject]@{index=0;parent=-1;type='ControlType.Document';name='';offscreen=$false},[pscustomobject]@{index=1;parent=0;type='ControlType.Text';name=($prefix+' '+[char]0xB7+' CTX');offscreen=$false})
if ($NegativeControl) {
 Wait-ScrollPage -ReadTree { $hostOnly } -Role scroll-plain -SurfaceId 2 -TimeoutMs 20 -PollMs 1 | Out-Null
 throw 'NEGATIVE_CONTROL_BUG: host-only tree was accepted'
}
$script:attempt=0
$r=Wait-ScrollPage -ReadTree { $script:attempt++; if($script:attempt -eq 1){$hostOnly}else{$ready} } -Role scroll-plain -SurfaceId 2 -TimeoutMs 1000 -PollMs 1
if ($r.attempts -ne 2 -or $r.labelIndex -ne 1) { throw 'did not wait for actual page/header' }
'PASS delayed page/header requires two observations'
foreach($bad in @('missing-page','wrong-seat','wrong-title','duplicate','offscreen','outside-document')) {
 $script:badNodes=@($ready | ForEach-Object { [pscustomobject]@{index=$_.index;parent=$_.parent;type=$_.type;name=$_.name;offscreen=$_.offscreen} })
 switch($bad) {
  'missing-page' { $script:badNodes[0].type='ControlType.Pane' }
  'wrong-seat' { $script:badNodes[1].name='scroll-plain'+[char]0xB7+'s20' }
  'wrong-title' { $script:badNodes[1].name='WR-plain' }
  'duplicate' { $script:badNodes+= [pscustomobject]@{index=2;parent=0;type='ControlType.Text';name=$ready[1].name;offscreen=$false} }
  'offscreen' { $script:badNodes[1].offscreen=$true }
  'outside-document' { $script:badNodes[1].parent=-1 }
 }
 $rejected=$false
 try { Wait-ScrollPage -ReadTree { $script:badNodes } -Role scroll-plain -SurfaceId 2 -TimeoutMs 20 -PollMs 1 | Out-Null }
 catch { if($_ -notmatch 'SCROLL_READINESS_TIMEOUT'){throw};$rejected=$true }
 if(-not $rejected){throw "accepted invalid readiness: $bad"}
 "PASS rejects $bad"
}
$scope=@(
 [pscustomobject]@{index=0;parent=-1;type='ControlType.Document';name='';offscreen=$false},
 [pscustomobject]@{index=1;parent=0;type='ControlType.Group';class='pane';name='';offscreen=$false},
 [pscustomobject]@{index=2;parent=1;type='ControlType.Text';name=$prefix;offscreen=$false},
 [pscustomobject]@{index=3;parent=1;type='ControlType.Edit';name='Terminal input';offscreen=$false})
$p=Find-ScrollPane -Nodes $scope -HeaderIndex 2
if($p.paneIndex -ne 1 -or $p.inputIndex -ne 3){throw 'incorrect bound terminal scope'}
'PASS input belongs to independently bound pane class'
$scope[3].parent=0
$rejected=$false
try { Find-ScrollPane -Nodes $scope -HeaderIndex 2 | Out-Null } catch { $rejected=$true }
if(-not $rejected){throw 'whole Document accepted as terminal scope'}
'PASS rejects whole-page fallback'
$cross=@(
 [pscustomobject]@{index=0;parent=-1;type='ControlType.Document';name='';offscreen=$false},
 [pscustomobject]@{index=1;parent=0;type='ControlType.Group';name='';offscreen=$false},
 [pscustomobject]@{index=2;parent=1;type='ControlType.Group';name='';offscreen=$false},
 [pscustomobject]@{index=3;parent=2;type='ControlType.Text';name=$prefix;offscreen=$false},
 [pscustomobject]@{index=4;parent=1;type='ControlType.Group';name='';offscreen=$false},
 [pscustomobject]@{index=5;parent=4;type='ControlType.Text';name=('s1 '+[char]0xB7+' ');offscreen=$false},
 [pscustomobject]@{index=6;parent=4;type='ControlType.Edit';name='Terminal input';offscreen=$false})
$rejected=$false
try { Find-ScrollPane -Nodes $cross -HeaderIndex 3 | Out-Null } catch { $rejected=$true }
if(-not $rejected){throw 'cross-seat input binding accepted'}
'PASS rejects other-seat input under shared layout'
$rejected=$false
try { Wait-ScrollPage -ReadTree {  $late=[Diagnostics.Stopwatch]::StartNew(); while($late.ElapsedMilliseconds -lt 100){[Threading.Thread]::Sleep(5)}; $ready } -Role scroll-plain -SurfaceId 2 -TimeoutMs 20 -PollMs 1 | Out-Null } catch { if($_ -notmatch 'SCROLL_READINESS_TIMEOUT'){throw};$rejected=$true }
if(-not $rejected){throw 'late readiness accepted after deadline'}
'PASS rejects readiness arriving after deadline'

# Partial provider tree omits the other seat header; identity cannot be inferred from absence.
$partial=@(
 [pscustomobject]@{index=0;parent=-1;type='ControlType.Document';name='';offscreen=$false},
 [pscustomobject]@{index=1;parent=0;type='ControlType.Group';class='layout';name='';offscreen=$false},
 [pscustomobject]@{index=2;parent=1;type='ControlType.Group';class='pane';name='';offscreen=$false},
 [pscustomobject]@{index=3;parent=2;type='ControlType.Text';name=$prefix;offscreen=$false},
 [pscustomobject]@{index=4;parent=1;type='ControlType.Group';class='pane';name='';offscreen=$false},
 [pscustomobject]@{index=5;parent=4;type='ControlType.Edit';name='Terminal input';offscreen=$false})
$rejected=$false
try { Find-ScrollPane -Nodes $partial -HeaderIndex 3 | Out-Null } catch { $rejected=$true }
if(-not $rejected){throw 'partial UIA crossed the positive pane boundary'}
'PASS rejects partial-tree other-seat input'
$scope[3].parent=1;$scope[1].class=''
$rejected=$false
try { Find-ScrollPane -Nodes $scope -HeaderIndex 2 | Out-Null } catch { $rejected=$true }
if(-not $rejected){throw 'input selected without positive pane boundary'}
'PASS rejects missing positive pane identity'
