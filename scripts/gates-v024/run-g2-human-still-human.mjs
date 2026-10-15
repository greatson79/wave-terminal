import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
const root = process.cwd();
const main = readFileSync(`${root}/ui/src/main.ts`, 'utf8');
const tauri = readFileSync(`${root}/src-tauri/src/main.rs`, 'utf8');
if (!main.includes('source.takeHuman()') || !tauri.includes('human.unwrap_or(true)')) throw Error('human input path missing');
const test = spawnSync('bun', ['test', 'src/inputsource.test.ts', '-t', 'G2'], { cwd: `${root}/ui`, encoding: 'utf8' });
if (test.status !== 0 || !(test.stdout + test.stderr).includes('2 pass')) throw Error(test.stdout + test.stderr);
console.log('G2 human-keys-guarded PASS');
