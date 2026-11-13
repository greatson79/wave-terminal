import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdirSync,existsSync,renameSync,appendFileSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {classifyViewport,hasNoOlderResponse} from './scroll-contract.mjs';
assert.equal(process.platform,'win32','Windows runner only; no native Mac launch');
const out=resolve(process.env.SCROLL_OUT || 'scroll-evidence');
const load=p=>JSON.parse(readFileSync(p,'utf8').replace(/^\uFEFF/,''));
const app=load(join(out,'app-start.json'));
const wave=join(process.env.LOCALAPPDATA,'Wave Terminal','wave.exe');
const cli=args=>execFileSync(wave,args,{encoding:'utf8',timeout:15000,windowsHide:true}).trim();
const wait=ms=>new Promise(r=>setTimeout(r,ms));
const rows=[],created=[];
const save=(n,v)=>writeFileSync(join(out,n),JSON.stringify(v,null,2)+'\n');
const ps=join(process.env.SystemRoot,'System32','WindowsPowerShell','v1.0','powershell.exe');
const quote=s=>`'${s.replaceAll("'","''")}'`;
async function observe(title,ref,dir,action) {
 let error;
 try { execFileSync(ps,['-NoProfile','-NonInteractive','-Mta','-ExecutionPolicy','Bypass','-File',resolve('scripts/proofs/scroll-uia.ps1'),'-AppPid',String(app.pid),'-Title',title,'-Role',`scroll-${title.slice(3)}`,'-SurfaceId',ref.slice(8),'-Out',dir,'-Action',action],{timeout:35000,windowsHide:true,stdio:'pipe'}); }
 catch(e) { error=String(e); }
 const observation=existsSync(join(dir,'observation.json'))?load(join(dir,'observation.json')):{source:'unavailable',markers:[]};
 if(error) observation.helperError=error;
 if(error || observation.captureError || observation.appPid!==app.pid || observation.title!==title || !existsSync(join(dir,'screen.png'))) observation.source='unavailable';
 return observation;
}
try {
 assert.equal(process.env.CYS_NO_AUTOSTART,'1');
 assert.ok(!process.env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS,'no CDP retry route');
 save('initial-status.json',JSON.parse(cli(['status','--json'])));
 for(const mode of ['plain','sgr','legacy','alt-sgr','alt-legacy']) {
  const title=`WR-${mode}`;
  const dir=join(out,mode);mkdirSync(dir);
  const command=`powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -Command "& ${quote(resolve('scripts/proofs/scroll-stub.ps1'))} -Directory ${quote(dir)}"`;
  const ref=cli(['new-surface','--cmd',command,'--cwd',process.env.RUNNER_TEMP,'--title',title,'--role',`scroll-${mode}`]);
  assert.match(ref,/^surface:\d+$/);created.push({ref,title});save('created.json',created);
  try {
   // Positive receipt control separates a dropped report from a dead input reader.
   const health=`health|${mode}|idle`;
   writeFileSync(join(dir,'control.txt'),health);
   const healthDeadline=Date.now()+10000;
   while(!existsSync(join(dir,'ready.txt')) || readFileSync(join(dir,'ready.txt'),'utf8')!==health || !existsSync(join(dir,'reader-ready.txt'))) {
    assert.ok(Date.now()<healthDeadline,'stub/reader readiness not established');await wait(100);
   }
   await wait(400);
   const initial=await observe(title,ref,join(out,`${mode}-observer-first`),'snapshot');
   assert.ok(!initial.error && !initial.helperError && initial.markers.length>=3,'visible-marker observer unavailable; zero native rows = not executed');
   const healthInput=await observe(title,ref,join(out,`${mode}-reader-control`),'PageUp');
   assert.ok(!healthInput.helperError && !healthInput.error && healthInput.input,'native positive control injection unavailable');
   await wait(200);
   if(mode.startsWith('alt-')) assert.equal(classifyViewport(initial,healthInput),'MOVED_UP','surrogate PageUp redraw positive control failed');
   const bytePath=join(dir,'bytes.tsv');
   const hex=existsSync(bytePath)?readFileSync(bytePath,'utf8').trim().split('\n').map(s=>(s.split('\t')[1]??'').replaceAll('-','')).join(''):'';
   assert.ok(hex.includes('1B5B357E'),'PageUp ESC[5~ did not reach stub; input receipt not proven');
   save(`${mode}-reader-control.json`,{expectedHex:'1B5B357E',receivedHex:hex,readerReady:true});
   for(const stream of (mode.startsWith('alt-')?['idle']:['idle','stream'])) for(const action of (mode.startsWith('alt-')?['wheel','ShiftPageUp']:['wheel','small','PageUp','ShiftPageUp'])) {
    const id=`${mode}-${stream}-${action}`;
    const control=`${id}|${mode}|${stream}`;
    writeFileSync(join(dir,'next.txt'),control);renameSync(join(dir,'next.txt'),join(dir,'control.txt'));
    const deadline=Date.now()+10000;
    while(!existsSync(join(dir,'ready.txt')) || readFileSync(join(dir,'ready.txt'),'utf8')!==control) {
     assert.ok(Date.now()<deadline,'stub did not start/reset; launch not proven');await wait(100);
    }
    await wait(400);
    // CLI read-screen is retained as PTY/output readiness only, never viewport evidence.
    const screen=cli(['read-screen','--surface',ref,'--lines','40']);
    writeFileSync(join(out,`${id}-pty.txt`),screen);
    assert.match(screen,/WR\d{4}/,'numbered PTY output missing');
    const before=await observe(title,ref,join(out,`${id}-before`),'snapshot');
    const bytesPath=join(dir,'bytes.tsv');
    const offset=existsSync(bytesPath)?readFileSync(bytesPath).length:0;
    const immediate=await observe(title,ref,join(out,`${id}-input`),action);
    await wait(700);
    const after=await observe(title,ref,join(out,`${id}-after`),'snapshot');
    const received=existsSync(bytesPath)?readFileSync(bytesPath).subarray(offset).toString('utf8'):'';
    assert.ok(!existsSync(join(dir,'reader-fault.txt')),'input reader faulted');
    const row={id,mode,stream,action,ref,before,immediate,after,viewport:classifyViewport(before,after),receivedBytesTsv:received,inputDelivery:received?'APP_BYTES_OBSERVED':'NO_APP_BYTES_OBSERVED',injection:immediate.input&&!immediate.helperError&&!immediate.error?'NATIVE_SENDINPUT_ACCEPTED':'NOT_PROVEN',hardware:'NOT_TESTED',zeroResponse:hasNoOlderResponse({...immediate,markers:immediate.preInputMarkers},immediate,after),seatKind:mode.startsWith('alt-')?'CLAUDE_ALT_SURROGATE':'NORMAL_RAW_SHELL'};
    rows.push(row);appendFileSync(join(out,'matrix.jsonl'),JSON.stringify(row)+'\n');
   }
  } finally { cli(['close-surface',ref]); }
 }
 save('result.json',{outcome:rows.every(r=>r.viewport!=='NOT_PROVEN' && r.injection!=='NOT_PROVEN')?'MATRIX_MEASURED':'NOT_PROVEN',rows:rows.length,version:process.env.SCROLL_VERSION,claudeActual:'NOT_TESTED; alt rows are app-history surrogates',nativeUnitsAreNotDomPixels:true});
 const failures=rows.filter(r=>r.action==='wheel' && r.stream==='idle' && r.zeroResponse===true).map(r=>r.id);
 save('red.json',{outcome:failures.length?'RED':'NO_RED_OBSERVED',version:process.env.SCROLL_VERSION,failedCases:failures,rows:rows.length,actualClaude:'NOT_TESTED',scope:'idle native wheel no older-marker response in near-input pair AND delayed sample; stream rows diagnostic only; alt rows surrogate only'});
 assert.ok(rows.length===28 && rows.every(r=>r.viewport!=='NOT_PROVEN' && r.injection!=='NOT_PROVEN'),'native matrix incomplete; inspect retained evidence; no retry');
} catch(error) { save('failure.json',{error:String(error),rows:rows.length,created});throw error; }
