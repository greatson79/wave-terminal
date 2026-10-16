import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const result = spawnSync('bun', ['test', 'src/paneheading.test.ts'], {
  cwd: fileURLToPath(new URL('../../ui/', import.meta.url)), encoding: 'utf8',
});
process.stdout.write(result.stdout || '');
process.stderr.write(result.stderr || '');
if (result.error) throw result.error;
const passed = Number((result.stdout + result.stderr).match(/\b(\d+) pass\b/)?.[1]);
const failed = Number((result.stdout + result.stderr).match(/\b(\d+) fail\b/)?.[1]);
if (result.status !== 0 || passed !== 6 || failed !== 0) {
  throw Error(`paneheading tests: exit=${result.status}, pass=${passed}, fail=${failed}`);
}
console.log('G22 paneheading PASS');
