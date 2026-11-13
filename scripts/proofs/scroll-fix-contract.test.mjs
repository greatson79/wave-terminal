import test from 'node:test';
import assert from 'node:assert/strict';
import {checkAltReceipt,checkAltScreen,classifyFixRow} from './scroll-fix-contract.mjs';
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
 assert.equal(checkAltScreen('WR1281\nWR1282','WR1278\nWR1279',view(1282),view(1279)),true);
 assert.equal(checkAltScreen('WR1281\nWR1282','WR1281\nWR1282',view(1282),view(1279)),false);
 assert.equal(checkAltScreen('WR1281','WR1278\n[<64;96;11M',view(1282),view(1279)),false);
});
test('missing evidence is unexecuted, moved without receipt is fail',()=>{
 const row={before:view(1282),immediate:{...view(1279),input:{}},after:view(1279),mode:'alt-legacy',action:'wheel',gridBefore:'WR1281',gridAfter:'WR1278',receiptValid:true,altScreenValid:true};
 assert.equal(classifyFixRow({...row,gridAfter:''}),'NOT EXECUTED');assert.equal(classifyFixRow({...row,immediate:undefined}),'NOT EXECUTED');assert.equal(classifyFixRow({...row,action:'PageUp',altScreenValid:false}),'FAIL');
 assert.equal(classifyFixRow(row),'PASS');assert.equal(classifyFixRow({...row,receiptValid:false}),'FAIL');assert.equal(classifyFixRow({...row,immediate:{...row.immediate,captureError:'failed'}}),'NOT EXECUTED');
});
