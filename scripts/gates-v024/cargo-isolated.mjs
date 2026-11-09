import { spawnSync } from 'node:child_process';
import { lstatSync, mkdirSync, mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { isAbsolute, join, relative, sep } from 'node:path';
import { pathToFileURL } from 'node:url';

const protectedPath = /(^|[\\/])(?:Applications|\.cys|\.wave)(?:[\\/]|$)|(?:^|[\\/])(?:opt[\\/]homebrew[\\/]bin|usr[\\/]local[\\/]bin|Library[\\/]LaunchAgents)(?:[\\/]|$)/i;

export function validateTestRoot(raw, platform = process.platform) {
  // Reject before querying the OS temp directory or resolving any filesystem path.
  if (!raw || !isAbsolute(raw) || protectedPath.test(raw)) {
    throw Error('WAVE_TEST_ROOT must be an explicit absolute disposable directory, never a system/runtime path');
  }
  if (platform === 'win32') {
    throw Error('Windows dirs6 uses the real Known Folder profile; cargo-isolated requires a reviewed Windows isolation harness on CI/clean VM');
  }
  const root = realpathSync(raw);
  const parent = realpathSync(tmpdir());
  if (!/^(?:\/private)?\/(?:tmp|var\/tmp)(?:\/|$)/.test(parent)
      && !(platform === 'darwin' && parent.startsWith('/private/var/folders/'))) {
    throw Error('WAVE_TEST_ROOT requires an OS temporary location; inherited TMPDIR cannot select a system directory');
  }
  const rel = relative(parent, root);
  if (!rel || rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel)
      || protectedPath.test(root) || lstatSync(raw).isSymbolicLink() || !lstatSync(root).isDirectory()) {
    throw Error('WAVE_TEST_ROOT must be a real disposable subdirectory of the OS temp directory');
  }
  return root;
}

export function isolatedEnv(inherited, temp, buildRoot = temp) {
  const env = Object.fromEntries(Object.entries(inherited).filter(([key]) =>
    !/^(?:CYS_|AITERM_|JAVIS_|WAVE_|CLAUDE_|XDG_|CARGO_|RUSTC|RUSTDOC|RUSTFLAGS|DYLD_|LD_PRELOAD)/i.test(key)));
  for (const key of ['CARGO_HOME', 'RUSTUP_HOME']) {
    const candidate = inherited[key] || (inherited.HOME && join(inherited.HOME, key === 'CARGO_HOME' ? '.cargo' : '.rustup'));
    env[key] = join(temp, key.toLowerCase());
    if (candidate && isAbsolute(candidate) && !protectedPath.test(candidate)) {
      try {
        const cache = realpathSync(candidate);
        if (!protectedPath.test(cache) && lstatSync(cache).isDirectory()) env[key] = cache;
      } catch (error) {
        if (error.code !== 'ENOENT') throw error;
      }
    }
  }
  return { ...env, HOME: temp, USERPROFILE: temp, CARGO_TARGET_DIR: join(buildRoot, 'target'),
    TMPDIR: join(temp, 'tmp'), TMP: join(temp, 'tmp'), TEMP: join(temp, 'tmp'),
    XDG_CONFIG_HOME: join(temp, 'config'), XDG_DATA_HOME: join(temp, 'data'),
    XDG_STATE_HOME: join(temp, 'state'), XDG_CACHE_HOME: join(temp, 'cache'),
    XDG_RUNTIME_DIR: join(temp, 'run'),
    WAVE_TEST_ROOT: temp, CYS_NO_AUTOSTART: '1',
    CYS_HOME: temp, CYS_PACK_DIR: join(temp, 'pack'), CYS_CONFIG_DIR: join(temp, 'claude'),
    CYS_ACCOUNT_DIR: join(temp, 'claude'), CYS_LOCAL_DIR: join(temp, 'local'),
    CYS_SOCKET: join(temp, 'run', 'cys.sock'),
    CLAUDE_CONFIG_DIR: join(temp, 'claude') };
}

export function main() {
const root = validateTestRoot(process.env.WAVE_TEST_ROOT);
const args = process.argv.slice(2);
if (args[0] !== 'test') throw Error('cargo-isolated accepts test only');
const temp = mkdtempSync(join(root, 'cargo-'));
try {
  const env = isolatedEnv(process.env, temp, root);
  const target = env.CARGO_TARGET_DIR;
  try {
    const metadata = lstatSync(target);
    if (metadata.isSymbolicLink() || !metadata.isDirectory()) throw Error('WAVE_TEST_ROOT target must be a real build directory');
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  for (const dir of ['tmp', 'config', 'data', 'state', 'cache', 'run', 'pack', 'claude', 'local']) {
    mkdirSync(join(temp, dir), { mode: 0o700 });
  }
  if (args.length === 1) args.push('--quiet', '--', '--test-threads=1');
  const gate = args[1] === 'exit_code_contract_' ? ['G19 exit-code-contract PASS', 22]
    : args[1] === 'recover_scope_' ? ['G20 recover-scope PASS', 12]
    : args[1] === 'label_scope_' ? ['G21 label-scope PASS', 10]
    : args[1] === 'pack_owner_' ? ['G24 pack-owner PASS', 4] : null;
  const result = spawnSync('cargo', args, { env, cwd: process.cwd(),
    stdio: gate ? 'pipe' : 'inherit', encoding: gate ? 'utf8' : undefined });
  if (gate) { process.stdout.write(result.stdout || ''); process.stderr.write(result.stderr || ''); }
  if (result.error) throw result.error;
  if (result.status !== 0) process.exitCode = result.status || 1;
  else if (gate) {
    const passed = [...(result.stdout || '').matchAll(/test result: ok\. (\d+) passed/g)]
      .reduce((total, match) => total + Number(match[1]), 0);
    if (passed !== gate[1]) throw Error(`expected ${gate[1]} tests, got ${passed}`);
    console.log(args[1] === 'exit_code_contract_'
      ? `${gate[0]} (source-structure ${passed} · behavior 0)` : gate[0]);
  }
  else if (args[1] === 'single_owner_') console.log('G7 single-owner PASS');
} finally {
  rmSync(temp, { recursive: true, force: true });
}
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try { main(); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
