#!/usr/bin/env python3
"""Hermetic path regressions: extracted real functions, no daemon or learner profile."""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

DEPT = Path(__file__).resolve().parents[1] / "cys-dept"
BASH = os.environ.get("CYS_TEST_BASH", "bash")


class DepartmentPathTests(unittest.TestCase):
    def test_registry_readers_keep_windows_escape_sequences_as_data(self):
        source = DEPT.read_text(encoding="utf-8")
        definitions = "\n".join(re.findall(r"^reg_(?:count|names)\(\).*", source, re.M))
        for path in [r"C:\Users\최형석\내 팀\depts.json", r"D:\a\최형석\depts.json", "registry ' quoted.json"]:
            with self.subTest(path=path), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                if os.name == "nt":
                    target = root / "Users" / "최형석" / "a" / "내 팀" / "depts.json"
                    target.parent.mkdir(parents=True)
                    path = str(target)
                else:
                    target = root / path
                target.write_text(json.dumps({"depts": {"dept-3": {}}}), encoding="utf-8")
                program = 'reg_init(){ :; }\n' + definitions + '\nreg_count\nreg_names\n'
                result = subprocess.run([BASH, "-c", program], env={**os.environ, "REG": path},
                                        cwd=root, input="", text=True, encoding="utf-8", capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.splitlines(), ["1", "dept-3"])

    def test_native_python_boundary_converts_msys_argv_without_rewriting_code_or_pipe(self):
        source = DEPT.read_text(encoding="utf-8")
        marker = "# HOTFIX-PYTHON-BOUNDARY-BEGIN"
        self.assertIn(marker, source, "one native-Python argv boundary is required")
        boundary = source.split(marker, 1)[1].split("# HOTFIX-PYTHON-BOUNDARY-END", 1)[0]
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "uname").write_text("#!/bin/sh\nprintf 'MINGW64_NT\\n'\n", encoding="utf-8")
            (root / "cygpath").write_text("#!/bin/sh\nprintf 'C:/%s\\n' \"${2#/c/}\"\n", encoding="utf-8")
            (root / "python3").write_text("#!/bin/sh\nprintf '%s\\n' \"$MSYS2_ARG_CONV_EXCL\" \"$@\"\n", encoding="utf-8")
            for executable in root.iterdir(): executable.chmod(0o700)
            code = "import json; print('not a /c/path')"
            program = boundary + '\npython3 -c "$TEST_CODE" /c/Users/최형석/내\\ 팀/depts.json "\\\\\\\\.\\\\pipe\\\\cys-dept-dept-1"\n'
            result = subprocess.run([BASH, "-c", program], env={**os.environ, "PATH": str(root)+os.pathsep+os.environ.get("PATH", ""), "TEST_CODE":code},
                                    text=True, encoding="utf-8", capture_output=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.splitlines(), ["*", "-c", code, "C:/Users/최형석/내 팀/depts.json", r"\\.\pipe\cys-dept-dept-1"])


if __name__ == "__main__":
    unittest.main()
