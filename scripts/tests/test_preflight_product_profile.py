#!/usr/bin/env python3
"""D1 profile changes verdicts only; missing/invalid profiles fail closed."""
import contextlib
import io
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('d1_pf', ROOT / 'cysjavis-pack/bin/javis_preflight.py')
pf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pf)
IDS = ('C20.nlm-sot', 'C21.harness-creator', 'C24.korean-law-mcp')
MARKER = '제품 프로필 — 선택 구성요소는 경고로 표시'
# C21 is an optional feature in this product: missing toolchain is WARN by itself (no FAIL to map).
MISSING = [pf.FAIL, pf.WARN, pf.FAIL]


class ProductProfileTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.path = Path(self.tmp.name) / 'preflight-product-profile.json'
        p = mock.patch.object(pf, 'pack_dir', return_value=self.tmp.name)
        p.start()
        self.addCleanup(p.stop)

    def write_profile(self, warnings=None):
        self.path.write_text(json.dumps({'schema_version': 1, 'decision': MARKER,
            'warnings': {cid: '제품 설치 필수 조건에서 잠정 제외' for cid in IDS}
            if warnings is None else warnings}), encoding='utf-8')

    def measure_missing_tools(self):
        check = pf.Preflight(False, [])
        with mock.patch.object(check, '_nlm_version', return_value=(None, None)), \
             mock.patch.object(check, '_harness_root', return_value=None), \
             mock.patch.object(check, '_klaw_version', return_value=(None, None)):
            check.c20_nlm_sot()
            check.c21_harness_creator()
            check.c24_korean_law_mcp()
        return check.results

    def test_absent_profile_keeps_real_checks_fail(self):
        rows = self.measure_missing_tools()
        self.assertEqual([r['status'] for r in rows], MISSING)
        self.assertTrue(all(set(r) == {'id', 'status', 'detail'} for r in rows))

    def test_present_profile_warns_and_retains_original_evidence(self):
        original = self.measure_missing_tools()
        self.write_profile()
        rows = self.measure_missing_tools()
        self.assertEqual([r['status'] for r in rows], [pf.WARN] * 3)
        self.assertEqual(rows[1], original[1])  # C21 WARN is native, not profile-mapped
        for before, after in zip(original[::2], rows[::2]):
            self.assertEqual(after['original_status'], pf.FAIL)
            self.assertEqual(after['original_detail'], before['detail'])
            self.assertIn(MARKER, after['detail'])
            self.assertIn('제품 기준 경고', after['detail'])
            self.assertIn(before['detail'], after['detail'])

    def test_delete_or_empty_replacement_restores_fail(self):
        self.write_profile()
        self.path.unlink()
        self.assertEqual([r['status'] for r in self.measure_missing_tools()], MISSING)
        self.write_profile({})
        self.assertEqual([r['status'] for r in self.measure_missing_tools()], MISSING)

    def test_other_statuses_and_other_checks_unchanged(self):
        self.write_profile()
        check = pf.Preflight(False, [])
        for status in (pf.PASS, pf.WARN, pf.FIXED, pf.SKIP, pf.BLOCKED):
            check.add(IDS[0], status, 'original')
            self.assertEqual(check.results[-1], {'id': IDS[0], 'status': status, 'detail': 'original'})
        check.add('C03.pin.worker', pf.FAIL, 'original')
        self.assertEqual(check.results[-1]['status'], pf.FAIL)

    def test_invalid_profile_does_not_weaken_any_check(self):
        for content in ('[', '[]', '{}', '{"schema_version":true}',
                json.dumps({'schema_version':1, 'decision':MARKER, 'warnings':{IDS[0]:'ok', 'C03.pin.worker':'bad'}}),
                json.dumps({'schema_version':1, 'decision':MARKER, 'warnings':{IDS[0]:'two\nlines'}})):
            with self.subTest(content=content):
                self.path.write_text(content)
                rows = self.measure_missing_tools()
                self.assertEqual([r['status'] for r in rows if r['id'] in IDS], MISSING)
                self.assertTrue(any(r['id'] == 'C00.product-profile' and r['status'] == pf.WARN for r in rows))

    def test_shipped_profile_matches_and_has_no_internal_names(self):
        shipped = json.loads((ROOT / 'cysjavis-pack/preflight-product-profile.json').read_text(encoding='utf-8'))
        self.assertEqual(shipped['decision'], MARKER)
        src = (ROOT / 'cysjavis-pack/bin/javis_preflight.py').read_text(encoding='utf-8')
        # '주인님'은 제품 기본 호칭(soul.md 보강)이라 소스엔 남는다 — 프로필·decision 문구만 금지.
        for text, banned in ((json.dumps(shipped, ensure_ascii=False), ('테오', '주인님', 'D1 잠정')),
                             (src, ('테오', 'D1 잠정'))):
            for b in banned:
                self.assertNotIn(b, text)

    def test_upgraded_install_with_legacy_decision_still_applies(self):
        # seed-once: 업그레이드 설치본의 프로필은 구 문구 그대로 남는다(.new 없음) — 원판정 회귀 금지.
        legacy = bytes.fromhex('443120ec9ea0eca09528ed858cec98a420eb8c80eab2b020c2b720eca3bcec9db8eb8b9820ed9995ec9db820eb8c80eab8b029').decode()
        self.path.write_text(json.dumps({'schema_version': 1, 'decision': legacy,
            'warnings': {cid: '잠정 제외' for cid in IDS}}), encoding='utf-8')
        rows = self.measure_missing_tools()
        self.assertEqual([r['status'] for r in rows], [pf.WARN] * 3)
        self.assertFalse(any(r['id'] == 'C00.product-profile' for r in rows))
        self.assertTrue(all(legacy not in r['detail'] for r in rows))
        self.path.write_text(json.dumps({'schema_version': 1, 'decision': 'other',
            'warnings': {cid: 'x' for cid in IDS}}), encoding='utf-8')
        self.assertEqual([r['status'] for r in self.measure_missing_tools() if r['id'] in IDS], MISSING)

    def test_thread_sink_keeps_transformed_row(self):
        self.write_profile()
        check = pf.Preflight(False, [])
        check._local.sink = []
        check.add(IDS[0], pf.FAIL, 'missing')
        self.assertEqual(check.results, [])
        self.assertEqual(check._local.sink[0]['status'], pf.WARN)

    def test_cli_json_counts_exit_and_unrelated_failure(self):
        def run_selected(check):
            for cid in IDS:
                check.add(cid, pf.FAIL, 'missing tool')
            if extra_failure[0]:
                check.add('C03.pin.worker', pf.FAIL, 'pin missing')
            return check.results
        extra_failure = [False]
        for enabled, unrelated, expected_exit, fails, warns in (
                (False, False, 1, 3, 0), (True, False, 0, 0, 3),
                (True, True, 1, 1, 3)):
            if enabled:
                self.write_profile()
            extra_failure[0] = unrelated
            output = io.StringIO()
            with mock.patch.object(pf.Preflight, 'run', run_selected), \
                 mock.patch('sys.argv', ['javis_preflight.py', '--json']), \
                 contextlib.redirect_stdout(output):
                self.assertEqual(pf.main(), expected_exit)
            result = json.loads(output.getvalue())
            self.assertEqual((result['fails'], result['warns']), (fails, warns))
            self.assertEqual(result['ok'], expected_exit == 0)

    def test_c21_missing_is_optional_warn_and_never_fetches_upstream(self):
        self.assertEqual(pf.HARNESS_REPO, '')
        for mode, allow in (('fix', True), ('fix', False), ('dry', False), ('safe', False), ('report', False)):
            with self.subTest(mode=mode, allow=allow):
                check = pf.Preflight(mode == 'fix', [], mode=mode, allow_irreversible=allow)
                with mock.patch.object(check, '_harness_root', return_value=None), \
                     mock.patch.object(pf.subprocess, 'run', side_effect=AssertionError('no subprocess')), \
                     mock.patch.object(pf.shutil, 'which', return_value='/usr/bin/git'):
                    check.c21_harness_creator()
                self.assertEqual(check.results, [{'id': 'C21.harness-creator', 'status': pf.WARN,
                                                  'detail': pf.C21_OPTIONAL_DETAIL}])
                self.assertEqual(check.planned, [])
        detail = pf.C21_OPTIONAL_DETAIL
        self.assertIn('선택 기능', detail)
        self.assertIn('지금 필요 없음', detail)
        for banned in ('git', 'clone', 'http', 'idoforgod', 'install', '설치'):
            self.assertNotIn(banned, detail)
        src = (ROOT / 'cysjavis-pack/bin/javis_preflight.py').read_text(encoding='utf-8')
        self.assertNotIn('idoforgod', src)

    def test_shipped_profile_exact_scope_and_reasons(self):
        profile = json.loads((ROOT / 'cysjavis-pack/preflight-product-profile.json').read_text())
        self.assertEqual(set(profile['warnings']), set(IDS))
        self.assertEqual(profile['decision'], MARKER)
        for reason in profile['warnings'].values():
            self.assertTrue(reason.strip())
            self.assertEqual(len(reason.splitlines()), 1)


if __name__ == '__main__':
    unittest.main()
