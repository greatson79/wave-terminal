#!/usr/bin/env python3
"""Windows down-sock for a CRASHED department: clean up only after verifying it is gone (fail closed otherwise)."""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

DEPT = Path(__file__).resolve().parents[1] / "cys-dept"
BASH = os.environ.get("CYS_TEST_BASH", "bash")
SOCK = r"\\.\pipe\cys-dept-dept-1"


def run(pipe, alive_pids=(), recorded=None, windows=True):
    """Run the real down-sock verb; only the OS probes (pipe list, tasklist) and identity are stubbed."""
    with tempfile.TemporaryDirectory(prefix="dept-crash-") as tmp:
        home = Path(tmp); bin_dir = home / ".local/bin"; bin_dir.mkdir(parents=True)
        (bin_dir / "cys").write_text("#!/bin/sh\necho \"$@\" >> \"$HOME/cys-calls\"\nexit 1\n")  # identify fails
        (bin_dir / "uname").write_text("#!/bin/sh\nprintf 'MINGW64_NT\\n'\n")
        (bin_dir / "cygpath").write_text('#!/bin/sh\nprintf "%s\\n" "$2"\n')
        (bin_dir / "tasklist").write_text("#!/bin/sh\n" + "".join(
            'case "$*" in *"PID eq %d"*) printf \'"x.exe","%d"\\r\\n\';; esac\n' % (p, p) for p in alive_pids))
        for f in bin_dir.iterdir(): f.chmod(0o700)
        source = DEPT.read_text(encoding="utf-8")
        source = re.sub(r"^dept_pipe_present\(\)\{.*?^\}\n", "dept_pipe_present(){ printf '%s\\r\\n' " + pipe + "; }\n", source, flags=re.S | re.M)
        if not windows:
            (bin_dir / "uname").write_text("#!/bin/sh\nprintf 'Linux\\n'\n")
        script = home / "cys-dept"; script.write_text(source, encoding="utf-8")
        reg = home / ".cys/depts.json"; reg.parent.mkdir()
        entry = {"socket": SOCK}
        if recorded is not None: entry["pids"] = recorded
        reg.write_text(json.dumps({"depts": {"dept-1": entry}}))
        result = subprocess.run([BASH, str(script), "down-sock", SOCK], capture_output=True, text=True, encoding="utf-8",
                                stdin=subprocess.DEVNULL, timeout=20,
                                env={**os.environ, "HOME": str(home), "CYS_DEPTS_JSON": str(reg), "CYS_ROLE": "cso",
                                     "CYS_PACK_DIR": str(home / "pack"), "PATH": str(bin_dir) + os.pathsep + os.environ["PATH"]})
        return result, json.loads(reg.read_text())["depts"], (home / "cys-calls").read_text() if (home / "cys-calls").exists() else ""


class CrashedDeptTests(unittest.TestCase):
    def test_pipe_absent_and_no_live_pid_is_cleaned_up(self):
        r, depts, _ = run("absent", recorded=[4242])
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("부서가 이미 종료되어 있어 정리했습니다", r.stderr)
        self.assertEqual(depts, {})

    def test_pipe_absent_without_recorded_pids_is_cleaned_up(self):
        r, depts, _ = run("absent")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(depts, {})

    def test_pipe_answers_but_identity_unreadable_is_refused(self):
        r, depts, _ = run("present")
        self.assertEqual(r.returncode, 9)
        self.assertIn("팀 정보를 유지합니다", r.stderr)
        self.assertIn("dept-1", depts)

    def test_recorded_pid_still_alive_is_refused_even_if_pipe_is_gone(self):
        r, depts, _ = run("absent", alive_pids=[4242], recorded=[4242])
        self.assertEqual(r.returncode, 9)
        self.assertIn("dept-1", depts)

    def test_unreadable_recorded_pid_is_refused(self):
        r, depts, _ = run("absent", recorded=["oops"])
        self.assertEqual(r.returncode, 9)
        self.assertIn("dept-1", depts)

    def test_unix_path_unchanged(self):
        r, depts, _ = run("present", windows=False)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertNotIn("이미 종료", r.stderr)
        self.assertEqual(depts, {})


if __name__ == "__main__":
    unittest.main()
