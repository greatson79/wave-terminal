import test from 'node:test';
import assert from 'node:assert/strict';
import {classifyOwnerStream,hasActiveOwnerProducer} from './scroll-owner-contract.mjs';
const frame=(first,elapsedMs)=>({source:'terminal-visible-markers',markers:[first,first+1,first+2],elapsedMs});
const sample=(ids=[1240,1240,1240,1240])=>({preInputTime:'2026-11-13T00:00:00.000Z',inputEndedAt:'2026-11-13T00:00:00.040Z',source:'terminal-visible-markers',preInputMarkers:[1258,1259,1260],input:{kind:'native-wheel'},frames:ids.map((id,i)=>frame(id,[50,150,700,2800][i]))});
test('gated stream requires movement and retains selected history for2s',()=>{
 const r=classifyOwnerStream(sample());assert.equal(r.verdict,'NOT RED');assert.equal(r.retentionMs,2750);
});
test('distinguishes dead wheel from moved-then-jumped-back',()=>{
 assert.match(classifyOwnerStream(sample([1258,1260,1280,1300])).sees,/wheel dead/);
 assert.match(classifyOwnerStream({...sample([1258,1260,1280,1300]),input:{kind:'native-key'}}).sees,/keyboard scroll did not respond/);
 const r=classifyOwnerStream(sample([1240,1240,1258,1300]));assert.equal(r.verdict,'RED');assert.match(r.sees,/jumps back/);
});
test('missing input, malformed frames, bad timing and late movement cannot be RED',()=>{
 for(const bad of [{...sample(),input:null},{...sample(),error:'focus failed'},{...sample(),frames:[]},{...sample(),frames:[frame(1240,50),frame(1240,50),frame(1240,700),frame(1240,2800)]},{...sample(),frames:[frame(1240,50),frame(1240,150),frame(1240,700),frame(1240,1000)]},sample([1258,1260,1280,1240])])assert.equal(classifyOwnerStream(bad).verdict,'NOT EXECUTED');
 assert.equal(classifyOwnerStream({...sample(),frames:[frame(1240,50),{...frame(1240,150),markers:[1,1]},frame(1240,700),frame(1240,2800)]}).verdict,'NOT EXECUTED');
});
test('absence of older history is not a product RED',()=>{assert.equal(classifyOwnerStream({...sample(),captureError:'failed'},false).verdict,'NOT EXECUTED');assert.deepEqual(classifyOwnerStream(sample(),false),{verdict:'NOT RED',sees:'nothing to scroll'});});

test('dead-wheel verdict rejects early/late schedule and stale baseline',()=>{
 for(const times of [[1,2,3,4],[500,600,700,2800],[50,150,700,5000]]) {
  const s=sample([1258,1258,1258,1258]);s.frames.forEach((f,i)=>f.elapsedMs=times[i]);assert.equal(classifyOwnerStream(s).verdict,'NOT EXECUTED');
 }
 assert.equal(classifyOwnerStream({...sample([1258,1258,1258,1258]),inputEndedAt:'2026-11-13T00:00:00.200Z'}).verdict,'NOT EXECUTED');
});
test('producer must actively advance ordered bursts through observation',()=>{
 const start=Date.parse('2026-11-13T00:00:00.000Z');
 const counters=Array.from({length:22},(_,i)=>({at:start-100+i*150,burst:i+1,line:2000+i*5}));
 const observation={preInputTime:new Date(start).toISOString(),frames:[{time:new Date(start+90).toISOString()},{time:new Date(start+2840).toISOString()}]};
 assert.equal(hasActiveOwnerProducer(counters,observation,18),true);
 for(const bad of [[counters[0],counters.at(-1)],counters.map((r,i)=>i===5?{...r,line:r.line+1}:r),counters.map((r,i)=>i===5?{...r,burst:1}:r),counters.filter((_,i)=>i<5 || i>10),[...counters,{at:NaN,burst:0,line:0}]])assert.equal(hasActiveOwnerProducer(bad,observation,18),false);
 assert.equal(hasActiveOwnerProducer(counters,{...observation,frames:[{time:new Date(start+600).toISOString()},observation.frames[1]]},10),false);
});
