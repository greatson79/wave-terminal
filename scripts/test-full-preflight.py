#!/usr/bin/env python3
"""Full-pack preflight and Windows no-symlink regression tests."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('full_pf', ROOT / 'cysjavis-pack/bin/javis_preflight.py')
pf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pf)


class FullPreflightTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        patcher = mock.patch.object(pf, 'pack_dir', return_value=str(self.root))
        patcher.start()
        self.addCleanup(patcher.stop)

    def profile(self, value='wave-light'):
        (self.root / 'manifest.json').write_text(json.dumps({
            'schema': 'wave-pack.manifest.v1', 'product': 'Wave Terminal', 'profile': value
        }))

    def test_stale_light_manifest_does_not_disable_full_pack_checks(self):
        self.profile()
        check = pf.Preflight(False, [])
        for cid in ('C20.nlm-sot', 'C21.harness-creator', 'C24.korean-law-mcp'):
            with self.subTest(cid=cid):
                self.assertFalse(check.skipped(cid))
        self.assertEqual(check.results, [])

    def test_explicit_skip_still_requires_the_requested_check_id(self):
        self.profile()
        check = pf.Preflight(False, ['C20.nlm-sot'])
        self.assertTrue(check.skipped('C20.nlm-sot'))
        self.assertFalse(check.skipped('C21.harness-creator'))
        self.assertEqual(check.results, [{
            'id': 'C20.nlm-sot', 'status': pf.SKIP, 'detail': 'skipped by --skip'
        }])

    def test_missing_unknown_and_malformed_manifest_keep_full_checks(self):
        for content in (None, '{"profile":"unknown"}', '['):
            with self.subTest(content=content):
                if content is not None:
                    (self.root / 'manifest.json').write_text(content)
                check = pf.Preflight(False, [])
                self.assertFalse(check.skipped('C21.harness-creator'))

    def test_bundled_full_directives_keep_content_pins_and_worker_rules(self):
        # Verify real packaged directives, not only isolated fixture strings.
        bundle = ROOT / 'cysjavis-pack/directives'
        import sys
        sys.path.insert(0, str(ROOT / 'cysjavis-pack/bin'))
        import javis_orchestra as orchestra
        with mock.patch.object(pf, 'pack_dir', return_value=str(ROOT / 'cysjavis-pack')):
            with mock.patch.object(pf, 'is_dept_pack', return_value=False):
                check = pf.Preflight(False, [])
                check.c03_content_pins()
        pinned = {r['id']: r['status'] for r in check.results}
        self.assertEqual(pinned['C03.pin.master'], pf.PASS, check.results)
        self.assertEqual(pinned['C03.pin.worker'], pf.PASS, check.results)
        self.assertEqual(pinned['C03.pin.reviewer'], pf.PASS, check.results)
        worker_text = (bundle / 'WORKER_DIRECTIVE.md').read_text(encoding='utf-8')
        self.assertIsNotNone(orchestra.extract_rules_from_text(worker_text))
        with mock.patch.object(orchestra, 'pack_dir', return_value=str(ROOT / 'cysjavis-pack')):
            self.assertIsNotNone(orchestra.extract_constraints())

    def test_stale_light_manifest_keeps_directive_pin_failure(self):
        self.profile()
        directory = self.root / 'directives'
        directory.mkdir()
        for name in pf.CONTENT_PINS:
            (directory / name).write_text('# stub')
        check = pf.Preflight(False, [])
        with mock.patch.object(pf, 'is_dept_pack', return_value=False):
            check.c03_content_pins()
        failures = {r['id'] for r in check.results if r['status'] == pf.FAIL}
        self.assertTrue({'C03.pin.master', 'C03.pin.worker', 'C03.pin.reviewer'} <= failures)

    def test_windows_skill_copy_detects_content_drift_and_preserves_user_files(self):
        source, dest = self.root / 'source', self.root / 'copy'
        source.mkdir()
        (source / 'SKILL.md').write_text('fixture')
        with mock.patch.object(pf.os, 'name', 'nt'), mock.patch.object(pf.os, 'symlink', side_effect=AssertionError('privilege required')):
            pf.Preflight._link_skill(str(source), str(dest))
            self.assertTrue(pf.Preflight._symlink_ok(str(dest), str(source)))
        (dest / 'SKILL.md').write_text('changed')
        with mock.patch.object(pf.os, 'name', 'nt'):
            self.assertFalse(pf.Preflight._symlink_ok(str(dest), str(source)))
            with self.assertRaises(FileExistsError):
                pf.Preflight._link_skill(str(source), str(dest))
        self.assertEqual((dest / 'SKILL.md').read_text(), 'changed')

    def test_windows_dept_launcher_is_idempotent_and_preserves_existing(self):
        binary = self.root / 'bin'
        binary.mkdir()
        (binary / 'cys-dept').write_text('#!/bin/bash\n')
        cys = str(binary / 'cys.exe')
        with mock.patch.object(pf.os, 'name', 'nt'), mock.patch.object(pf.shutil, 'which', side_effect=lambda name: cys if name == 'cys' else None), mock.patch.object(pf.os, 'symlink', side_effect=AssertionError('privilege required')):
            first = pf.Preflight(True, [])
            first.c11b_cys_dept_path()
            self.assertEqual(first.results[-1]['status'], pf.FIXED)
            second = pf.Preflight(True, [])
            second.c11b_cys_dept_path()
            self.assertEqual(second.results[-1]['status'], pf.PASS)
        launcher = binary / 'cys-dept.cmd'
        self.assertIn('bash "', launcher.read_text())
        launcher.write_text('user-owned')
        with mock.patch.object(pf.os, 'name', 'nt'), mock.patch.object(pf.shutil, 'which', return_value=cys):
            pf.Preflight(True, []).c11b_cys_dept_path()
        self.assertEqual(launcher.read_text(), 'user-owned')

if __name__ == '__main__':
    unittest.main(verbosity=2)
