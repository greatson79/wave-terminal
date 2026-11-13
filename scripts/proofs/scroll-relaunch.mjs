import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdirSync,existsSync,renameSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {classifyViewport} from './scroll-contract.mjs';
import {classifyRelaunch} from './scroll-relaunch-contract.mjs';

assert.equal(process.platform,'win32','Windows disposable runner only');
assert.equal(process.env.GITHUB_ACTIONS,'true','never relaunch a real workstation app');
assert.equal(process.env.CYS_NO_AUTOSTART,'1');
const out=resolve(process.env.SCROLL_OUT || 'scroll-evidence');
const load=p=>JSON.parse(readFileSync(p,'utf8').replace(/^\uFEFF/,''));
const app=load(join(out,'app-start.json'));
const wave=join(process.env.LOCALAPPDATA,'Wave Terminal','wave.exe');
const cli=args=>execFileSync(wave,args,{encoding:'utf8',timeout:15000,windowsHide:true}).trim();
const ps=join(process.env.SystemRoot,'System32','WindowsPowerShell','v1.0','powershell.exe');
const quote=s=>`'${s.replaceAll("'","''")}'`;
const save=(n,v)=>writeFileSync(join(out,n),JSON.stringify(v,null,2)+'\n');
const wait=ms=>new Promise(r=>setTimeout(r,ms));
let ref,identity,baseline,control=false;
const attempts=[];
async function observe(name,action='snapshot') {
 const dir=join(out,`relaunch-${name}`);
 let error;
 try { execFileSync(ps,['-NoProfile','-NonInteractive','-Mta','-ExecutionPolicy','Bypass','-File',resolve('scripts/proofs/scroll-uia.ps1'),'-AppPid',String(app.pid),'-Title','WR-relaunch','-Role','scroll-relaunch','-SurfaceId',ref.slice(8),'-Out',dir,'-Action',action],{timeout:35000,windowsHide:true}); }
 catch(e) { error=String(e); }
 const value=existsSync(join(dir,'observation.json'))?load(join(dir,'observation.json')):{markers:[]};
 if(error || value.appPid!==app.pid || value.title!=='WR-relaunch' || !existsSync(join(dir,'screen.png'))) value.helperError=error || 'unbound observation';
 return value;
}
try {
 assert.equal(app.version,'0.3.5');
 const pin=load(join(out,'identity.json'));
 const status=JSON.parse(cli(['status','--json']));
 identity=JSON.parse(cli(['identify']));save('relaunch-daemon-before.json',identity);
 assert.equal(status.daemon.version,'0.3.5');
 assert.ok(pin.source.startsWith(status.daemon.build_id),'installed daemon source pin mismatch');
 const dir=join(out,'relaunch-stub');mkdirSync(dir);
 const command=`powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -Command "& ${quote(resolve('scripts/proofs/scroll-stub.ps1'))} -Directory ${quote(dir)}"`;
 ref=cli(['new-surface','--cmd',command,'--cwd',process.env.RUNNER_TEMP,'--title','WR-relaunch','--role','scroll-relaunch']);
 assert.match(ref,/^surface:\d+$/);
 const layout=await observe('layout');assert.ok(layout.readiness && layout.bounds,'proof pane unavailable');
 const live=JSON.parse(cli(['status','--json']));
 const others=live.surfaces.filter(s=>!s.exited && s.surface_ref!==ref);
 assert.ok(others.every(s=>!s.agent && !s.role),'unexpected runner agent; do not close it');
 for(const seat of others)cli(['close-surface',seat.surface_ref]);
 save('relaunch-startup-shells-closed.json',others);
 const seat=live.surfaces.find(s=>s.surface_ref===ref);save('relaunch-seat-before.json',seat);
 // Output starts only after the rendered pane attaches; the stub then stays idle.
 writeFileSync(join(dir,'next.txt'),'relaunch|plain|idle');renameSync(join(dir,'next.txt'),join(dir,'control-0001.txt'));
 const deadline=Date.now()+10000;
 while(!existsSync(join(dir,'ready.txt'))) {assert.ok(Date.now()<deadline,'stub output not ready');await wait(100);}
 await wait(700);
 baseline=await observe('baseline');
 const geometry=load(join(dir,'geometry.json'));save('relaunch-geometry.json',geometry);
 assert.ok(Number.isInteger(geometry.rows) && geometry.rows>0,'viewport height unavailable');
 assert.ok(baseline.markers?.length>=3 && baseline.markers[0]>1001,'baseline not observing retained history');
 const wheel=await observe('control-wheel','wheel');save('relaunch-control-wheel.json',wheel);
 assert.ok(wheel.input && classifyViewport(baseline,wheel)==='MOVED_UP','wheel control failed: do not relaunch');
 let depth;
 for(let page=1;page<=4;page++) {
  depth=await observe(`control-depth-${page}`,'ShiftPageUp');
  assert.ok(depth.input && classifyViewport(baseline,depth)==='MOVED_UP','retained history depth input/observer unavailable');
 }
 const retainedRows=baseline.markers[0]-depth.markers[0];
 save('relaunch-retained-depth.json',{baseline,depth,retainedRows,viewportRows:geometry.rows});
 assert.ok(retainedRows>3*geometry.rows,'more than three GUI screens not observed: do not relaunch');
 let reset;
 for(let page=1;page<=5;page++) reset=await observe(`control-reset-${page}`,'ShiftPageDown');
 assert.ok(reset.input && classifyViewport(baseline,reset)==='STATIONARY','bottom reset failed: do not relaunch');
 control=true;
 const quit=JSON.parse(execFileSync(ps,['-NoProfile','-NonInteractive','-Command',
  `$ErrorActionPreference='Stop';$p=Get-Process -Id ${app.pid};$actual=(Get-CimInstance Win32_Process -Filter 'ProcessId=${app.pid}').ExecutablePath;if($actual -ne ${quote(app.path)}){throw 'GUI process identity mismatch'};if(-not $p.CloseMainWindow()){throw 'GUI close refused'};if(-not $p.WaitForExit(15000)){throw 'GUI did not exit; no forced kill'};@{pid=$p.Id;path=$actual;exited=$true}|ConvertTo-Json -Compress`],{encoding:'utf8',timeout:20000,windowsHide:true}));
 save('relaunch-gui-quit.json',quit);
 const survived=JSON.parse(cli(['identify']));save('relaunch-daemon-gui-closed.json',survived);
 assert.equal(survived.daemon_pid,identity.daemon_pid);assert.equal(survived.started_at,identity.started_at);
 const relaunched=JSON.parse(execFileSync(ps,['-NoProfile','-NonInteractive','-Command',
  `$ErrorActionPreference='Stop';$p=Start-Process ${quote(app.path)} -PassThru -RedirectStandardOutput ${quote(join(out,'relaunch.stdout.log'))} -RedirectStandardError ${quote(join(out,'relaunch.stderr.log'))};@{pid=$p.Id;path=${quote(app.path)}}|ConvertTo-Json -Compress`],{encoding:'utf8',timeout:15000,windowsHide:true}));
 assert.notEqual(relaunched.pid,app.pid);app.pid=relaunched.pid;save('relaunch-gui-start.json',relaunched);
 await wait(1500);
 const afterIdentity=JSON.parse(cli(['identify']));save('relaunch-daemon-after.json',afterIdentity);
 const afterSeat=JSON.parse(cli(['status','--json'])).surfaces.find(s=>s.surface_ref===ref);save('relaunch-seat-after.json',afterSeat || null);
 const sameSeat=!!afterSeat && !afterSeat.exited && afterSeat.pid===seat.pid && afterSeat.cmd===seat.cmd;
 assert.ok(sameSeat,'same seat did not survive');
 assert.equal(afterIdentity.daemon_pid,identity.daemon_pid);assert.equal(afterIdentity.started_at,identity.started_at);
 const restored=await observe('restored');save('relaunch-restored.json',restored);
 assert.equal(classifyViewport(baseline,restored),'STATIONARY','restored baseline differs: cannot attribute lost history');
 for(const action of ['wheel','ShiftPageUp']) {const observation=await observe(action,action);attempts.push(observation);save(`relaunch-${action}.json`,observation);}
 // Shipped CSS hides the scrollbar; do not substitute a fabricated native input.
 save('relaunch-scrollbar.json',{action:'scrollbar',status:'ABSENT BY DESIGN',source:'bcfa8bc6396035885fc66ed79e5507afedb2a2bd ui/src/style.css:516-518',input:null,reason:'shipped ui/src/style.css:517-518 hides viewport scrollbar; no native scrollbar attempt proven'});
 const verdict=classifyRelaunch({control,beforeIdentity:identity,afterIdentity,sameSeat,baseline,attempts});
 save('relaunch-result.json',{os:'Windows',verdict,criteria:'criteria_relaunch_row_v2.md (requires Theo activation)',control,sameSeat,attempts,scrollbar:'ABSENT BY DESIGN (source-verified; not a tested input)',ownerCase:'NOT PROVEN',touchpad:'NOT PROVEN'});
 assert.notEqual(verdict,'NOT EXECUTED','row incomplete; retain evidence, never label RED');
} catch(error) {
 save('relaunch-incomplete.json',{os:'Windows',verdict:'NOT EXECUTED',error:String(error),control,ref,attempts});throw error;
} finally {
 // Only the proof-created seat is closed; no daemon stop or process-wide kill.
 if(ref)cli(['close-surface',ref]);
}
