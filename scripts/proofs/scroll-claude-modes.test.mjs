import test from 'node:test';
import assert from 'node:assert/strict';
import {claudeModes,claudeStartupFrame} from './scroll-claude-modes.mjs';
test('reads actual ESC bytes including combined modes and cursor repaint',()=>{
 const raw=Buffer.from('x\x1b[?1003;1006h\x1b[?1049l\x1b[3A\r\x1b[2K','latin1');
 const r=claudeModes(raw);
 assert.deepEqual(r.modes.map(x=>[x.offset,x.sequence]),[[1,'ESC[?1003;1006h'],[14,'ESC[?1049l']]);
 assert.deepEqual(r.cursorUp,[{offset:22,rows:3}]);assert.equal(r.eraseLineCount,1);
});
test('literal spellings, unrelated modes and incomplete sequences are not evidence',()=>{
 assert.deepEqual(claudeModes(Buffer.from('ESC[?1006h \\x1b[?1049h \x1b[?25h \x1b[?1003')).modes,[]);
});

test('startup proof requires versioned onboarding, not a Claude path in an error',()=>{
 assert.equal(claudeStartupFrame(Buffer.from("Welcome\x1b[12Gto Claude\x1b[19GCode v2.1.323")),true);
 assert.equal(claudeStartupFrame(Buffer.from('error running C:/claude.exe: invalid config')),false);
 assert.equal(claudeStartupFrame(Buffer.from('Welcome to Claude Code v0.0.0')),false);
});
