import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "javis_preflight.py"
DEPT_TOOL = SCRIPT.parent / "cys-dept"


class DeptStatusLineTest(unittest.TestCase):
    def test_new_and_existing_department_accounts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            pack = root / "pack"
            (pack / "hooks").mkdir(parents=True)
            (pack / "hooks" / "cys-statusline.sh").write_text("#!/bin/sh\n")
            account = root / "dept-1"
            account.mkdir()
            settings = account / "settings.json"
            settings.write_text(json.dumps({"statusLine": {"type": "command", "command": "previous"}, "theme": "dark"}))
            env = {**os.environ, "HOME": tmp, "CYS_PACK_DIR": str(pack), "CYS_DEPTS_JSON": str(root / "depts.json")}
            subprocess.run([sys.executable, str(SCRIPT), "--statusline-account", str(account)], env=env, check=True)
            data = json.loads(settings.read_text())
            self.assertEqual(data["theme"], "dark")
            self.assertIn("CYS_PREV_STATUSLINE", data["statusLine"]["command"])
            self.assertTrue((account / "settings.json.bak-preflight").exists())
            (root / "depts.json").write_text(json.dumps({"depts": {"dept-1": {"account_dir": str(account)}}}))
            before = settings.read_bytes()
            subprocess.run([sys.executable, str(SCRIPT), "--statusline-depts"], env=env, check=True)
            self.assertEqual(settings.read_bytes(), before)

    def test_invalid_settings_are_preserved(self):
        with tempfile.TemporaryDirectory() as tmp:
            account = Path(tmp) / "dept-2"
            account.mkdir()
            settings = account / "settings.json"
            settings.write_text("{")
            result = subprocess.run([sys.executable, str(SCRIPT), "--statusline-account", str(account)],
                                    env={**os.environ, "HOME": tmp, "CYS_PACK_DIR": tmp}, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(settings.read_text(), "{")
            settings.write_text("[]")
            result = subprocess.run([sys.executable, str(SCRIPT), "--statusline-account", str(account)],
                                    env={**os.environ, "HOME": tmp, "CYS_PACK_DIR": tmp}, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(settings.read_text(), "[]")

    def test_one_bad_department_does_not_skip_other_accounts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            good = root / "dept-good"
            good.mkdir()
            (root / "depts.json").write_text(json.dumps({"depts": {
                "bad": {"account_dir": str(root / "missing")},
                "good": {"account_dir": str(good)},
            }}))
            result = subprocess.run([sys.executable, str(SCRIPT), "--statusline-depts"],
                                    env={**os.environ, "HOME": tmp, "CYS_PACK_DIR": tmp,
                                         "CYS_DEPTS_JSON": str(root / "depts.json")}, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("cys-statusline.sh", (good / "settings.json").read_text())

    def test_invalid_registry_root_reports_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            reg = Path(tmp) / "depts.json"
            reg.write_text("[]")
            env = {**os.environ, "HOME": tmp, "CYS_ROLE": "",
                   "CYS_PACK_DIR": str(SCRIPT.parents[1]), "CYS_DEPTS_JSON": str(reg)}
            bash = os.environ.get("CYS_TEST_BASH", "bash")
            if bash != "bash":
                ambient = subprocess.run(["bash", str(DEPT_TOOL), "statusline-all"], env=env, capture_output=True)
                print(f"ambient bash: rc={ambient.returncode} stdout={ambient.stdout!r} stderr={ambient.stderr!r}", file=sys.stderr)
            result = subprocess.run([bash, str(DEPT_TOOL), "statusline-all"], env=env, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("등록부", result.stderr.decode(),
                          f"bash={bash} rc={result.returncode} stdout={result.stdout!r} stderr={result.stderr!r}")
            self.assertNotIn("Traceback", result.stderr.decode())


if __name__ == "__main__":
    unittest.main()
