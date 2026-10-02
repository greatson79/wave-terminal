#!/usr/bin/env python3
"""Updater release wiring: latest endpoint, space-free asset name, latest.json == uploaded name, ad-hoc resign."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
MAKE = ROOT / 'scripts/make-update-manifest.sh'
CHECK = ROOT / 'scripts/check-update-manifest.sh'
NAME_RE = re.compile(r'^wave-terminal-\d+\.\d+\.\d+-macos-(arm64|x64)\.app\.tar\.gz$')


def sh(*args, cwd=None, env=None):
    return subprocess.run(['sh', *map(str, args)], cwd=cwd, env=env, capture_output=True, text=True)


class UpdaterConfigTests(unittest.TestCase):
    def test_endpoint_is_latest_release_path(self):
        conf = json.loads((ROOT / 'src-tauri/tauri.conf.json').read_text(encoding='utf-8'))
        self.assertEqual(conf['plugins']['updater']['endpoints'],
                         ['https://github.com/greatson79/wave-terminal/releases/latest/download/latest.json'])

    def test_asset_name_space_free_and_conventional(self):
        for arch, want in (('aarch64', 'arm64'), ('aarch64-apple-darwin', 'arm64'),
                           ('x86_64', 'x64'), ('x86_64-apple-darwin', 'x64')):
            r = sh(MAKE, '--asset-name', '0.1.2', arch)
            self.assertEqual(r.returncode, 0, r.stderr)
            name = r.stdout.strip()
            self.assertEqual(name, 'wave-terminal-0.1.2-macos-%s.app.tar.gz' % want)
            self.assertNotIn(' ', name)
            self.assertRegex(name, NAME_RE)

    def test_release_workflow_publishes_space_free_name(self):
        y = (ROOT / '.github/workflows/release.yml').read_text(encoding='utf-8')
        self.assertIn('sh scripts/make-update-manifest.sh "$VERSION"', y)
        self.assertIn('rm -f "$BUNDLE/macos/$PRODUCT.app.tar.gz" "$BUNDLE/macos/$PRODUCT.app.tar.gz.sig"', y)
        self.assertIn('gh release upload "${{ github.ref_name }}" dist-update/*.app.tar.gz', y)
        self.assertIn('sh scripts/check-update-manifest.sh dist-update/latest.json', y)
        b = (ROOT / 'scripts/build-macos-signed.sh').read_text(encoding='utf-8')
        self.assertIn('sh scripts/make-update-manifest.sh "$VERSION" greatson79 wave-terminal "$TARGET" "$APP.tar.gz.sig"', b)

    def test_release_doc_has_url_asset_check(self):
        doc = (ROOT / 'docs/RELEASE.md').read_text(encoding='utf-8')
        self.assertIn('sh scripts/check-update-manifest.sh dist-update/latest.json greatson79/wave-terminal v', doc)


class ManifestGenerationTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp)
        self.env = dict(os.environ, UPDATE_OUT=str(self.tmp / 'out'))

    def build(self, arch, base=None):
        d = self.tmp / ('bundle-' + arch)
        d.mkdir()
        (d / 'Wave Terminal.app.tar.gz').write_bytes(b'tar-' + arch.encode())
        (d / 'Wave Terminal.app.tar.gz.sig').write_text('SIG-' + arch)
        args = ['0.1.2', 'greatson79', 'wave-terminal', arch, d / 'Wave Terminal.app.tar.gz.sig']
        if base:
            args.append(base)
        r = sh(MAKE, *args, env=self.env)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)

    def test_latest_json_references_exact_uploaded_asset(self):
        out = self.tmp / 'out'
        (out).mkdir()
        (out / 'latest.json').write_text(json.dumps({'version': '0.1.2', 'notes': 'n', 'pub_date': 'p',
            'platforms': {'windows-x86_64': {'signature': 'W', 'url':
                'https://github.com/greatson79/wave-terminal/releases/download/v0.1.2/Wave.Terminal_0.1.2_x64-setup.exe'}}}))
        self.build('aarch64', out / 'latest.json')
        self.build('x86_64', out / 'latest.json')
        doc = json.loads((out / 'latest.json').read_text())
        self.assertEqual(set(doc['platforms']), {'windows-x86_64', 'darwin-aarch64', 'darwin-x86_64'})
        self.assertEqual(doc['platforms']['darwin-aarch64']['signature'], 'SIG-aarch64')
        uploaded = sorted(p.name for p in out.iterdir() if p.name != 'latest.json')
        self.assertEqual(uploaded, ['wave-terminal-0.1.2-macos-arm64.app.tar.gz', 'wave-terminal-0.1.2-macos-arm64.app.tar.gz.sig',
                                    'wave-terminal-0.1.2-macos-x64.app.tar.gz', 'wave-terminal-0.1.2-macos-x64.app.tar.gz.sig'])
        for key in ('darwin-aarch64', 'darwin-x86_64'):
            name = doc['platforms'][key]['url'].rsplit('/', 1)[1]
            self.assertIn(name, uploaded)
            self.assertRegex(name, NAME_RE)
        assets = self.tmp / 'assets.txt'
        assets.write_text('\n'.join(uploaded + ['Wave.Terminal_0.1.2_x64-setup.exe', 'latest.json']) + '\n')
        r = sh(CHECK, out / 'latest.json', 'greatson79/wave-terminal', 'v0.1.2', assets)
        self.assertEqual(r.returncode, 0, r.stdout)
        # 자산 하나라도 실제 업로드 목록에 없으면(예: 공백 이름 → GitHub 가 '.'으로 바꿈) 실패
        assets.write_text('\n'.join(uploaded[:2] + ['Wave.Terminal_0.1.2_x64-setup.exe']) + '\n')
        r = sh(CHECK, out / 'latest.json', 'greatson79/wave-terminal', 'v0.1.2', assets)
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn('darwin-x86_64', r.stdout)
        r = sh(CHECK, out / 'latest.json', 'greatson79/wave-terminal', 'v0.1.3', assets)
        self.assertEqual(r.returncode, 1, r.stdout)

    def test_spaced_url_rejected(self):
        j = self.tmp / 'l.json'
        j.write_text(json.dumps({'platforms': {'darwin-aarch64': {'url':
            'https://github.com/o/r/releases/download/v1/Wave Terminal_aarch64.app.tar.gz'}}}))
        a = self.tmp / 'a.txt'
        a.write_text('Wave Terminal_aarch64.app.tar.gz\n')
        self.assertEqual(sh(CHECK, j, 'o/r', 'v1', a).returncode, 1)


@unittest.skipUnless(sys.platform == 'darwin' and shutil.which('codesign'), 'macOS codesign only')
class AdhocResignTests(unittest.TestCase):
    def test_inside_out_adhoc_resign_passes_strict_verify(self):
        tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp)
        app = tmp / 'Demo App.app'
        (app / 'Contents/MacOS').mkdir(parents=True)
        (app / 'Contents/Resources/runtime/bin').mkdir(parents=True)
        fw = app / 'Contents/Frameworks/Inner.framework'
        (fw / 'Resources').mkdir(parents=True)
        (app / 'Contents/Info.plist').write_text(
            '<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>'
            '<key>CFBundleExecutable</key><string>demo</string>'
            '<key>CFBundleIdentifier</key><string>test.demo</string></dict></plist>')
        shutil.copy('/usr/bin/true', app / 'Contents/MacOS/demo')
        shutil.copy('/bin/echo', app / 'Contents/Resources/runtime/bin/tool')
        shutil.copy('/usr/bin/true', fw / 'Inner')
        (fw / 'Resources/Info.plist').write_text(
            '<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>'
            '<key>CFBundleExecutable</key><string>Inner</string>'
            '<key>CFBundleIdentifier</key><string>test.inner</string></dict></plist>')
        (app / 'Contents/Resources/data.txt').write_text('x')
        # 봉인 후 리소스 변경 → strict 검증 실패(빌드 후 dedup·런타임 주입과 같은 상태)
        subprocess.run(['codesign', '--force', '--sign', '-', str(app)], capture_output=True)
        (app / 'Contents/Resources/data.txt').write_text('changed')
        before = subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], capture_output=True)
        self.assertNotEqual(before.returncode, 0)
        r = subprocess.run(['bash', str(ROOT / 'scripts/macos-adhoc-resign.sh'), str(app)],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn('NOT NOTARIZED', r.stdout)
        self.assertIn('Mach-O 3개', r.stdout)
        after = subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], capture_output=True)
        self.assertEqual(after.returncode, 0, after.stderr)


if __name__ == '__main__':
    unittest.main()
