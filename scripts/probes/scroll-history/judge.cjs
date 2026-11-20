// Same xterm major/minor as the UI; only synthetic input is persisted.
const fs = require('node:fs');
const { Terminal } = require('@xterm/headless');
async function judge(bytes, initialModes = Buffer.alloc(0)) {
  const term = new Terminal({ cols: 120, rows: 35, scrollback: 5000, allowProposedApi: true });
  const write = data => new Promise(resolve => term.write(data, resolve));
  if (initialModes.length) await write(initialModes);
  await write(bytes);
  function count(buffer) {
    const lines = Array.from({ length: buffer.length }, (_, i) => buffer.getLine(i).translateToString(true));
    const observed = [...new Set(lines.flatMap(line => {
      const match = line.match(/(?:^|\s)SCROLL-(\d{3})\s*$/);
      return match ? [Number(match[1])] : [];
    }))].sort((a,b) => a-b);
    const labels = observed.filter(n => n >= 1 && n <= 100);
    const outOfRangeScrollLabels = observed.filter(n => n < 1 || n > 100);
    const numbers = [...new Set(lines.filter(line => /^\d+$/.test(line.trim())).map(Number))].sort((a,b) => a-b);
    return { length: buffer.length, baseY: buffer.baseY, labels, labelCount: labels.length, outOfRangeScrollLabels,
      missingScrollLabels: Array.from({length:100},(_,i)=>i+1).filter(n=>!labels.includes(n)),
      numbers, numberCount: numbers.length, lines };
  }
  const result = { cols: 120, rows: 35, scrollback: 5000, activeType: term.buffer.active.type,
    active: count(term.buffer.active), normal: count(term.buffer.normal) };
  term.dispose();
  return result;
}
function shape(bytes) {
  const text = bytes.toString('latin1');
  const hits = re => (text.match(re) || []).length;
  const modes = [...text.matchAll(/\x1b\[\?([0-9;]+)([hl])/g)];
  const modeCount = (number, action) => modes.filter(m => m[1].split(";").includes(String(number)) && m[2] === action).length;
  return { bytes: bytes.length, eraseDisplay2: hits(/\x1b\[2J/g), eraseScrollback3: hits(/\x1b\[3J/g),
    home: hits(/\x1b\[H/g), home1_1: hits(/\x1b\[1;1H/g), cursorPosition: hits(/\x1b\[\d+;\d+H/g),
    alt1049h: modeCount(1049,"h"), alt1049l: modeCount(1049,"l"),
    sync2026h: modeCount(2026,"h"), sync2026l: modeCount(2026,"l"),
    CR: hits(/\r/g), LF: hits(/\n/g), cursorUp: hits(/\x1b\[\d*A/g), eraseLine: hits(/\x1b\[\d*K/g),
    scrollRegion: hits(/\x1b\[(?:\d*;\d*)?r/g),
    privateModes: [...new Set([...text.matchAll(/\x1b\[\?([0-9;]+)[hl]/g)].map(x=>x[1]))] };
}
module.exports = { judge, shape };
if (require.main === module) (async () => {
  if (process.argv[2] === '--self-test') {
    const assert = require('node:assert/strict');
    const plain = Buffer.from(Array.from({length:300},(_,i)=>`${i+1}\r\n`).join(''));
    assert.equal((await judge(plain)).active.numberCount, 300);
    assert.ok((await judge(Buffer.concat([plain,Buffer.from('\x1b[3J')]))).active.numberCount < 300);
    const labels=Buffer.from(Array.from({length:100},(_,i)=>`SCROLL-${String(i+1).padStart(3,'0')}\r\n`).join(''));
    assert.equal((await judge(labels)).active.labelCount,100);
    assert.equal(shape(Buffer.from('\x1b[2J\x1b[3J\x1b[?1049h\x1b[?2026l\r\n\x1b[2A\x1b[K\x1b[1;35r')).eraseScrollback3,1);
    const outside = await judge(Buffer.from('SCROLL-000\r\nSCROLL-101\r\n'));
    assert.equal(outside.active.labelCount, 0);
    assert.deepEqual(outside.active.outOfRangeScrollLabels, [0,101]);
    assert.equal(shape(Buffer.from('\x1b[12;34H')).cursorPosition, 1);
    assert.equal(shape(Buffer.from('\x1b[?25;1049h\x1b[?2026;25l')).alt1049h, 1);
    assert.equal(shape(Buffer.from('\x1b[?25;1049h\x1b[?2026;25l')).sync2026l, 1);
    console.log('RESULT=completed judge self-test: plain300, scroll100, known-clear negative, shape');
    return;
  }
  const [input, output, modes] = process.argv.slice(2);
  const bytes=fs.readFileSync(input);
  const result={ input, shape:shape(bytes), ...(await judge(bytes,modes?fs.readFileSync(modes):undefined)) };
  fs.writeFileSync(output,JSON.stringify(result,null,2)+'\n');
  console.log(JSON.stringify({input, bytes:bytes.length, activeType:result.activeType,
    activeLabels:result.active.labelCount, activeNumbers:result.active.numberCount,
    normalLabels:result.normal.labelCount, normalBaseY:result.normal.baseY, shape:result.shape}));
})().catch(error=>{console.error(error.message);process.exitCode=1;});
