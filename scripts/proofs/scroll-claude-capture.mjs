import assert from 'node:assert/strict';
import {execFileSync,spawn} from 'node:child_process';
import {mkdirSync,writeFileSync,readFileSync,existsSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {createHash,randomUUID} from 'node:crypto';
import {claudeModes,claudeStartupFrame} from './scroll-claude-modes.mjs';
assert.equal(process.platform,'win32','Windows ConPTY runner only');
const out=resolve('scroll-evidence','claude-conpty');mkdirSync(out);
const save=(name,value)=>writeFileSync(join(out,name),JSON.stringify(value,null,2)+'\n');
const hash=b=>createHash('sha256').update(b).digest('hex');
const wave=join(process.env.LOCALAPPDATA,'Wave Terminal','wave.exe');
const claude=join(process.env.RUNNER_TEMP,'claude-f3','node_modules','@anthropic-ai','claude-code','bin','claude.exe');
const cli=args=>execFileSync(wave,args,{encoding:'utf8',timeout:15000,windowsHide:true}).trim();
const version=execFileSync(claude,['--version'],{encoding:'utf8',timeout:15000,windowsHide:true}).trim();
assert.match(version,/2\.1\.323/);
const token=randomUUID();writeFileSync(join(out,'token.txt'),token);
const quote=s=>`'${s.replaceAll("'","''")}'`;
const command=`powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -Command "& ${quote(resolve('scripts/proofs/scroll-claude-child.ps1'))} -Directory ${quote(out)} -Claude ${quote(claude)}"`;
const ref=cli(['new-surface','--cmd',command,'--cwd',process.env.RUNNER_TEMP,'--title','F3 Claude startup','--role','scroll-claude-capture']);
assert.match(ref,/^surface:\d+$/);
let attach;const chunks=[];let stderr='';let attachError;let cleanupError;
const wait=ms=>new Promise(resolve=>setTimeout(resolve,ms));
try {
 attach=spawn(wave,['attach',ref],{stdio:['ignore','pipe','pipe'],windowsHide:true});
 attach.on('error',error=>{attachError=error});
 attach.stdout.on('data',b=>chunks.push(Buffer.from(b)));attach.stderr.on('data',b=>{stderr+=b.toString()});
 const deadline=Date.now()+10000;
 while(!stderr.includes('[attached surface:')){assert.ok(Date.now()<deadline && !attachError && attach.exitCode===null,'raw attach unavailable');await wait(50)}
 writeFileSync(join(out,'start.flag'),'start');
 await wait(12000);
} finally {
 try {cli(['close-surface',ref]);await wait(500)} catch(error){cleanupError=String(error)}
 try {if(attach?.exitCode===null){attach.kill();await Promise.race([new Promise(resolve=>attach.once('close',resolve)),wait(1000)]);}} catch(error){cleanupError=String(error)}
 finally {const all=Buffer.concat(chunks);writeFileSync(join(out,'attach.raw'),all);writeFileSync(join(out,'attach.stderr.txt'),stderr);save('cleanup.json',{error:cleanupError??null});}
}
const all=Buffer.concat(chunks);const begin=Buffer.from(`F3_CHILD_START_${token}`);const offset=all.indexOf(begin);
assert.ok(offset>=0,'child-start marker absent; not Claude output proof');
let raw=all.subarray(offset+begin.length);const end=raw.indexOf(Buffer.from(`F3_CHILD_END_${token}`));if(end>=0)raw=raw.subarray(0,end);
writeFileSync(join(out,'claude-output.raw'),raw);
const childExit=existsSync(join(out,'child-exit.json'))?JSON.parse(readFileSync(join(out,'child-exit.json'),'utf8').replace(/^\uFEFF/,'')):null;
const startup=claudeStartupFrame(raw);
const outcome=childExit?.exitCode ? 'STARTUP_ERROR' : startup?'STARTUP_CAPTURED':'NOT_PROVEN';
const analysis=claudeModes(raw);
save('result.json',{outcome,cleanupError:cleanupError??null,childExit,startupBannerVerified:startup,workingSession:'NOT_EXECUTED; no owner account/login',os:'Windows ConPTY (installed Wave portable-pty)',ref,version,executable:claude,executableSha256:hash(readFileSync(claude)),rawPath:join(out,'claude-output.raw'),sha256:hash(raw),rawBytes:raw.length,...analysis,framing:'bytes after unique child marker; initial attach snapshot excluded',childEnvironment:existsSync(join(out,'child-environment.json'))?'child-environment.json':'UNAVAILABLE'});

assert.equal(outcome,'STARTUP_CAPTURED','versioned Claude startup frame not proven; inspect raw/error evidence');
assert.equal(cleanupError,undefined,'capture cleanup failed; raw evidence retained');
