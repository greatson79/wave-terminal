export function claudeModes(raw) {
 const text=raw.toString('latin1');
 const targets=new Set(['1000','1002','1003','1006','1015','1049']);
 return {
  modes:[...text.matchAll(/\x1b\[\?([0-9;]+)([hl])/g)].filter(m=>m[1].split(';').some(x=>targets.has(x))).map(m=>({offset:m.index,sequence:m[0].replace('\x1b','ESC'),hex:Buffer.from(m[0],'latin1').toString('hex')})),
  cursorUp:[...text.matchAll(/\x1b\[([0-9]*)A/g)].map(m=>({offset:m.index,rows:Number(m[1]||1)})),
  eraseLineCount:[...text.matchAll(/\x1b\[(?:0|1|2)?K/g)].length,
 };
}

export function claudeStartupFrame(raw) {
 const text=raw.toString('utf8').replace(/\x1b\][\s\S]*?(?:\x07|\x1b\\)/g,'').replace(/\x1b\[[0-?]*[ -/]*[@-~]/g,'');
 return /Claude\s*Code\s*v?2\.1\.323/.test(text) && /Welcome|Let's\s*get\s*started|Choose\s*(?:the\s*)?text\s*style|How\s*would\s*you\s*like\s*to\s*log\s*in/i.test(text);
}
