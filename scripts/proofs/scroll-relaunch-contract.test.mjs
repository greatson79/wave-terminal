import test from 'node:test';
import assert from 'node:assert/strict';
import {classifyRelaunch} from './scroll-relaunch-contract.mjs';
const view = n => ({source:'terminal-visible-markers', markers:[n,n+1,n+2]});
const identity = {daemon_pid:123,started_at:1234};
const sample = {control:true,sameSeat:true,beforeIdentity:identity,afterIdentity:identity,
 baseline:view(1200),attempts:['wheel','ShiftPageUp'].map(action=>({...view(1200),action,input:{}}))};
test('relaunch requires survival and positive control; missing input cannot prove RED',()=>{
 assert.equal(classifyRelaunch(sample),'RED');
 for (const delta of [{control:false},{sameSeat:false},{beforeIdentity:{}},
  {afterIdentity:{...identity,daemon_pid:124}},{afterIdentity:{...identity,started_at:1235}},
  {attempts:sample.attempts.slice(0,1)}, {attempts:sample.attempts.map(a=>({...a,input:null}))}])
  assert.equal(classifyRelaunch({...sample,...delta}),'NOT EXECUTED');
});
test('older validated markers prove retained history; malformed observer cannot',()=>{
 const older={...view(1197),action:'wheel',input:{}};
 assert.equal(classifyRelaunch({...sample,attempts:[older]}),'PASS');
 assert.equal(classifyRelaunch({...sample,attempts:[{...older,helperError:'failed'}]}),'NOT EXECUTED');
 assert.equal(classifyRelaunch({...sample,attempts:[{...older,markers:[1197,1199]}]}),'NOT EXECUTED');
});

test('only wheel or ShiftPageUp can prove PASS; absent scrollbar is not a tested input',()=>{
 assert.equal(classifyRelaunch({...sample,attempts:[{...view(1197),action:'scrollbar',input:{}}]}),'NOT EXECUTED');
 assert.equal(classifyRelaunch({...sample,attempts:[{...view(1197),action:'ShiftPageUp',input:{}}]}),'PASS');
});
