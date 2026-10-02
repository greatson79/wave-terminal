#!/usr/bin/env python3
"""Read-only DMG runtime measurement. No app launch, install, release or updater writes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def measure(dmg, out, target, runner=subprocess.run):
    dmg, out = Path(dmg).resolve(), Path(out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    result = {'dmg': str(dmg), 'target': target, 'ok': False, 'steps': []}
    env = dict(os.environ)
    env.update(HOME=str(out / 'home'), PYTHONDONTWRITEBYTECODE='1',
               NPM_CONFIG_USERCONFIG='/dev/null', NPM_CONFIG_GLOBALCONFIG='/dev/null')
    Path(env['HOME']).mkdir(exist_ok=True)

    def run(name, cmd):
        try:
            proc = runner(cmd, input=b'', capture_output=True, timeout=180, env=env,
                          cwd=env['HOME'])
            code, stdout, stderr = proc.returncode, proc.stdout, proc.stderr
        except subprocess.TimeoutExpired as exc:
            code, stdout, stderr = 124, exc.stdout or b'', exc.stderr or b''
        except OSError as exc:
            code, stdout, stderr = 127, b'', str(exc).encode()
        (out / (name + '.stdout')).write_bytes(stdout)
        (out / (name + '.stderr')).write_bytes(stderr)
        result['steps'].append({'step': name, 'command': cmd, 'exit': code,
                                'stdout_bytes': len(stdout), 'stderr_bytes': len(stderr)})
        return code

    try:
        digest = hashlib.sha256()
        with dmg.open('rb') as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b''):
                digest.update(chunk)
        result['dmg_sha256'] = digest.hexdigest()
        mount = tempfile.mkdtemp(prefix='w3-dmg-mount-')
        attached = False
        try:
            if run('attach', ['/usr/bin/hdiutil', 'attach', '-readonly', '-nobrowse',
                              '-mountpoint', mount, str(dmg)]) != 0:
                return 1
            attached = True
            apps = list(Path(mount).glob('*.app'))
            if len(apps) != 1:
                result['error'] = 'Expected exactly one top-level .app, found %d' % len(apps)
                return 1
            runtime = apps[0] / 'Contents/Resources/runtime'
            result['app_name'] = apps[0].name
            result['runtime_files'] = sorted(str(p.relative_to(runtime)) for p in runtime.rglob('*'))
            verify = Path(__file__).with_name('verify-mac-runtime.sh')
            result['ok'] = run('runtime', ['/bin/bash', str(verify), str(runtime), target]) == 0
        finally:
            if attached or os.path.ismount(mount):
                attached = True
                if run('detach', ['/usr/bin/hdiutil', 'detach', mount]) != 0:
                    result['ok'] = False
                    result['error'] = 'Read-only DMG detach failed; inspect detach.stderr'
                else:
                    attached = False
            # Never recursively remove a mount point, particularly after a failed detach.
            if not attached:
                try:
                    Path(mount).rmdir()
                except OSError:
                    pass
    except (OSError, ValueError) as exc:
        result['error'] = str(exc)
        result['ok'] = False
    finally:
        (out / 'result.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--dmg', required=True)
    ap.add_argument('--out', required=True)
    ap.add_argument('--target', default='aarch64-apple-darwin',
                    choices=['aarch64-apple-darwin', 'x86_64-apple-darwin'])
    args = ap.parse_args()
    sys.exit(measure(args.dmg, args.out, args.target))
