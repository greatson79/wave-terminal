#!/usr/bin/env python3
"""Isolated profile applicability and Windows no-symlink regression tests."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('light_pf', ROOT / 'cysjavis-pack/bin/javis_preflight.py')
pf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pf)


class LightPreflightTests(unittest.TestCase):
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

    def test_light_external_checks_never_install_or_require_login(self):
        self.profile()
        check = pf.Preflight(True, [])
        with mock.patch.object(pf.subprocess, 'run', side_effect=AssertionError('external call')):
            check.c20_nlm_sot()
            check.c21_harness_creator()
            check.c24_korean_law_mcp()
        self.assertEqual({r['id'] for r in check.results}, set(pf.LIGHT_OPTIONAL_CHECKS))
        self.assertTrue(all(r['status'] == pf.SKIP for r in check.results))
        self.assertEqual(check.planned, [])

    def test_missing_unknown_and_malformed_manifest_keep_full_checks(self):
        self.assertEqual(pf.Preflight(False, []).profile, 'full')
        self.profile('unknown')
        self.assertFalse(pf.Preflight(False, []).skipped('C21.harness-creator'))
        (self.root / 'manifest.json').write_text('[')
        self.assertEqual(pf.Preflight(False, []).profile, 'full')

    def test_light_keeps_directive_pin_failure(self):
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
