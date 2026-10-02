import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('dmg_measure', ROOT / 'scripts/measure-macos-runtime-dmg.py')
measure = importlib.util.module_from_spec(spec)
spec.loader.exec_module(measure)


class DmgMeasurement(unittest.TestCase):
    def scenario(self, attach=0, verify=0, detach=0, apps=1, timeout=False, attach_timeout=False):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            dmg = root / 'fixture.dmg'
            dmg.write_bytes(b'fixture-dmg-bytes')
            mount = root / 'mount'
            mount.mkdir()
            calls = []
            def runner(cmd, **kwargs):
                calls.append(cmd)
                if cmd[1] == 'attach':
                    for n in range(apps):
                        (mount / ('Fixture%d.app' % n) / 'Contents/Resources/runtime').mkdir(parents=True)
                    if attach_timeout:
                        raise subprocess.TimeoutExpired(cmd, 180, output=b"mounted then timed out")
                    code = attach
                elif cmd[1] == 'detach':
                    code = detach
                else:
                    if timeout:
                        raise subprocess.TimeoutExpired(cmd, 180, output=b'partial', stderr=b'timed out')
                    code = verify
                return subprocess.CompletedProcess(cmd, code, b'output', b'error' if code else b'')
            with patch.object(measure.tempfile, 'mkdtemp', return_value=str(mount)), \
                 patch.object(measure.os.path, 'ismount', return_value=attach_timeout):
                code = measure.measure(dmg, root / 'out', 'aarch64-apple-darwin', runner)
            result = json.loads((root / 'out/result.json').read_text())
            return code, result, calls

    def test_success_requires_readonly_attach_verify_and_detach(self):
        code, result, calls = self.scenario()
        self.assertEqual(code, 0)
        self.assertTrue(result['ok'])
        self.assertEqual([s['step'] for s in result['steps']], ['attach', 'runtime', 'detach'])
        self.assertIn('-readonly', calls[0])
        self.assertIn('-nobrowse', calls[0])
        self.assertEqual(len(result['dmg_sha256']), 64)

    def test_attach_timeout_after_mount_still_detaches(self):
        code, result, calls = self.scenario(attach_timeout=True)
        self.assertEqual(code, 1)
        self.assertFalse(result['ok'])
        self.assertEqual(result['steps'][0]['exit'], 124)
        self.assertEqual([s['step'] for s in result['steps']], ['attach', 'detach'])
        self.assertEqual(calls[-1][1], 'detach')

    def test_runtime_failure_is_not_success_and_still_detaches(self):
        code, result, calls = self.scenario(verify=1)
        self.assertEqual(code, 1)
        self.assertFalse(result['ok'])
        self.assertEqual(result['steps'][1]['exit'], 1)
        self.assertEqual(calls[-1][1], 'detach')

    def test_attach_failure_never_runs_verifier(self):
        code, result, calls = self.scenario(attach=1)
        self.assertEqual(code, 1)
        self.assertEqual(len(calls), 1)
        self.assertFalse(result['ok'])

    def test_detach_failure_is_not_reported_as_success(self):
        code, result, calls = self.scenario(detach=1)
        self.assertEqual(code, 1)
        self.assertIn('detach failed', result['error'])

    def test_ambiguous_apps_rejected_with_detach(self):
        for count in (0, 2):
            code, result, calls = self.scenario(apps=count)
            self.assertEqual(code, 1)
            self.assertEqual(len(calls), 2)
            self.assertEqual(calls[-1][1], 'detach')

    def test_timeout_preserves_partial_output_and_detaches(self):
        code, result, calls = self.scenario(timeout=True)
        self.assertEqual(code, 1)
        self.assertEqual(result['steps'][1]['exit'], 124)
        self.assertEqual(result['steps'][1]['stdout_bytes'], len(b'partial'))
        self.assertEqual(calls[-1][1], 'detach')


if __name__ == '__main__':
    unittest.main()
