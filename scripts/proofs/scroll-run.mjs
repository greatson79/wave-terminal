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
 const initialStatus=JSON.parse(cli(['status','--json']));save('initial-status.json',initialStatus);
 let commandNumber=0;
 async function publishControl(dir,value) {
  writeFileSync(join(dir,'next.txt'),value);renameSync(join(dir,'next.txt'),join(dir,`control-${String(++commandNumber).padStart(4,'0')}.txt`));
  const deadline=Date.now()+10000;
  while(!existsSync(join(dir,'ready.txt')) || readFileSync(join(dir,'ready.txt'),'utf8')!==value){assert.ok(Date.now()<deadline,'stub control acknowledgement timeout');await wait(100);}
 }
 for(const mode of ['plain','sgr','legacy','alt-sgr','alt-legacy']) {
  const title=`WR-${mode}`;
  const dir=join(out,mode);mkdirSync(dir);
  const command=`powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -Command "& ${quote(resolve('scripts/proofs/scroll-stub.ps1'))} -Directory ${quote(dir)}"`;
  const ref=cli(['new-surface','--cmd',command,'--cwd',process.env.RUNNER_TEMP,'--title',title,'--role',`scroll-${mode}`]);
  assert.match(ref,/^surface:\d+$/);created.push({ref,title});save('created.json',created);
  // Bind the rendered proof pane before taking the startup-shell snapshot.
  const layout=await observe(title,ref,join(out,`${mode}-layout`),'snapshot');
  assert.ok(layout.readiness && layout.bounds,'rendered proof-pane layout not ready');
  const live=JSON.parse(cli(['status','--json']));
  assert.equal(live.daemon.build_id,initialStatus.daemon.build_id,'daemon changed during setup');
  const others=live.surfaces.filter(s=>!s.exited && s.surface_ref!==ref);
  assert.ok(others.every(s=>!s.agent && !s.role),'unexpected runner agent; do not close it');
  for(const seat of others)cli(['close-surface',seat.surface_ref]);
  save(`${mode}-startup-shells-closed.json`,others.map(s=>s.surface_ref));await wait(500);
  try {
   // Positive receipt control separates a dropped report from a dead input reader.
   const health=`health|${mode}|idle`;
   await publishControl(dir,health);
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
    // Native PageUp restores product follow before fresh output; prove its receipt.
    const resetOffset=readFileSync(bytePath).length;
    const resetInput=await observe(title,ref,join(out,`${id}-reset-input`),'PageUp');
    assert.ok(!resetInput.error && !resetInput.helperError && resetInput.input,'reset PageUp not injected');
    await wait(100);
    const resetBytes=readFileSync(bytePath).subarray(resetOffset).toString('utf8');
    assert.ok(resetBytes.split('\n').map(line=>(line.split('\t')[1]??'').replaceAll('-','')).join('').includes('1B5B357E'),'fresh reset PageUp receipt absent');
    save(`${id}-reset-control.json`,{receivedBytesTsv:resetBytes,input:resetInput.input});
    await publishControl(dir,`${id}|${mode}|idle`);
    await wait(400);
    // CLI read-screen is retained as PTY/output readiness only, never viewport evidence.
    const screen=cli(['read-screen','--surface',ref,'--lines','40']);
    writeFileSync(join(out,`${id}-pty.txt`),screen);
    assert.match(screen,/WR\d{4}/,'numbered PTY output missing');
    const before=await observe(title,ref,join(out,`${id}-before`),'snapshot');
    assert.equal(classifyViewport(initial,before),'STATIONARY','fresh history baseline differs from initial bottom; no trial');
    if(stream==='stream') await publishControl(dir,`${id}-start|${mode}|stream-go`);
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
