import test from 'node:test';
import assert from 'node:assert/strict';
import {checkAltReceipt,checkAltScreen,classifyFixRow,decodeFixMarker,hasFixMarkerOutput} from './scroll-fix-contract.mjs';
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
 assert.equal(checkAltScreen('WR-4-7-2-1-3\nWR-4-7-2-2-9','WR-4-7-1-5-2\nWR-4-7-1-7-7',view(1282),view(1279)),true);
 assert.equal(checkAltScreen('WR-4-7-2-1-3\nWR-4-7-2-2-9','WR-4-7-2-1-3\nWR-4-7-2-2-9',view(1282),view(1279)),false);
 assert.equal(checkAltScreen('WR-4-7-2-1-3','WR-4-7-1-5-2\n[<64;96;11M',view(1282),view(1279)),false);
});
test('missing evidence is unexecuted, moved without receipt is fail',()=>{
 const row={before:view(1282),immediate:{...view(1279),input:{}},after:view(1279),mode:'alt-legacy',action:'wheel',gridBefore:'WR-4-7-2-1-3',gridAfter:'WR-4-7-1-5-2',receiptValid:true,altScreenValid:true};
 assert.equal(classifyFixRow({...row,gridAfter:''}),'NOT EXECUTED');assert.equal(classifyFixRow({...row,immediate:undefined}),'NOT EXECUTED');assert.equal(classifyFixRow({...row,action:'PageUp',altScreenValid:false}),'FAIL');
 assert.equal(classifyFixRow(row),'PASS');assert.equal(classifyFixRow({...row,receiptValid:false}),'FAIL');assert.equal(classifyFixRow({...row,immediate:{...row.immediate,captureError:'failed'}}),'NOT EXECUTED');
});

test('marker payload excludes ambiguous0/6/8 and checksum rejects a changed digit',()=>{
 for(const [marker,number] of [['WR-3-9-3-9-3',1000],['WR-3-9-4-1-3',1001],['WR-4-4-1-3-4',1178],['WR-4-4-3-1-2',1190],['WR-4-4-4-4-3',1200],['WR-4-7-4-7-1',1300]]) {
  assert.equal(decodeFixMarker(marker),number);
  const slots=marker.slice(3).split('-');
  for(let at=0;at<5;at++)for(const digit of '1234579')if(digit!==slots[at]){const changed=[...slots];changed[at]=digit;assert.equal(decodeFixMarker('WR-'+changed.join('-')),null);}
 }
 assert.equal(decodeFixMarker('WR--4--4-3-1-2'),1190);
 for(const bad of ['WR-4-4-3-1-2-1','WR-4-4-33-1-2','WR-4-4-3-0-2','WR-4-4-3-6-2','WR-4-4-3-8-2','WR4431','WR-4-4-3-1-21','WR-1-1-1-1-1'])assert.equal(decodeFixMarker(bad),null);
});

test('FIX readiness consumes the checked field marker emitted by producer',()=>{
 assert.equal(hasFixMarkerOutput('noise\nWR-4-7-4-7-1\n'),true);
 for(const text of ['WR1300','WR-4-7-4-7-2','WR-4-7-4-7','WR-4-7-4-7-1x',''])assert.equal(hasFixMarkerOutput(text),false);
});
