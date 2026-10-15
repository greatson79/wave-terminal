import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir, homedir } from 'node:os';
import { join } from 'node:path';
const temp = mkdtempSync(join(tmpdir(), 'cys-v024-test-'));
const env = { ...process.env, HOME: temp, CYS_PACK_DIR: join(temp, 'pack'),
  RUSTUP_HOME: process.env.RUSTUP_HOME || join(homedir(), '.rustup'),
  CARGO_HOME: process.env.CARGO_HOME || join(homedir(), '.cargo') };
try {
  const args = process.argv.slice(2);
  if (args[0] !== 'test') throw Error('cargo-isolated accepts test only');
  if (args.length === 1) args.push('--quiet', '--', '--test-threads=1');
  const result = spawnSync('cargo', args, { env, cwd: process.cwd(), stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exitCode = result.status || 1;
  else if (args[1] === 'single_owner_') console.log('G7 single-owner PASS');
} finally {
  rmSync(temp, { recursive: true, force: true });
}
