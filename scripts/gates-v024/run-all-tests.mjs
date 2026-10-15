import { spawnSync } from 'node:child_process';
const root = process.cwd();
const cargo = spawnSync('node', ['scripts/gates-v024/cargo-isolated.mjs', 'test'], { cwd: root, stdio: 'inherit' });
if (cargo.status !== 0) throw Error(`cargo test failed: ${cargo.status}`);
const ui = spawnSync('bun', ['test'], { cwd: `${root}/ui`, stdio: 'inherit' });
if (ui.status !== 0) throw Error(`ui bun test failed: ${ui.status}`);
console.log('G10 all-tests PASS');
