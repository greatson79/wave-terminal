import test from 'node:test';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {launchGui} from './scroll-relaunch-launch.mjs';

const childCode='setTimeout(()=>{},4000)';
const receipt=String.raw`process.stdout.write(JSON.stringify({pid:child.pid}));`;
const run=code=>execFileSync(process.execPath,['--input-type=module','-e',code],{encoding:'utf8',timeout:2000});

test('inherited captured handles keep a completed launcher waiting',()=>{
 const code=`import {spawn} from 'node:child_process';
 const child=spawn(process.execPath,['-e',${JSON.stringify(childCode)}],{stdio:'inherit'});
 child.unref();${receipt}`;
 assert.throws(()=>run(code),error=>{
  assert.equal(error.code,'ETIMEDOUT');
  assert.ok(JSON.parse(error.stdout).pid>0,'parent already emitted child PID');
  return true;
 });
});

test('detached GUI launch returns its PID without waiting for long-lived child',()=>{
 const code=`import {launchGui} from ${JSON.stringify(new URL('./scroll-relaunch-launch.mjs',import.meta.url).href)};
 const child=await launchGui(process.execPath,['-e',${JSON.stringify(childCode)}]);${receipt}`;
 const {pid}=JSON.parse(run(code));
 assert.ok(Number.isInteger(pid) && pid>0);
 assert.doesNotThrow(()=>process.kill(pid,0),'child remains alive after launcher returned');
});

test('launch failure rejects instead of returning a false PID',async()=>{
 await assert.rejects(launchGui(new URL('./missing-gui-executable',import.meta.url).pathname),{code:'ENOENT'});
});
