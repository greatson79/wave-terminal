import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
const root = process.cwd();
const main = readFileSync(`${root}/ui/src/main.ts`, 'utf8');
if (!main.includes('source.takeHuman()') || !main.includes('data, human }))')) throw Error('onData human source is not wired to send_input');
const test = spawnSync('bun', ['test', 'src/inputsource.test.ts', '-t', 'G1'], { cwd: `${root}/ui`, encoding: 'utf8' });
if (test.status !== 0 || !(test.stdout + test.stderr).includes('2 pass')) throw Error(test.stdout + test.stderr);
console.log('G1 replies-not-human PASS');
