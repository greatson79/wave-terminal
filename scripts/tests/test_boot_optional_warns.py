#!/usr/bin/env python3
"""CEO condition 3: approved product WARNs must not turn the first master reply into an
install/approval question. Preflight text output folds them into one line; the bootstrap
summary carries only that line. FAIL rows stay reported normally."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]


def load(name, rel):
    spec = importlib.util.spec_from_file_location(name, ROOT / rel)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


pf = load('optwarn_pf', 'cysjavis-pack/bin/javis_preflight.py')
boot = load('optwarn_boot', 'cysjavis-pack/bin/javis_bootstrap.py')

# Wording that makes the master ask the user for install approval (one list, case-insensitive).
ASK_USER_PHRASES = ('설치할까요', '승인해 주세요', '승인해주세요', '설치 승인', '허락',
                    'install now?', 'approve')
ALLOWED = ('C13.claude-md', 'C20.nlm-sot', 'C21.harness-creator', 'C24.korean-law-mcp',
           'C26.video-creator', 'C44.serena-eval', 'C48.content-channel-deps',
           'C61.doc-code-sot', 'C62.pack-heal-ledger', 'C64.directive-bench')


def assert_no_ask(test, text):
    low = text.lower()
    for phrase in ASK_USER_PHRASES:
        test.assertNotIn(phrase.lower(), low, phrase)


def run_main(rows_fn, argv=('javis_preflight.py', '--fix')):
    out = io.StringIO()
    with mock.patch.object(pf.Preflight, 'run', rows_fn), \
         mock.patch('sys.argv', list(argv)), contextlib.redirect_stdout(out):
        code = pf.main()
    return code, out.getvalue()


class OptionalWarnOutputTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix='optwarn-')
        self.addCleanup(shutil.rmtree, self.tmp, True)
        shutil.copy(ROOT / 'cysjavis-pack/preflight-product-profile.json', self.tmp)
        p = mock.patch.object(pf, 'pack_dir', return_value=self.tmp)
        p.start()
        self.addCleanup(p.stop)

    def test_allowed_list_is_the_approved_set(self):
        self.assertEqual(pf.PRODUCT_OPTIONAL_WARNS,
                         frozenset(cid.split('.')[0] for cid in ALLOWED))

    def test_allowed_warns_fold_to_one_line_and_fail_stays(self):
        def rows(check):
            for cid in ALLOWED:  # worst-case wording inside every allowed WARN detail
                check.add(cid, pf.WARN, '설치할까요? 승인해 주세요 · install now? approve 허락 설치 승인')
            check.add('C24.korean-law-mcp', pf.WARN, '두 번째 행 승인해주세요')
            check.add('C30.git', pf.WARN, 'git 미설치')
            check.add('C03.pin.worker', pf.FAIL, 'pin missing')
            return check.results
        code, out = run_main(rows)
        self.assertEqual(code, 1)
        line = pf.OPTIONAL_WARN_LINE % len(ALLOWED)
        self.assertEqual([ln for ln in out.splitlines() if '지금 필요 없음' in ln],
                         ['[WARN] %s (%s)' % (line, '·'.join(c.split('.')[0] for c in ALLOWED))])
        self.assertIn('[FAIL] C03.pin.worker — pin missing', out)
        self.assertIn('[WARN] C30.git — git 미설치', out)
        self.assertIn('NOT READY', out)
        assert_no_ask(self, out)

    def test_fail_in_allowed_id_is_not_folded(self):
        def rows(check):
            check.add('C13.claude-md', pf.FAIL, 'broken')
            return check.results
        code, out = run_main(rows)
        self.assertEqual(code, 1)
        self.assertIn('[FAIL] C13.claude-md — broken', out)
        self.assertNotIn('지금 필요 없음', out)

    def test_json_keeps_full_detail(self):
        def rows(check):
            check.add('C21.harness-creator', pf.WARN, pf.C21_OPTIONAL_DETAIL)
            return check.results
        _, out = run_main(rows, ('javis_preflight.py', '--json'))
        self.assertEqual(json.loads(out)['checks'][0]['detail'], pf.C21_OPTIONAL_DETAIL)

    @staticmethod
    def real_missing_tools(check):
        # Real C20/C21/C24 code paths on a fresh machine (tools absent, --fix, no --allow-irreversible).
        with mock.patch.object(check, '_nlm_version', return_value=(None, None)), \
             mock.patch.object(check, '_install_nlm', return_value=False), \
             mock.patch.object(check, '_harness_root', return_value=None), \
             mock.patch.object(check, '_klaw_version', return_value=(None, None)), \
             mock.patch.object(pf.subprocess, 'run', side_effect=AssertionError('no install')):
            check.c20_nlm_sot()
            check.c21_harness_creator()
            check.c24_korean_law_mcp()
        return check.results

    def preflight_text(self):
        code, out = run_main(self.real_missing_tools)
        self.assertEqual(code, 0)
        return out

    def test_real_fresh_install_checks_ready_with_one_line(self):
        out = self.preflight_text()
        self.assertIn('[WARN] %s (C20·C21·C24)' % (pf.OPTIONAL_WARN_LINE % 3), out)
        self.assertIn('READY', out)
        for banned in ('allow-irreversible', 'npm install', 'git clone', 'idoforgod', 'uv tool'):
            self.assertNotIn(banned, out)
        assert_no_ask(self, out)

    def test_bootstrap_summary_and_boot_last_carry_only_the_line(self):
        pf_out = self.preflight_text()
        home = Path(self.tmp) / 'home'
        pack = home / '.cys/pack'
        (pack / 'bin').mkdir(parents=True)
        (pack / 'bin/javis_preflight.py').write_text('# stub\n')
        boot_last = home / '.cys/state/boot-last.json'

        def fake_run(cmd, timeout=120):
            if cmd[-2:] == [str(pack / 'bin/javis_preflight.py'), '--fix']:
                return 0, pf_out
            return 0, 'ok'
        out, err = io.StringIO(), io.StringIO()
        with mock.patch.multiple(boot, HOME=str(home), CYS_DIR=str(home / '.cys'), PACK=str(pack),
                                 MARKER=str(home / '.cys/.master-bootstrapped'),
                                 STATE_DIR=str(home / '.cys/state'), BOOT_LAST=str(boot_last),
                                 _run=fake_run), \
             mock.patch.dict(os.environ, {'CYS_SOCKET': ''}), \
             contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            self.assertEqual(boot.cmd_run(), 0)
        summary = json.loads(out.getvalue())
        self.assertEqual(summary['optional'], '%s (C20·C21·C24)' % (pf.OPTIONAL_WARN_LINE % 3))
        self.assertEqual(summary['warnings'], [])
        for text in (out.getvalue(), err.getvalue(), boot_last.read_text(encoding='utf-8')):
            assert_no_ask(self, text)
            self.assertNotIn('allow-irreversible', text)


if __name__ == '__main__':
    unittest.main()
