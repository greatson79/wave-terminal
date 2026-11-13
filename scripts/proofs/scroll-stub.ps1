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
 public static string Burst(ref int line) {
  var output=new StringBuilder();
  for(int i=0;i<5;i++)output.Append("WR"+(++line+1000).ToString("D4")+"\r\n");
  output.Append("\x1b[3A\r");
  for(int i=line-2;i<=line;i++)output.Append("\x1b[2KWR"+(i+1000).ToString("D4")+"\r\n");
  return output.ToString();
 }
 public static int Parse(ref string tail,string chunk) {
  tail+=chunk; int delta=0, consumed=0;
  foreach(Match m in Regex.Matches(tail,@"\x1b\[5~|\x1b\[<64;[0-9]+;[0-9]+M|\x1b\[M`..")) {
   delta+=m.Value=="\x1b[5~"?-10:-3; consumed=m.Index+m.Length;
  }
  tail=tail.Substring(consumed); if(tail.Length>128)tail=tail.Substring(tail.Length-128);
  return delta;
 }
 static void Draw(int top) { Console.Write("\x1b[2J\x1b[H"); for(int i=top;i<top+20;i++) Console.Write("WR"+(i+1000).ToString("D4")+"\r\n"); Console.Out.Flush(); }
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
  string last="",activeMode=""; bool stream=false,burst=false; int line=300; int burstCount=0; bool alt=false; int top=281;
  for(;;) {
   if(reader.IsCompleted) throw new Exception("input reader stopped",reader.Exception);
   string[] commands=Directory.GetFiles(dir,"control-*.txt"); Array.Sort(commands,StringComparer.Ordinal);
   if(commands.Length>0) {
    string control=commands[commands.Length-1];
    string value=File.ReadAllText(control).Trim();
    if(value!=last) {
     var parts=value.Split('|'); if(parts.Length!=3) throw new Exception("invalid control");
     if(parts[2]=="stream-go" || parts[2]=="burst-go" || parts[2]=="pause") {
      if(parts[1]!=activeMode || alt)throw new Exception("invalid streaming activation");
      last=value;stream=parts[2]!="pause";burst=parts[2]=="burst-go";File.WriteAllText(Path.Combine(dir,"ready.txt"),last);continue;
     }
     burst=false;burstCount=0;activeMode=parts[1];last=value; stream=parts[2]=="stream"; line=300; alt=parts[1].StartsWith("alt-"); top=281; Interlocked.Exchange(ref pending,0);
     Console.Write("\x1b[?1049l\x1b[?1000l\x1b[?1006l\x1b[3J\x1b[2J\x1b[H");
     if(alt) { Console.Write("\x1b[?1049h"); Draw(top); }
     else for(int i=1;i<=300;i++) Console.Write("WR"+(i+1000).ToString("D4")+"\r\n");
     if(parts[1]=="sgr" || parts[1]=="alt-sgr") Console.Write("\x1b[?1000h\x1b[?1006h");
     else if(parts[1]=="legacy" || parts[1]=="alt-legacy") Console.Write("\x1b[?1000h\x1b[?1006l");
     else if(parts[1]!="plain") throw new Exception("unknown mode");
     Console.Out.Flush(); File.WriteAllText(Path.Combine(dir,"geometry.json"),"{\"cols\":"+Console.WindowWidth+",\"rows\":"+Console.WindowHeight+"}"); File.WriteAllText(Path.Combine(dir,"ready.txt"),last);
    }
   }
   int delta=Interlocked.Exchange(ref pending,0);
   if(alt && delta!=0){top=Math.Max(1,Math.Min(281,top+delta));Draw(top);}
   Thread.Sleep(burst?150:75);
   if(stream && burst) {
    string output=Burst(ref line);Console.Write(output);Console.Out.Flush();
    File.AppendAllText(Path.Combine(dir,"producer.raw"),output);
    File.AppendAllText(Path.Combine(dir,"producer.tsv"),DateTime.UtcNow.ToString("o")+"\t"+(++burstCount)+"\t"+line+"\n");
   }
   else if(stream) { Console.Write("WR"+(++line+1000).ToString("D4")+"\r\n"); Console.Out.Flush(); }
  }
 }
}
'@
[ScrollStub]::Run($Directory)
