param([Parameter(Mandatory=$true)][int]$AppPid,
 [Parameter(Mandatory=$true)][string]$Title,
 [Parameter(Mandatory=$true)][string]$Role,
 [Parameter(Mandatory=$true)][int]$SurfaceId,
 [Parameter(Mandatory=$true)][string]$Out,
 [ValidateSet('snapshot','wheel','small','PageUp','ShiftPageUp','triplet','stream-wheel','stream-ShiftPageUp')][string]$Action='snapshot')
$ErrorActionPreference='Stop'
$script:CaptureClock=$null
if ($env:OS -ne 'Windows_NT') { throw 'Windows UIA only' }
if (Test-Path -LiteralPath $Out) { throw 'fresh output directory required' }
New-Item -ItemType Directory -Path $Out | Out-Null
. (Join-Path $PSScriptRoot 'scroll-readiness.ps1')
. (Join-Path $PSScriptRoot 'scroll-ocr-consensus.ps1')
$r=@{role=$Role;surfaceId=$SurfaceId;outcome='NOT_PROVEN';source='terminal-visible-markers';markers=@();action=$Action;time=[DateTime]::UtcNow.ToString('o');appPid=$AppPid;title=$Title}
function Save-ScrollScreen {
 $s=[System.Windows.Forms.SystemInformation]::VirtualScreen; $b=New-Object Drawing.Bitmap($s.Width,$s.Height)
 try { $g=[Drawing.Graphics]::FromImage($b); try { $g.CopyFromScreen($s.X,$s.Y,0,0,$s.Size) } finally { $g.Dispose() }; $b.Save((Join-Path $Out 'screen.png'),[Drawing.Imaging.ImageFormat]::Png) } finally { $b.Dispose() }
}
try {
 Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,WindowsBase,System.Drawing,System.Windows.Forms
 Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ScrollInput {
 public static void PrepareOcr(System.Drawing.Bitmap bitmap,int cutoff) {
  var data=bitmap.LockBits(new System.Drawing.Rectangle(0,0,bitmap.Width,bitmap.Height),System.Drawing.Imaging.ImageLockMode.ReadWrite,System.Drawing.Imaging.PixelFormat.Format32bppArgb);
  try { var bytes=new byte[Math.Abs(data.Stride)*data.Height]; Marshal.Copy(data.Scan0,bytes,0,bytes.Length);
   for(int i=0;i<bytes.Length;i+=4){int gray=(299*bytes[i+2]+587*bytes[i+1]+114*bytes[i]+500)/1000;byte color=(byte)(gray<cutoff?255:0);bytes[i]=bytes[i+1]=bytes[i+2]=color;bytes[i+3]=255;}
   Marshal.Copy(bytes,0,data.Scan0,bytes.Length);
  } finally { bitmap.UnlockBits(data); }
 }

 [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint type; public DATA data; }
 [StructLayout(LayoutKind.Explicit)] public struct DATA { [FieldOffset(0)] public MOUSE mouse; [FieldOffset(0)] public KEY key; }
 [StructLayout(LayoutKind.Sequential)] public struct MOUSE { public int x,y; public uint data,flags,time; public UIntPtr extra; }
 [StructLayout(LayoutKind.Sequential)] public struct KEY { public ushort vk,scan; public uint flags,time; public UIntPtr extra; }
 [DllImport("user32.dll")] static extern uint SendInput(uint n, INPUT[] a,int size);
 [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 public static void Wheel(int amount) { var a=new INPUT[]{new INPUT{type=0,data=new DATA{mouse=new MOUSE{data=unchecked((uint)amount),flags=0x800}}}}; Send(a); }
 public static void Key(ushort vk,bool up) { var a=new INPUT[]{new INPUT{type=1,data=new DATA{key=new KEY{vk=vk,flags=up?2u:0u}}}}; Send(a); }
 static void Send(INPUT[] a) { if(SendInput((uint)a.Length,a,Marshal.SizeOf(typeof(INPUT)))!=a.Length) throw new Exception("SendInput failed"); }
}
'@
 [ScrollInput]::SetProcessDPIAware() | Out-Null
 Save-ScrollScreen
 Set-Content (Join-Path $Out 'tree.json') '[]' -Encoding UTF8
 $p=Get-Process -Id $AppPid
 $windowClock=[Diagnostics.Stopwatch]::StartNew()
 do { $p.Refresh();if($p.HasExited){throw 'proof app exited'};$hwnd=$p.MainWindowHandle
  if($hwnd -ne [IntPtr]::Zero){break};Start-Sleep -Milliseconds 200
 }while($windowClock.ElapsedMilliseconds -lt 5000)
 if ($p.ProcessName -ne 'wave-app' -or $hwnd -eq [IntPtr]::Zero) { throw 'installed app window missing' }
 $root=[System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
 $walker=[System.Windows.Automation.TreeWalker]::RawViewWalker
 function Read-ScrollTree {
  $queue=New-Object 'System.Collections.Generic.Queue[object]'
  $queue.Enqueue(@{element=$root;parent=-1})
  $script:nodes=New-Object 'System.Collections.Generic.List[object]'
  $script:elements=New-Object 'System.Collections.Generic.List[object]'
  while($queue.Count){
   if($script:nodes.Count -ge 10000){throw 'raw UIA tree limit exceeded'}
   $item=$queue.Dequeue();$e=$item.element;$c=$e.Current;$i=$script:nodes.Count
   $script:elements.Add($e)
   $script:nodes.Add([pscustomobject]@{index=$i;parent=$item.parent;name=$c.Name;class=$c.ClassName;id=$c.AutomationId;type=$c.ControlType.ProgrammaticName;offscreen=$c.IsOffscreen})
   if(($script:nodes.Count % 256) -eq 0){$script:nodes | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $Out 'tree.json') -Encoding UTF8}
   $child=$walker.GetFirstChild($e)
   while($null -ne $child){
    if(($queue.Count+$script:nodes.Count) -ge 10000){throw 'raw UIA queue limit exceeded'}
    $queue.Enqueue(@{element=$child;parent=$i});$child=$walker.GetNextSibling($child)
   }
  }
  $script:nodes | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $Out 'tree.json') -Encoding UTF8
  return $script:nodes.ToArray()
 }
 $ready=Wait-ScrollPage -ReadTree { Read-ScrollTree } -Role $Role -SurfaceId $SurfaceId
 $r.readiness=@{attempts=$ready.attempts;elapsedMs=$ready.elapsedMs;headingPrefix=$ready.headingPrefix;nodeCount=$ready.nodes.Count}
 $scope=Find-ScrollPane -Nodes $ready.nodes -HeaderIndex $ready.labelIndex
 $pane=$script:elements[$scope.paneIndex]
 $children=@($scope.descendants | ForEach-Object { $script:elements[$_] })
 $edit=@($script:elements[$scope.inputIndex])
 [ScrollInput]::SetForegroundWindow($hwnd) | Out-Null
 $edit[0].SetFocus(); Start-Sleep -Milliseconds 100
 if ([ScrollInput]::GetForegroundWindow() -ne $hwnd) { throw 'app not foreground' }
 $focus=[System.Windows.Automation.AutomationElement]::FocusedElement
 if (-not [System.Windows.Automation.Automation]::Compare($focus,$edit[0])) { throw 'proof terminal not focused' }
 $bounds=$pane.Current.BoundingRectangle
 $appBounds=$root.Current.BoundingRectangle
 $r.originalPaneBounds=@($bounds.X,$bounds.Y,$bounds.Width,$bounds.Height)
 # A horizontally overflowing pane is cropped to its own visible app/screen intersection.
 $bounds=[Windows.Rect]::Intersect($bounds,$appBounds)
 $desktop=[System.Windows.Forms.SystemInformation]::VirtualScreen
 $bounds=[Windows.Rect]::Intersect($bounds,[Windows.Rect]::new($desktop.X,$desktop.Y,$desktop.Width,$desktop.Height))
 if($pane.Current.IsOffscreen -or $bounds.IsEmpty -or $bounds.Width -lt 100 -or $bounds.Height -lt 100){throw 'bound terminal has no usable visible app/screen intersection'}
 $r.bounds=@($bounds.X,$bounds.Y,$bounds.Width,$bounds.Height)
 # Read the marker stripe above lower-right notifications, within the same bound pane.
 $ocrBounds=[Windows.Rect]::new($bounds.X,$bounds.Y+35,[Math]::Min(120,$bounds.Width),[Math]::Min(350,$bounds.Height-35))
 $r.ocrBounds=@($ocrBounds.X,$ocrBounds.Y,$ocrBounds.Width,$ocrBounds.Height)
 function Capture-Terminal([string]$name) {
  # UIA identifies the pane; OCR observes only its rendered pixels.
  $bitmap=New-Object Drawing.Bitmap([int]$ocrBounds.Width,[int]$ocrBounds.Height)
  try {
   $g=[Drawing.Graphics]::FromImage($bitmap)
   try { $g.CopyFromScreen([int]$ocrBounds.X,[int]$ocrBounds.Y,0,0,$bitmap.Size);$script:CapturedAt=[DateTime]::UtcNow.ToString('o');if($script:CaptureClock){$script:CapturedElapsed=$script:CaptureClock.ElapsedMilliseconds} } finally { $g.Dispose() }
   $large=New-Object Drawing.Bitmap($bitmap,([Drawing.Size]::new($bitmap.Width*3,$bitmap.Height*3)))
   try { $large.Save((Join-Path $Out ($name+'.raw.png')),[Drawing.Imaging.ImageFormat]::Png); [ScrollInput]::PrepareOcr($large,155); $large.Save((Join-Path $Out $name),[Drawing.Imaging.ImageFormat]::Png) } finally { $large.Dispose() }
  } finally { $bitmap.Dispose() }
 }
 function Read-VisibleMarkers([string]$name) {
  $candidates=@()
  foreach($cutoff in @(155,135,125)) {
   $candidateName=$name+".cut$cutoff.png"
   $candidate=[Drawing.Bitmap]::new((Join-Path $Out ($name+'.raw.png')))
   try { [ScrollInput]::PrepareOcr($candidate,$cutoff);$candidate.Save((Join-Path $Out $candidateName),[Drawing.Imaging.ImageFormat]::Png) } finally {$candidate.Dispose()}
   $text=& 'C:/Program Files/Tesseract-OCR/tesseract.exe' (Join-Path $Out $candidateName) stdout --psm 6 -c tessedit_char_whitelist=WR0123456789 2> (Join-Path $Out ($candidateName+'.stderr.txt'))
   if($LASTEXITCODE -ne 0){throw 'terminal OCR failed'}
   $text | Set-Content (Join-Path $Out ($candidateName+'.txt')) -Encoding UTF8
   $markers=@([regex]::Matches(($text -join "`n"),'(?m)^WR(\d{4})\s*$') | ForEach-Object { [int]$_.Groups[1].Value })
   $candidates+=,@{cutoff=$cutoff;markers=$markers}
  }
  $r.ocrCandidates=$candidates
  $r.markers=@(Resolve-ScrollOcr -Candidates $candidates)
  $r.observer='pane-crop-tesseract';$r.outcome='VISIBLE_MARKERS'
 }
 Capture-Terminal 'observer.png'
 Read-VisibleMarkers 'observer.png'
 Capture-Terminal 'pre-input.png'
 $r.preInputTime=$script:CapturedAt
 if ($Action -in @('wheel','small','triplet','stream-wheel')) {
  [ScrollInput]::SetCursorPos([int]($bounds.X+$bounds.Width/2),[int]($bounds.Y+$bounds.Height/2)) | Out-Null
  if ($Action -in @('triplet','stream-wheel')) {for($i=0;$i -lt 3;$i++){[ScrollInput]::Wheel(120);if($i -lt 2){Start-Sleep -Milliseconds 16}};$r.input=@{kind='native-wheel';delta=120;count=3;unit='native units, DOM deltaMode unobserved'}}
  elseif ($Action -eq 'wheel') { [ScrollInput]::Wheel(120); $r.input=@{kind='native-wheel';delta=120;unit='WHEEL_DELTA units; DOM deltaMode unobserved'} }
  else { for($i=0;$i -lt 40;$i++) { [ScrollInput]::Wheel(4); Start-Sleep -Milliseconds 16 }; $r.input=@{kind='native-small-wheel';delta=4;count=40;unit='native wheel units, NOT pixels'} }
 } elseif ($Action -in @('PageUp','ShiftPageUp','stream-ShiftPageUp')) {
  try { if ($Action -in @('ShiftPageUp','stream-ShiftPageUp')) { [ScrollInput]::Key(0x10,$false) }; [ScrollInput]::Key(0x21,$false) }
  finally { [ScrollInput]::Key(0x21,$true); if ($Action -in @('ShiftPageUp','stream-ShiftPageUp')) { [ScrollInput]::Key(0x10,$true) } }
  $r.input=@{kind='native-key';key=$Action}
 }
 if($Action.StartsWith('stream-')) {
  $script:CaptureClock=[Diagnostics.Stopwatch]::StartNew();$r.inputEndedAt=[DateTime]::UtcNow.ToString('o');$captures=@()
  foreach($target in @(50,150,700,2800)) {
   while($script:CaptureClock.ElapsedMilliseconds -lt $target){Start-Sleep -Milliseconds ([Math]::Max(1,$target-$script:CaptureClock.ElapsedMilliseconds))}
   $name="frame-$target.png";Capture-Terminal $name
   $captures+=,@{name=$name;elapsedMs=$script:CapturedElapsed;time=$script:CapturedAt}
  }
  Read-VisibleMarkers 'pre-input.png';$r.preInputMarkers=@($r.markers);$r.frames=@()
  foreach($capture in $captures) {
   Read-VisibleMarkers $capture.name
   $r.frames+=,@{name=$capture.name;elapsedMs=$capture.elapsedMs;time=$capture.time;source='terminal-visible-markers';markers=@($r.markers);ocrCandidates=$r.ocrCandidates}
  }
 } else {
 Start-Sleep -Milliseconds 150
 Capture-Terminal 'post-input.png'
 $r.postInputTime=[DateTime]::UtcNow.ToString('o')
 Read-VisibleMarkers 'pre-input.png'
 $r.preInputMarkers=@($r.markers)
 Read-VisibleMarkers 'post-input.png'
 }
 if ($r.markers.Count -ge 2) { $r.outcome='VISIBLE_MARKERS' }
} catch { $r.error=$_.ToString() }
finally {
 try {
  Save-ScrollScreen
 } catch { $r.captureError=$_.ToString() }
 $r | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $Out 'observation.json') -Encoding UTF8
}
if ($r.error) { exit 2 }
