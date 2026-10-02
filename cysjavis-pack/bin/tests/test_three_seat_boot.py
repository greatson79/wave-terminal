"""Three-seat default boot with real bootstrap/orchestra and an isolated daemon stub."""
import argparse
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

BIN = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('three_seat_orchestra', BIN / 'javis_orchestra.py')
orch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(orch)


class ThreeSeatBoot(unittest.TestCase):
    def test_required_roles_do_not_depend_on_reviewer_installation(self):
        for installed in (False, True):
            self.assertEqual(orch.effective_required_roles(
                detect=lambda *a: (installed, 'fixture')), ['cso', 'worker'])
        result = orch.effective_required_roles()
        result.append('mutated')
        self.assertEqual(orch.REQUIRED_ROLES, ['cso', 'worker'])

    def test_check_accepts_no_reviewers_but_rejects_missing_core(self):
        for roles, expected in ((['master', 'cso', 'worker-2'], 0),
                                (['master', 'cso'], 1), (['master', 'worker'], 1)):
            status = {'surfaces': [{'role': r, 'agent_alive': True} for r in roles]}
            with patch.object(orch, 'cys_status', return_value=status), \
                 patch.object(orch, '_quiet_alive_roles', return_value={}), \
                 contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(orch.cmd_check(argparse.Namespace()), expected)

    def test_default_reviewers_never_spawn(self):
        for native in (False, True):
            roster = orch.reviewer_roster(detect=lambda *a: (native, 'fixture'))
            with patch.object(orch, 'reviewer_roster', return_value=roster), \
                 patch.object(orch, '_boot_one_node') as boot, \
                 contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(orch.cmd_boot_reviewers(argparse.Namespace(plan=False, spawn=False)), 0)
                boot.assert_not_called()

    def test_explicit_spawn_keeps_native_and_fallback_paths(self):
        for native in (False, True):
            roster = orch.reviewer_roster(detect=lambda *a: (native, 'fixture'))
            with patch.object(orch, 'reviewer_roster', return_value=roster), \
                 patch.object(orch, '_boot_one_node', return_value=True) as boot, \
                 contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(orch.cmd_boot_reviewers(argparse.Namespace(plan=False, spawn=True)), 0)
                self.assertEqual([c.args for c in boot.call_args_list],
                                 [(e['role'], e['agent']) for e in roster])

    def test_explicit_spawn_keeps_second_fallback_and_failure(self):
        roster = orch.reviewer_roster(detect=lambda *a: (True, 'fixture'))
        with patch.object(orch, 'reviewer_roster', return_value=roster), \
             patch.object(orch, '_boot_one_node', side_effect=[False, True, False, True]) as boot, \
             contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(orch.cmd_boot_reviewers(argparse.Namespace(plan=False, spawn=True)), 0)
            self.assertEqual([c.args[0] for c in boot.call_args_list],
                             ['reviewer-gemini', 'reviewer-claude-1', 'reviewer-codex', 'reviewer-claude-2'])
        with patch.object(orch, 'reviewer_roster', return_value=roster), \
             patch.object(orch, '_boot_one_node', return_value=False), \
             contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(orch.cmd_boot_reviewers(argparse.Namespace(plan=False, spawn=True)), 1)

    def test_real_bootstrap_and_orchestra_with_zero_reviewers(self):
        with tempfile.TemporaryDirectory(prefix='w3-three-seat-') as tmp:
            root = Path(tmp)
            pack = root / '.cys/pack'
            (pack / 'bin').mkdir(parents=True)
            for name in ('javis_bootstrap.py', 'javis_orchestra.py'):
                shutil.copyfile(BIN / name, pack / 'bin' / name)
            (pack / 'agents.json').write_text('{}')
            (pack / 'bin/javis_preflight.py').write_text('print("preflight fixture READY")\n')
            stub = root / 'stub'
            stub.mkdir()
            cys = stub / 'cys'
            cys.write_text('#!' + sys.executable + '\n' + '''import json, pathlib, sys
root = pathlib.Path(__file__).parent.parent
with (root / 'calls.jsonl').open('a') as f: f.write(json.dumps(sys.argv[1:])+'\\n')
if sys.argv[1] == 'status':
    print(json.dumps({'surfaces': [{'role': r, 'agent_alive': True} for r in ['master','cso','worker']]}))
elif sys.argv[1] == '--version': print('cys test-fixture')
elif sys.argv[1] not in ['ping','claim-role','boot']: sys.exit(90)
''')
            cys.chmod(0o755)
            env = dict(os.environ, HOME=str(root), CYS_PACK_DIR=str(pack),
                       PATH=str(stub) + os.pathsep + os.environ.get('PATH', ''),
                       CYS_BOOT_CHECK_RETRIES='1', CYS_BOOT_CHECK_INTERVAL_S='0',
                       CYS_SOCKET='', CYS_SURFACE_ID='7')
            result = subprocess.run([sys.executable, str(pack / 'bin/javis_bootstrap.py')],
                                    cwd=root, env=env, capture_output=True, text=True, timeout=15)
            self.assertEqual(result.returncode, 0, result.stderr)
            summary = json.loads(result.stdout)
            steps = dict(summary['steps'])
            for step in ['①preflight','②ping','③claim-role','④boot','④b-boot-reviewers','⑤check#1']:
                self.assertEqual(steps[step], 0)
            print('REAL_SCRIPTS_STUB_DAEMON reviewer_count=0 ' + json.dumps(summary, ensure_ascii=False))
            history = json.loads((root / '.cys/state/boot-last.json').read_text())
            reviewer_step = next(s for s in history['steps'] if s['step'] == '④b-boot-reviewers')
            self.assertIn('스폰 0', reviewer_step['detail'])
            calls = [json.loads(s) for s in (root / 'calls.jsonl').read_text().splitlines()]
            self.assertFalse(any(c[0] == 'launch-agent' for c in calls))
            # Public CLI accepts explicit opt-in without spawning in plan mode.
            plan = subprocess.run([sys.executable, str(pack / 'bin/javis_orchestra.py'),
                                   'boot-reviewers', '--spawn', '--plan'], cwd=root, env=env,
                                  capture_output=True, text=True, timeout=10)
            self.assertEqual(plan.returncode, 0, plan.stderr)
            self.assertIn('PLAN', plan.stdout)


if __name__ == '__main__':
    unittest.main()
