param([Parameter(Mandatory=$true)][string]$Directory)
$ErrorActionPreference='Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Windows proof stub only' }
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Text.RegularExpressions;
using System.Runtime.InteropServices;
public static class ScrollStub {
 public static int Parse(ref string tail,string chunk) {
  tail+=chunk; int delta=0, consumed=0;
  foreach(Match m in Regex.Matches(tail,@"\x1b\[5~|\x1b\[<64;[0-9]+;[0-9]+M|\x1b\[M`..")) {
   delta+=m.Value=="\x1b[5~"?-10:-3; consumed=m.Index+m.Length;
  }
  tail=tail.Substring(consumed); if(tail.Length>128)tail=tail.Substring(tail.Length-128);
  return delta;
 }
 static void Draw(int top) { Console.Write("\x1b[2J\x1b[H"); for(int i=top;i<top+20;i++) Console.Write("WR"+i.ToString("D4")+"\r\n"); Console.Out.Flush(); }
 [DllImport("kernel32.dll")] static extern IntPtr GetStdHandle(int n);
 [DllImport("kernel32.dll")] static extern bool SetConsoleMode(IntPtr h, uint m);
 public static void Run(string dir) {
  if(!SetConsoleMode(GetStdHandle(-10),0x200) || !SetConsoleMode(GetStdHandle(-11),7))
   throw new Exception("VT raw console mode unavailable");
  int pending=0;
  var input=Console.OpenStandardInput();
  var reader=Task.Run(()=> { try { var b=new byte[4096]; string tail=""; File.WriteAllText(Path.Combine(dir,"reader-ready.txt"),DateTime.UtcNow.ToString("o")); for(;;) {
   int n=input.Read(b,0,b.Length); if(n==0) return;
   File.AppendAllText(Path.Combine(dir,"bytes.tsv"),DateTime.UtcNow.ToString("o")+"\t"+BitConverter.ToString(b,0,n)+"\n");
   // Surrogate control: PageUp proves app redraw works; wheel reports navigate app history.
   string data=Encoding.GetEncoding(28591).GetString(b,0,n);
   Interlocked.Add(ref pending,Parse(ref tail,data));
  }} catch(Exception e) { File.WriteAllText(Path.Combine(dir,"reader-fault.txt"),e.ToString()); throw; }});
  string last=""; bool stream=false; int line=300; bool alt=false; int top=281;
  for(;;) {
   if(reader.IsCompleted) throw new Exception("input reader stopped",reader.Exception);
   string control=Path.Combine(dir,"control.txt");
   if(File.Exists(control)) {
    string value=File.ReadAllText(control).Trim();
    if(value!=last) {
     var parts=value.Split('|'); if(parts.Length!=3) throw new Exception("invalid control");
     last=value; stream=parts[2]=="stream"; line=300; alt=parts[1].StartsWith("alt-"); top=281; Interlocked.Exchange(ref pending,0);
     Console.Write("\x1b[?1049l\x1b[?1000l\x1b[?1006l\x1b[3J\x1b[2J\x1b[H");
     if(alt) { Console.Write("\x1b[?1049h"); Draw(top); }
     else for(int i=1;i<=300;i++) Console.Write("WR"+i.ToString("D4")+"\r\n");
     if(parts[1]=="sgr" || parts[1]=="alt-sgr") Console.Write("\x1b[?1000h\x1b[?1006h");
     else if(parts[1]=="legacy" || parts[1]=="alt-legacy") Console.Write("\x1b[?1000h\x1b[?1006l");
     else if(parts[1]!="plain") throw new Exception("unknown mode");
     Console.Out.Flush(); File.WriteAllText(Path.Combine(dir,"ready.txt"),last);
    }
   }
   int delta=Interlocked.Exchange(ref pending,0);
   if(alt && delta!=0){top=Math.Max(1,Math.Min(281,top+delta));Draw(top);}
   Thread.Sleep(75);
   if(stream) { Console.Write("WR"+(++line).ToString("D4")+"\r\n"); Console.Out.Flush(); }
  }
 }
}
'@
[ScrollStub]::Run($Directory)
