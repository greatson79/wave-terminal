import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, rmSync, rmdirSync, unlinkSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
const root = process.cwd();
const cargo = spawnSync('node', ['scripts/gates-v024/cargo-isolated.mjs', 'test'], { cwd: root, stdio: 'inherit' });
if (cargo.status !== 0) throw Error(`cargo test failed: ${cargo.status}`);
const ui = spawnSync('bun', ['test'], { cwd: `${root}/ui`, stdio: 'inherit' });
if (ui.status !== 0) throw Error(`ui bun test failed: ${ui.status}`);
if (process.platform === 'darwin') {
  const triple = process.arch === 'arm64' ? 'aarch64-apple-darwin' : 'x86_64-apple-darwin';
  const files = [
    [`src-tauri/binaries/cys-${triple}`, ''],
    [`src-tauri/binaries/cysd-${triple}`, ''],
    ['src-tauri/resources/pack.tar.gz', ''],
    ['src-tauri/resources/pack-manifest.json', '{}'],
    ['ui/dist/index.html', '<html></html>'],
  ];
  const createdFiles = [], createdDirs = [];
  const temp = mkdtempSync(join(tmpdir(), 'cys-v024-check-'));
  try {
    for (const [rel, content] of files) {
      const path = join(root, rel), dir = dirname(path);
      if (!existsSync(dir)) { mkdirSync(dir, { recursive: true }); createdDirs.push(dir); }
      if (!existsSync(path)) { writeFileSync(path, content); createdFiles.push(path); }
    }
    const env = { ...process.env, HOME: temp, CYS_PACK_DIR: join(temp, 'pack'),
      RUSTUP_HOME: process.env.RUSTUP_HOME || join(homedir(), '.rustup'),
      CARGO_HOME: process.env.CARGO_HOME || join(homedir(), '.cargo') };
    const app = spawnSync('cargo', ['check', '--manifest-path', 'src-tauri/Cargo.toml'], { cwd: root, env, stdio: 'inherit' });
    if (app.status !== 0) throw Error(`src-tauri cargo check failed: ${app.status}`);
  } finally {
    for (const path of createdFiles) unlinkSync(path);
    for (const dir of createdDirs.reverse()) rmdirSync(dir);
    rmSync(temp, { recursive: true, force: true });
  }
}
console.log('G10 all-tests PASS');
