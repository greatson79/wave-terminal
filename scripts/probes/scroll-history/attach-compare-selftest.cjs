const assert = require('node:assert/strict');
const { compareWindow } = require('./attach-compare.cjs');
const begin = Buffer.from('BEGIN_unique');
const end = Buffer.from('END_unique');
const frame = payload => Buffer.concat([Buffer.from('snapshot'), begin, Buffer.from(payload), end, Buffer.from('later snapshot')]);
const reference = frame('abcdef');
assert.deepEqual(compareWindow(reference, frame('abcdef'), begin, end), {
  comparable:true, equal:true, referenceBytes:6, clientBytes:6, missingBytes:0, omission:false,
});
assert.equal(compareWindow(reference, frame('abef'), begin, end).omission, true);
assert.equal(compareWindow(reference, frame('abXYef'), begin, end).omission, false);
assert.equal(compareWindow(reference, frame('abcdefg'), begin, end).missingBytes, -1);
assert.equal(compareWindow(reference, Buffer.from('unfinished'), begin, end).comparable, false);
assert.equal(compareWindow(reference, Buffer.concat([end, begin]), begin, end).comparable, false);
assert.equal(compareWindow(reference, Buffer.concat([begin, frame('abcdef')]), begin, end).comparable, false);
assert.equal(compareWindow(Buffer.from('ABAokEND'),Buffer.from('ABABAokEND'),Buffer.from('ABA'),Buffer.from('END')).comparable,false);
const chunks = [reference.subarray(0, 13), reference.subarray(13, 20), reference.subarray(20)];
assert.equal(compareWindow(reference, Buffer.concat(chunks), begin, end).equal, true);
console.log('PASS aligned window equality, omission, mutation, excess, missing/reversed/duplicate markers, chunk-independent comparison');
