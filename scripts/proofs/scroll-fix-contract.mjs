import {classifyViewport} from './scroll-contract.mjs';
export function checkAltReceipt(bytes,mode,highColumn=false) {
 const raw=Buffer.from(bytes);
 if(mode==='alt-legacy')return raw.length===6 && raw.subarray(0,4).equals(Buffer.from([27,91,77,96])) && raw[4]>=33 && raw[5]>=33 && (!highColumn || raw.equals(Buffer.from('1b5b4d60802b','hex')));
 const m=/^\x1b\[<64;(\d+);(\d+)M$/.exec(raw.toString('latin1'));
 return !!m && Number(m[1])>=1 && Number(m[2])>=1 && (!highColumn || (Number(m[1])===96 && Number(m[2])===11));
}
export function checkAltScreen(beforeGrid,afterGrid,before,after) {
 const lines=text=>text.split(/\r?\n/).map(s=>s.trim()).filter(Boolean);
 const a=lines(beforeGrid),b=lines(afterGrid);
 if(!a.length || !b.length || ![...a,...b].every(line=>/^WR\d{4}$/.test(line)))return false;
 const start=text=>Number(text[0].slice(2));
 return classifyViewport(before,after)==='MOVED_UP' && start(b)<start(a) && before.markers[0]-start(a)===after.markers[0]-start(b);
}
export function classifyFixRow(row) {
 if(row.immediate?.error || row.immediate?.helperError || row.after?.error || row.after?.helperError || !row.immediate?.input || [row.immediate,row.after].some(o=>classifyViewport(row.before,o)==='NOT_PROVEN'))return 'NOT EXECUTED';
 if(row.mode.startsWith('alt-') && (!row.gridBefore?.trim() || !row.gridAfter?.trim()))return 'NOT EXECUTED';
 if(classifyViewport(row.before,row.immediate)!=='MOVED_UP' || classifyViewport(row.before,row.after)!=='MOVED_UP')return 'FAIL';
 if(row.mode.startsWith('alt-') && (!row.altScreenValid || (row.action.startsWith('wheel') && !row.receiptValid)))return 'FAIL';
 return 'PASS';
}
