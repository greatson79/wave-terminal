// SKILLS_USED: ponytail(lite), hallucination-guard, debugging-and-error-recovery, test-driven-development.
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const net = require('node:net');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const { Terminal } = require('@xterm/headless');
const { judge, shape } = require('./judge.cjs');
const { compareWindow } = require('./attach-compare.cjs');
const FIXTURE_SHA = 'a6770aba1c94852f31f354c183a183a3c681fcd60f312ad26cc15f771949794d';
const STRESS_REPEATS=256;
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
function splitAck(bytes) {
  const end = bytes.indexOf(10);
  if (end < 0) return null;
  const ack = JSON.parse(bytes.subarray(0,end).toString('utf8'));
  if (ack.ok !== true) throw new Error(`attach rejected: ${JSON.stringify(ack)}`);
  return bytes.subarray(end+1);
}
function expectedClose(intentional, proof,expectedHash=FIXTURE_SHA) {
  return intentional || proof?.complete===true && proof.sha256===expectedHash;
}
function deliveryMetrics(completed,chunks) {
  const childBytes=completed?.bytes??null;
  const clientBytes=chunks.reduce((total,chunk)=>total+chunk.length,0);
  return {childBytes,childChunks:completed?.chunks??null,clientBytes,clientChunks:chunks.length,
    childMinusClientBytes:childBytes===null?null:childBytes-clientBytes,
    caveat:'Diagnostic delta only: ConPTY transforms bytes, attach snapshots add bytes, and the deliberate disconnect omits live bytes; delta does not prove broadcast loss.'};
}
function measure(term) {
  function count(buffer) {
    const lines = [];
    for (let i=0; i<buffer.length; i++) {
      const line = buffer.getLine(i);
      const text = line.translateToString(!buffer.getLine(i+1)?.isWrapped);
      if (line.isWrapped && lines.length) lines[lines.length-1] += text;
      else lines.push(text);
    }
    const labels = [...new Set(lines.flatMap(line => {
      const match=line.match(/(?:^|\s)SCROLL-(\d{3})\s*$/);
      return match?[Number(match[1])]:[];
    }))].filter(n=>n>=1&&n<=100).sort((a,b)=>a-b);
    return { labelCount:labels.length, labels, missing:Array.from({length:100},(_,i)=>i+1).filter(n=>!labels.includes(n)), baseY:buffer.baseY, length:buffer.length };
  }
  return {cols:term.cols,rows:term.rows,activeType:term.buffer.active.type,normal:count(term.buffer.normal),active:count(term.buffer.active)};
}
async function waitFor(test, description, timeout=30000) {
  const deadline = Date.now()+timeout;
  while (Date.now()<deadline) { const value = await test(); if (value) return value; await sleep(25); }
  throw new Error(`timeout: ${description}`);
}
function connect(socket) {
  return new Promise((resolve,reject) => {
    const client = net.createConnection(socket);
    client.once('error',reject);
    client.once('connect',()=>{ client.removeListener('error',reject); resolve(client); });
  });
}
async function rpc(socket, method, params={}) {
  const client = await connect(socket);
  try {
    return await new Promise((resolve,reject) => {
      let data = Buffer.alloc(0);
      client.setTimeout(5000,()=>reject(new Error(`RPC timeout: ${method}`)));
      client.on('error',reject);
      client.on('end',()=>reject(new Error(`RPC EOF: ${method}`)));
      client.on('data',chunk=>{
        data=Buffer.concat([data,chunk]);
        const end=data.indexOf(10);
        if(end>=0) {
          try { const reply=JSON.parse(data.subarray(0,end).toString('utf8')); if(reply.ok!==true) throw new Error(JSON.stringify(reply)); resolve(reply.result); }
          catch(error) { reject(error); }
        }
      });
      client.write(JSON.stringify({id:1,method,params})+'\n');
    });
  } finally { client.destroy(); }
}
async function childReplay(fixture, directory) {
  const bytes=fs.readFileSync(fixture);
  await waitFor(()=>fs.existsSync(path.join(directory,'start')), 'start child');
  let chunks=0;
  let payloadBytes=0;
  const hash=crypto.createHash('sha256');
  const repeats=Number(process.env.SCROLL_STRESS_REPEATS||0);
  if(!Number.isInteger(repeats)||repeats<0||repeats>STRESS_REPEATS)throw new Error('invalid child stress repetitions');
  const stress=repeats>0;
  const begin=Buffer.from(`\r\n${process.env.SCROLL_BEGIN}\r\n`),end=Buffer.from(`\r\n${process.env.SCROLL_END}\r\n`);
  if(stress) {
    fs.writeSync(1,begin);
    await waitFor(()=>fs.existsSync(path.join(directory,'stress')), 'stress gate',120000);
    for(let repetition=0;repetition<repeats;repetition++) {
      for(let i=0;i<bytes.length;i+=512) {
        const chunk=bytes.subarray(i,i+512);
        fs.writeSync(1,chunk);hash.update(chunk);payloadBytes+=chunk.length;chunks++;
      }
      fs.writeFileSync(path.join(directory,'progress.json'),JSON.stringify({bytes:payloadBytes,chunks,phase:'stress',repetition:repetition+1,complete:false}));
    }
    fs.writeSync(1,end);
    fs.writeFileSync(path.join(directory,'progress.json'),JSON.stringify({bytes:payloadBytes,chunks,phase:'await-tail',complete:false}));
    await waitFor(()=>fs.existsSync(path.join(directory,'tail')), 'tail gate',120000);
  }
  for(let i=0;i<bytes.length;i+=512) {
    const chunk=bytes.subarray(i,i+512);
    fs.writeSync(1,chunk);hash.update(chunk);payloadBytes+=chunk.length;chunks++;
    fs.writeFileSync(path.join(directory,'progress.json'),JSON.stringify({bytes:payloadBytes,chunks,phase:'tail',tailBytes:Math.min(i+512,bytes.length),complete:false}));
    await sleep(40);
  }
  fs.writeFileSync(path.join(directory,'complete.json'),JSON.stringify({bytes:payloadBytes,chunks,sha256:hash.digest('hex'),fixture_sha256:crypto.createHash('sha256').update(bytes).digest('hex'),marker_bytes:stress?begin.length+end.length:0,complete:true}));
  await sleep(2000);
}
async function run(executable, evidence, variant='FAST') {
  if (!executable || !evidence) throw new Error('Usage: node attach-repro.cjs ABSOLUTE_WAVED NEW_EVIDENCE_DIR FAST|SLOW');
  if(!['FAST','SLOW'].includes(variant))throw new Error('variant must be FAST or SLOW');
  if(process.platform!=='win32'||process.env.GITHUB_ACTIONS!=='true'||process.env.RUNNER_ENVIRONMENT!=='github-hosted')throw new Error('CI-only: real daemon execution requires a disposable GitHub-hosted Windows runner; never run on an owner PC');
  if(path.basename(executable).toLowerCase()!=='waved'+(process.platform==='win32'?'.exe':'')) throw new Error('requires the real waved build, never cysd or live app');
  executable=path.resolve(executable);
  evidence=path.resolve(evidence);
  fs.mkdirSync(evidence); // Refuse overwrite; retain every run.
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'wave-attach-'));
  const fixture=path.join(__dirname,'fixtures','claude_scroll100.bin');
  const bytes=fs.readFileSync(fixture);
  if(crypto.createHash('sha256').update(bytes).digest('hex')!==FIXTURE_SHA) throw new Error('fixture SHA mismatch');
  const payloadHasher=crypto.createHash('sha256');
  for(let repetition=0;repetition<=STRESS_REPEATS;repetition++)payloadHasher.update(bytes);
  const expectedPayloadSha=payloadHasher.digest('hex');
  const nonce=crypto.randomBytes(4).toString('hex');
  const begin=Buffer.from(`WAVE_STRESS_BEGIN_${nonce}`),end=Buffer.from(`WAVE_STRESS_END_${nonce}`);
  const rawBaseline=await judge(bytes);
  if(rawBaseline.active.labelCount!==100) throw new Error('raw fixture baseline is not 100/100');
  fs.writeFileSync(path.join(evidence,'raw-baseline.json'),JSON.stringify(rawBaseline,null,2));
  const socket=process.platform==='win32'?`\\\\.\\pipe\\wave-scroll-attach-${process.pid}-${Date.now()}`:path.join(root,'seat.sock');
  const home=path.join(root,'home');
  fs.mkdirSync(home);
  // Windows known-folder home ignores these overrides; disposable hosted CI is the boundary.
  const env={...process.env,CYS_SOCKET:socket,CYS_PACK_DIR:path.join(root,'pack'),CYS_ACCOUNT_DIR:path.join(root,'account'),CLAUDE_CONFIG_DIR:path.join(root,'account'),CYS_NO_AUTORESTORE:'1',CYS_AUTORESTORE:'0',CYS_NO_AUTOSTART:'1',CYS_NO_OFFICE_BRIDGE:'1',CYS_AUTOKILL_DUP:'0',HOME:home,USERPROFILE:home,LOCALAPPDATA:path.join(root,'local'),APPDATA:path.join(root,'roaming'),XDG_STATE_HOME:path.join(root,'state'),CYS_SHELL:process.platform==='win32'?path.join(process.env.WINDIR,'System32','WindowsPowerShell','v1.0','powershell.exe'):'/bin/sh'};
  for(const key of ['CYS_SURFACE_ID','CYS_SURFACE_REF','CYS_ROLE','JAVIS_SURFACE_ID','AITERM_SURFACE_ID','CLAUDECODE','CLAUDE_CODE_SESSION_ID','CLAUDE_CODE_CHILD_SESSION']) delete env[key];
  const log=fs.openSync(path.join(evidence,'waved.log'),'wx');
  const daemon=spawn(executable,[],{cwd:root,env,stdio:['ignore',log,log]});
  const exited=new Promise(resolve=>daemon.once('exit',(code,signal)=>resolve({code,signal})));
  let spawnError;
  daemon.once('error',error=>{spawnError=error;});
  const term=new Terminal({cols:120,rows:35,scrollback:5000,allowProposedApi:true});
  const events=[];
  const chunks=[];
  const referenceChunks=[];
  const primaryTokens={tail:Buffer.alloc(0),begin:false,end:false},referenceTokens={tail:Buffer.alloc(0),begin:false,end:false};
  let writes=Promise.resolve(), stream,referenceStream, sid, streamError, attachment=0, completed, completedMeasurement=false,comparison;
  const started=Date.now();
  const record=(kind,detail={})=>events.push({atMs:Date.now()-started,kind,...detail});
  const progress=()=>{try{return JSON.parse(fs.readFileSync(path.join(root,'progress.json'),'utf8').replace(/^\uFEFF/,''));}catch{return null;}};
  const checkpoint=async name=>{await writes;record('checkpoint',{name,delivery:deliveryMetrics(progress(),chunks),...measure(term)});};
  const observe=(tokens,chunk)=>{
    const probe=Buffer.concat([tokens.tail,chunk]);
    tokens.begin||=probe.includes(begin);tokens.end||=probe.includes(end);
    tokens.tail=probe.subarray(Math.max(0,probe.length-Math.max(begin.length,end.length)+1));
  };
  const write=(chunk,reference=false)=>{
    if(!chunk.length)return;
    if(reference){referenceChunks.push(chunk);observe(referenceTokens,chunk);record('reference-write',{bytes:chunk.length});return;}
    chunks.push(chunk);
    observe(primaryTokens,chunk);
    record('write',{attachment,bytes:chunk.length,b64:chunk.toString('base64')});
    writes=writes.then(()=>new Promise(resolve=>term.write(chunk,resolve)));
  };
  async function attach(reference=false) {
    const client=await connect(socket);
    if(!reference)attachment++;
    const attachId=reference?'reference':attachment;
    let ack=Buffer.alloc(0), acknowledged=false;
    const ready=new Promise((resolve,reject)=>{
      client.on('error',error=>{
        record('stream-error',{attachment:attachId,intentional:client.probeIntentionalClose===true,error:String(error)});
        if(!client.probeIntentionalClose){streamError=error;reject(error);}
      });
      const ended=kind=>{
        let proof;
        try{proof=JSON.parse(fs.readFileSync(path.join(root,'complete.json'),'utf8').replace(/^\uFEFF/,''));}catch{}
        const expected=expectedClose(client.probeIntentionalClose===true,proof,expectedPayloadSha);
        record(kind,{attachment:attachId,intentional:client.probeIntentionalClose===true,childComplete:proof?.complete===true,expected});
        if(!acknowledged||!expected){streamError=new Error(`unexpected attach ${kind} (${attachId})`);reject(streamError);}
      };
      client.on('end',()=>ended('stream-end'));
      client.on('close',()=>ended('stream-close'));
      client.on('data',chunk=>{
        if(acknowledged){write(chunk,reference);return;}
        ack=Buffer.concat([ack,chunk]);
        try {
          const remainder=splitAck(ack);
          if(remainder!==null){acknowledged=true;record('attach-ack',{attachment:attachId});write(remainder,reference);resolve();}
        } catch(error){streamError=error;reject(error);}
      });
    });
    client.write(JSON.stringify({id:1,method:'surface.attach',params:{surface_id:sid}})+'\n');
    await Promise.race([ready,sleep(5000).then(()=>{throw new Error('attach ACK timeout');})]);
    return client;
  }
  async function resize(cols,name) {
    await checkpoint(`before-${name}`);
    term.resize(cols,35); // Same app order: xterm fit/reflow first, daemon RPC second.
    record('xterm-resize',{cols,rows:35});
    await rpc(socket,'surface.resize',{surface_id:sid,cols,rows:35});
    record('daemon-resize-ack',{cols,rows:35});
    await checkpoint(`after-${name}`);
  }
  try {
    await waitFor(async()=>{if(spawnError)throw spawnError;if(daemon.exitCode!==null)throw new Error(`waved exited ${daemon.exitCode}`);try{return await rpc(socket,'system.identify',{caller:'ui'});}catch{return false;}},'isolated waved ready',60000);
    const quote=s=>"'"+s.replaceAll("'",process.platform==='win32'?"''":"'\\''")+"'";
    const command=process.platform==='win32'?`Set-ExecutionPolicy -Scope Process Bypass -Force; & ${quote(path.join(__dirname,'attach-child.ps1'))}`:`stty -onlcr -echo; exec ${quote(process.execPath)} ${quote(__filename)} --child ${quote(fixture)} ${quote(root)}`;
    const created=await rpc(socket,'surface.create',{cwd:root,cmd:command,title:'synthetic scroll probe',rows:35,cols:120,env:{SCROLL_REPLAY:fixture,SCROLL_ATTACH_DIR:root,SCROLL_STRESS_REPEATS:String(STRESS_REPEATS),SCROLL_BEGIN:begin.toString(),SCROLL_END:end.toString()}});
    sid=created.surface_id;
    if(!Number.isInteger(sid))throw new Error('surface.create did not return surface_id');
    record('created',{variant,surfaceId:sid,daemonPid:daemon.pid,root,socket,fixtureSha256:FIXTURE_SHA});
    stream=await attach();
    referenceStream=await attach(true);
    await resize(70,'initial-narrow-fit');
    fs.writeFileSync(path.join(root,'start'),'go',{flag:'wx'});
    await waitFor(()=>{if(streamError)throw streamError;return primaryTokens.begin&&referenceTokens.begin;},'both BEGIN markers');
    await checkpoint('before-stress');
    const pauseStart=Date.now(),referenceStart=referenceChunks.length,referenceBytesStart=deliveryMetrics(null,referenceChunks).clientBytes;
    if(variant==='SLOW'){stream.pause();record('reader-pause',{progress:progress(),delivery:deliveryMetrics(progress(),chunks)});}
    fs.writeFileSync(path.join(root,'stress'),'go',{flag:'wx'});
    await waitFor(()=>{if(streamError)throw streamError;const p=progress();return p&&p.bytes>0;},'stress emission',120000);
    if(variant==='SLOW') {
      await sleep(Math.max(0,5000-(Date.now()-pauseStart)));
      record('reader-resume',{progress:progress(),delivery:deliveryMetrics(progress(),chunks),pauseMs:Date.now()-pauseStart,referenceSocketChunksDuringPause:referenceChunks.length-referenceStart,referenceSocketBytesDuringPause:deliveryMetrics(null,referenceChunks).clientBytes-referenceBytesStart,overflowProven:false});
      stream.resume();
    }
    await waitFor(()=>{if(streamError)throw streamError;return primaryTokens.end&&referenceTokens.end;},'both END markers after slow resume',120000);
    comparison=compareWindow(Buffer.concat(referenceChunks),Buffer.concat(chunks),begin,end);
    record('stress-window-comparison',{...comparison,overflowProven:false});
    await checkpoint('after-stress-before-reattach');
    if(!comparison.comparable)throw new Error('INDETERMINATE: stress markers not comparable');
    fs.writeFileSync(path.join(root,'tail'),'go',{flag:'wx'});
    await waitFor(()=>{if(streamError)throw streamError;const p=progress();return p&&p.phase==='tail'&&p.tailBytes>=bytes.length/3&&p.tailBytes<bytes.length;},'mid-tail resize point');
    record('mid-tail-resize-progress',{progress:progress()});
    await resize(60,'mid-stream-resize');
    await waitFor(()=>{if(streamError)throw streamError;const p=progress();return p&&p.phase==='tail'&&p.tailBytes>=bytes.length*0.55&&p.tailBytes<bytes.length;},'mid-tail reattach point');
    await checkpoint('before-reattach');
    record('disconnect',{progress:progress()});
    stream.probeIntentionalClose=true;
    stream.destroy();
    await sleep(500); // Same backend REATTACH_DELAY_MS; output intentionally continues during gap.
    stream=await attach();
    await waitFor(()=>events.some(event=>event.kind==='write'&&event.attachment===attachment),'reattach snapshot bytes',5000);
    await checkpoint('after-reattach-snapshot');
    completed=await waitFor(()=>{if(streamError)throw streamError;try{return JSON.parse(fs.readFileSync(path.join(root,'complete.json'),'utf8').replace(/^\uFEFF/,''));}catch{return false;}},'complete fixture');
    if(completed.complete!==true||completed.bytes!==bytes.length*(STRESS_REPEATS+1)||completed.chunks!==Math.ceil(bytes.length/512)*(STRESS_REPEATS+1)||completed.sha256!==expectedPayloadSha||completed.fixture_sha256!==FIXTURE_SHA)throw new Error('INDETERMINATE: child fixture incomplete/mismatched');
    if(process.platform==='win32'&&(!completed.ps.startsWith('5.1.')||completed.window_cols!==60||completed.window_rows!==35))throw new Error('INDETERMINATE: wrong PowerShell/console geometry');
    await sleep(1500);
    await checkpoint('final');
    if(streamError)throw streamError;
    completedMeasurement=true;
    console.log(JSON.stringify({result:'completed-measurement',variant,rawLabels:rawBaseline.active.labelCount,labels:measure(term).normal.labelCount,delivery:deliveryMetrics(completed,chunks),comparison,attachments:attachment,evidence}));
  } catch(error) {
    if(!comparison){comparison=compareWindow(Buffer.concat(referenceChunks),Buffer.concat(chunks),begin,end);record('stress-window-comparison',{...comparison,overflowProven:false});}
    record('failure',{error:String(error)});
    throw error;
  } finally {
    if(stream){stream.probeIntentionalClose=true;stream.destroy();}
    if(referenceStream){referenceStream.probeIntentionalClose=true;referenceStream.destroy();}
    await writes;
    if(sid!==undefined)try{await rpc(socket,'surface.close',{surface_id:sid});}catch(error){record('cleanup-error',{error:String(error)});}
    if(daemon.exitCode===null)daemon.kill();
    const status=await Promise.race([exited,sleep(5000).then(()=>null)]);
    const output=Buffer.concat(chunks);
    fs.writeFileSync(path.join(evidence,'received.bin'),output,{flag:'wx'});
    fs.writeFileSync(path.join(evidence,'reference.bin'),Buffer.concat(referenceChunks),{flag:'wx'});
    if(completed)fs.writeFileSync(path.join(evidence,'child-complete.json'),JSON.stringify(completed,null,2),{flag:'wx'});
    fs.writeFileSync(path.join(evidence,'result.json'),JSON.stringify({completedMeasurement,variant,platform:process.platform,fixtureSha256:FIXTURE_SHA,expectedPayloadSha,stressRepeats:STRESS_REPEATS,markers:{begin:begin.toString(),end:end.toString()},rawLabels:rawBaseline.active.labelCount,attachments:attachment,delivery:deliveryMetrics(completed,chunks),referenceDelivery:deliveryMetrics(completed,referenceChunks),comparison,overflowProven:false,shape:shape(output),final:measure(term),events},null,2),{flag:'wx'});
    fs.writeFileSync(path.join(evidence,'lifecycle.json'),JSON.stringify({ownedPid:daemon.pid,status,isolatedRoot:root},null,2),{flag:'wx'});
    fs.closeSync(log);
    term.dispose();
    if(!status)throw new Error('owned waved did not exit within 5s');
  }
}
module.exports={splitAck,measure,expectedClose,deliveryMetrics};
if(require.main===module) {
  const task=process.argv[2]==='--child'?childReplay(process.argv[3],process.argv[4]):run(process.argv[2],process.argv[3],process.argv[4]);
  task.catch(error=>{console.error(error);process.exitCode=1;});
}
