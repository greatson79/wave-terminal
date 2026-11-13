import test from 'node:test';
import assert from 'node:assert/strict';
import { classifyViewport, hasNoOlderResponse } from './scroll-contract.mjs';
const before={source:'terminal-visible-markers',markers:[280,281,282]};
test('movement requires measured older markers, not a moved assertion',()=>{
 assert.equal(classifyViewport(before,{...before,moved:true}),'STATIONARY');
 assert.equal(classifyViewport(before,{source:before.source,markers:[260,261,262]}),'MOVED_UP');
 assert.equal(classifyViewport(before,{source:before.source,markers:[290,291,292]}),'MOVED_DOWN');
});
test('absent, duplicate or unbound observations are not movement proof',()=>{
 for(const after of [{moved:true},{source:'whole-page',markers:[1,2]}, {source:before.source,markers:[]},{source:before.source,markers:[1,1]}])
 assert.equal(classifyViewport(before,after),'NOT_PROVEN');
});

test('capture/helper failures cannot become measured viewport movement',()=>{
 for(const key of ['captureError','helperError','error']) assert.equal(classifyViewport(before,{source:before.source,markers:[260,261,262],[key]:'failed'}),'NOT_PROVEN');
});

test('zero-response rejects missing evidence and distinguishes older history from incoming output',()=>{
 assert.equal(hasNoOlderResponse(before,before,before),true);
 assert.equal(hasNoOlderResponse(before,before,{...before,markers:[290,291,292]}),true);
 assert.equal(hasNoOlderResponse(before,{...before,markers:[260,261,262]},before),false);
 assert.equal(hasNoOlderResponse(before,{markers:[]},before),null);
});
