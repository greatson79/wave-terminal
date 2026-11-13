import test from 'node:test';
import assert from 'node:assert/strict';
import {checkAltReceipt,checkAltScreen,classifyFixRow,decodeFixMarker} from './scroll-fix-contract.mjs';
const view=n=>({source:'terminal-visible-markers',markers:[n,n+1,n+2]});
test('high column legacy report stays six byte exact, UTF8 expansion rejects',()=>{
 assert.equal(checkAltReceipt(Buffer.from('1b5b4d60802b','hex'),'alt-legacy',true),true);
 for(const hex of ['1b5b4d60c2802b','1b5b4d60efbfbd2b','1b5b4d607f2b','1b5b4d60802b2b'])assert.equal(checkAltReceipt(Buffer.from(hex,'hex'),'alt-legacy',true),false);
});
test('SGR report proves the same column95/row10, no literal suffix',()=>{
 assert.equal(checkAltReceipt(Buffer.from('\x1b[<64;96;11M'),'alt-sgr',true),true);
 for(const s of ['\x1b[<64;95;11M','\x1b[<64;96;12M','[<64;96;11M','\x1b[<64;96;11Mx'])assert.equal(checkAltReceipt(Buffer.from(s),'alt-sgr',true),false);
});
test('alt movement must match app grid change, no literal report text',()=>{
 assert.equal(checkAltScreen('WR47213\nWR47229','WR47152\nWR47177',view(1282),view(1279)),true);
 assert.equal(checkAltScreen('WR47213\nWR47229','WR47213\nWR47229',view(1282),view(1279)),false);
 assert.equal(checkAltScreen('WR47213','WR47152\n[<64;96;11M',view(1282),view(1279)),false);
});
test('missing evidence is unexecuted, moved without receipt is fail',()=>{
 const row={before:view(1282),immediate:{...view(1279),input:{}},after:view(1279),mode:'alt-legacy',action:'wheel',gridBefore:'WR47213',gridAfter:'WR47152',receiptValid:true,altScreenValid:true};
 assert.equal(classifyFixRow({...row,gridAfter:''}),'NOT EXECUTED');assert.equal(classifyFixRow({...row,immediate:undefined}),'NOT EXECUTED');assert.equal(classifyFixRow({...row,action:'PageUp',altScreenValid:false}),'FAIL');
 assert.equal(classifyFixRow(row),'PASS');assert.equal(classifyFixRow({...row,receiptValid:false}),'FAIL');assert.equal(classifyFixRow({...row,immediate:{...row.immediate,captureError:'failed'}}),'NOT EXECUTED');
});

test('marker payload excludes ambiguous0/6/8 and checksum rejects a changed digit',()=>{
 for(const [marker,number] of [['WR39393',1000],['WR39413',1001],['WR44134',1178],['WR44312',1190],['WR44443',1200],['WR47471',1300]]) {
  assert.equal(decodeFixMarker(marker),number);
  for(let at=2;at<7;at++)for(const digit of '1234579')if(digit!==marker[at])assert.equal(decodeFixMarker(marker.slice(0,at)+digit+marker.slice(at+1)),null);
 }
 for(const bad of ['WR44302','WR44362','WR44382','WR4431','WR443121','WR11111'])assert.equal(decodeFixMarker(bad),null);
});
