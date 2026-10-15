import { spawnSync, execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
const root = process.cwd();
const pkg = JSON.parse(readFileSync(`${root}/ui/package.json`, 'utf8'));
const installed = JSON.parse(readFileSync(`${root}/ui/node_modules/@xterm/xterm/package.json`, 'utf8'));
const lock = readFileSync(`${root}/ui/bun.lock`, 'utf8');
if (pkg.dependencies['@xterm/xterm'] !== '5.5.0' || installed.version !== '5.5.0' || !lock.includes('"@xterm/xterm@5.5.0"')) throw Error('xterm must be exactly 5.5.0');
const current = readFileSync(`${root}/src/bin/cysd/handlers.rs`, 'utf8');
const baseline = execFileSync('git', ['show', 'f3c404e:src/bin/cysd/handlers.rs'], { cwd: root, encoding: 'utf8' });
const regexBody = s => {
  const start = s.indexOf('fn is_terminal_query_reply(');
  return s.slice(start, s.indexOf('\n}\n', start) + 3);
};
if (regexBody(current) !== regexBody(baseline)) throw Error('daemon reply regex changed');
const test = spawnSync('bun', ['test', 'src/inputsource.test.ts', '-t', 'G3'], { cwd: `${root}/ui`, encoding: 'utf8' });
if (test.status !== 0 || !(test.stdout + test.stderr).includes('1 pass')) throw Error(test.stdout + test.stderr);
console.log('G3 fallback-human-warn-once PASS');
