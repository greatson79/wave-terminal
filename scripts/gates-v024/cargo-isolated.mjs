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
  const gate = args[1] === 'exit_code_contract_' ? ['G19 exit-code-contract PASS', 22]
    : args[1] === 'recover_scope_' ? ['G20 recover-scope PASS', 8] : null;
  const result = spawnSync('cargo', args, { env, cwd: process.cwd(),
    stdio: gate ? 'pipe' : 'inherit', encoding: gate ? 'utf8' : undefined });
  if (gate) { process.stdout.write(result.stdout || ''); process.stderr.write(result.stderr || ''); }
  if (result.error) throw result.error;
  if (result.status !== 0) process.exitCode = result.status || 1;
  else if (gate) {
    const passed = [...(result.stdout || '').matchAll(/test result: ok\. (\d+) passed/g)]
      .reduce((total, match) => total + Number(match[1]), 0);
    if (passed !== gate[1]) throw Error(`expected ${gate[1]} tests, got ${passed}`);
    console.log(gate[0]);
  }
  else if (args[1] === 'single_owner_') console.log('G7 single-owner PASS');
} finally {
  rmSync(temp, { recursive: true, force: true });
}
