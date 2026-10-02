#!/usr/bin/env python3
"""C60 출하 승인 기록 — 새 설치(빈 HOME·작업폴더 기록 없음)에서 PASS, 변조 시 FAIL.

설치 경로: CYS_BIN(빌드된 cys 바이너리)이 있으면 실제 `cys init-pack`(앱 설치 코드 = src/pack.rs
install_into)으로, 없으면 scripts/gen-shipped-acks.py install_pack(build.rs 임베드 규칙 + 매니페스트
재현)으로 temp HOME에 설치한다. preflight는 **설치된 팩의** javis_preflight.py를 로드해 돌린다.
"""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('gen_acks', ROOT / 'scripts/gen-shipped-acks.py')
gen = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gen)
REMOVED = ('timesfm-forecasting', 'git-guardrails-claude-code')


class C60ShippedAckTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name) / 'home'
        self.ws = Path(self.tmp.name) / 'ws'          # 작업폴더 — _round 기록 없음
        self.ws.mkdir(parents=True)
        self.pack = self.home / '.cys' / 'pack'
        env = {k: v for k, v in os.environ.items()
               if k not in ('CYS_PACK_DIR', 'JAVIS_PACK_DIR', 'AITERM_JARVIS_DIR', 'JAVIS_ROOT')}
        env.update(HOME=str(self.home), CLAUDE_CONFIG_DIR=str(self.home / '.claude'))
        cys = os.environ.get('CYS_BIN')
        if cys:
            r = subprocess.run([cys, 'init-pack', '--no-install-hook'], env=env,
                               capture_output=True, text=True, timeout=300)
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        else:
            self.pack.mkdir(parents=True)
            gen.install_pack(str(self.pack))
        self.assertTrue((self.pack / '.install-manifest.json').is_file())
        env.update(CYS_PACK_DIR=str(self.pack), JAVIS_ROOT=str(self.ws))
        p = mock.patch.dict(os.environ, env, clear=True)
        p.start()
        self.addCleanup(p.stop)

    def c60(self):
        spec = importlib.util.spec_from_file_location('inst_pf', self.pack / 'bin/javis_preflight.py')
        pf = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(pf)
        check = pf.Preflight(False, [])
        check.c60_gate_wiring()
        return check.results[-1]

    def test_fresh_install_passes_without_workspace_record(self):
        self.assertFalse((self.ws / '_round').exists())
        row = self.c60()
        self.assertEqual(row['status'], 'PASS', row['detail'])
        self.assertIn('BLOCK 승인 6건', row['detail'])

    def test_tampering_approved_skill_fails(self):
        f = self.pack / 'skills/kosis-stats/scripts/run_kosis_stats.py'
        f.write_text(f.read_text(encoding='utf-8') + '\n# tampered\n', encoding='utf-8')
        row = self.c60()
        self.assertEqual(row['status'], 'FAIL', row['detail'])
        self.assertIn('kosis-stats', row['detail'])

    def test_tampering_shipped_record_fails(self):
        f = self.pack / 'round/skillscan_acknowledged.json'
        acks = json.loads(f.read_text(encoding='utf-8'))
        acks['timesfm-forecasting'] = {'fingerprint': 'sha256:' + '0' * 32}
        f.write_text(json.dumps(acks), encoding='utf-8')
        row = self.c60()
        self.assertEqual(row['status'], 'FAIL', row['detail'])
        self.assertIn('변조', row['detail'])

    def test_tampering_shipped_mcp_snapshot_fails(self):
        f = self.pack / 'round/mcp_approved/kosis-stats.json'
        snap = json.loads(f.read_text(encoding='utf-8'))
        snap['permissions'] = ['bash']
        f.write_text(json.dumps(snap), encoding='utf-8')
        row = self.c60()
        self.assertEqual(row['status'], 'FAIL', row['detail'])

    def test_unbound_record_ignored_workspace_record_still_works(self):
        # 매니페스트 결속이 없으면 출하 기록은 무시(예전과 같은 WARN) — 작업폴더 기록은 계속 유효.
        (self.pack / '.install-manifest.json').unlink()
        row = self.c60()
        self.assertEqual(row['status'], 'WARN', row['detail'])
        self.assertIn('미승인', row['detail'])
        self.assertIn('mcpgate 승인 스냅샷 0', row['detail'])
        rnd = self.ws / '_round'
        rnd.mkdir()
        shutil.copy(self.pack / 'round/skillscan_acknowledged.json', rnd / 'skillscan_acknowledged.json')
        shutil.copytree(self.pack / 'round/mcp_approved', rnd / 'mcp_approved')
        row = self.c60()
        self.assertEqual(row['status'], 'PASS', row['detail'])

    def test_removed_skills_not_shipped(self):
        for s in REMOVED:
            self.assertFalse((self.pack / 'skills' / s).exists(), s)
        hits = subprocess.run(['grep', '-rlE', '|'.join(REMOVED), str(self.pack)],
                              capture_output=True, text=True).stdout.split()
        self.assertEqual([h for h in hits if not h.endswith('.install-manifest.json')], [])


class ShippedRecordDriftTests(unittest.TestCase):
    def test_committed_records_match_installed_form(self):
        r = subprocess.run(['python3', str(ROOT / 'scripts/gen-shipped-acks.py'), '--check'],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)


if __name__ == '__main__':
    unittest.main()
