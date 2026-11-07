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


def run(pipe, alive_pids=(), recorded=None, windows=True, scan="", proc_start=None):
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
        source = re.sub(r"^dept_scan_procs\(\)\{.*?^\}\n", "dept_scan_procs(){ printf '%s' '" + scan + "'; }\n", source, flags=re.S | re.M)
        if proc_start is not None:
            source = re.sub(r"^dept_proc_start\(\)\{.*?^\}\n", "dept_proc_start(){ printf '%s\\r\\n' " + proc_start + "; }\n", source, flags=re.S | re.M)
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

    def test_pidless_dept_with_live_runtime_key_process_is_refused_and_named(self):
        r, depts, _ = run("absent", scan="777\\tnode.exe\\n")
        self.assertEqual(r.returncode, 9)
        self.assertIn("node.exe", r.stderr)
        self.assertIn("팀 정보를 유지합니다", r.stderr)
        self.assertIn("dept-1", depts)

    def test_pidless_dept_with_unknown_scan_is_refused(self):
        r, depts, _ = run("absent", scan="unknown\\t")
        self.assertEqual(r.returncode, 9)
        self.assertIn("dept-1", depts)

    def test_scan_guard_is_only_for_pidless_depts(self):
        r, depts, _ = run("absent", recorded=[{"pid": 4242, "start": 1000}], proc_start="none", scan="777\\tnode.exe\\n")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(depts, {})

    def test_same_pid_same_start_is_ours_and_refused(self):
        r, depts, _ = run("absent", recorded=[{"pid": 4242, "start": 1000}], proc_start="1010")
        self.assertEqual(r.returncode, 9)
        self.assertIn("dept-1", depts)

    def test_reused_pid_with_different_start_is_not_ours(self):
        r, depts, _ = run("absent", recorded=[{"pid": 4242, "start": 1000}], proc_start="90000")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(depts, {})

    def test_unknown_process_start_is_refused(self):
        r, depts, _ = run("absent", recorded=[{"pid": 4242, "start": 1000}], proc_start="unknown")
        self.assertEqual(r.returncode, 9)
        self.assertIn("dept-1", depts)


class ScanBoundaryTests(unittest.TestCase):
    def scan(self, keys, rows):
        """Run the real dept_scan_procs python with a fake powershell.exe that prints CIM-style rows."""
        source = DEPT.read_text(encoding="utf-8")
        body = re.search(r"dept_scan_procs\(\)\{.*?<<'PY_SCAN'\n(.*?)^PY_SCAN\n", source, re.S | re.M).group(1)
        with tempfile.TemporaryDirectory() as tmp:
            bin_dir = Path(tmp)
            (bin_dir / "powershell.exe").write_text("#!/bin/sh\ncat <<'EOF'\n" + "".join(r + "\n" for r in rows) + "EOF\n")
            (bin_dir / "powershell.exe").chmod(0o700)
            r = subprocess.run(["python3", "-c", body] + keys, capture_output=True, text=True, encoding="utf-8", timeout=20,
                               env={**os.environ, "PATH": str(bin_dir) + os.pathsep + os.environ["PATH"]})
            self.assertEqual(r.returncode, 0, r.stderr)
            return r.stdout.splitlines()

    def test_dept_1_key_does_not_match_dept_10_but_matches_itself(self):
        rows = ["10\tnode.exe\tnode C:\\Users\\u\\AppData\\Local\\cys\\cys-dept-dept-10\\x.js",
                "11\tcysd.exe\tcysd \\\\.\\pipe\\cys-dept-dept-10",
                "12\tbash.exe\tbash C:\\Users\\u\\.cys\\pack-dept-dept-10\\run.sh",
                "20\tnode.exe\tnode C:\\Users\\u\\AppData\\Local\\cys\\cys-dept-dept-1\\x.js",
                "21\tcysd.exe\t\"C:\\x\\cysd.exe\" --pipe \\\\.\\pipe\\cys-dept-dept-1",
                "22\tbash.exe\tbash C:\\Users\\u\\.cys\\pack-dept-dept-1"]
        found = self.scan(["cys-dept-dept-1", "C:/Users/u/.cys/pack-dept-dept-1"], rows)
        self.assertEqual([line.split("\t")[0] for line in found], ["20", "21", "22"])

    def test_dept_10_key_does_not_match_dept_1(self):
        rows = ["20\tnode.exe\tnode C:\\a\\cys-dept-dept-1\\x.js"]
        self.assertEqual(self.scan(["cys-dept-dept-10"], rows), [])


class PidWriterTests(unittest.TestCase):
    def test_reg_set_pids_records_pid_and_start_and_warns_on_bad_identity(self):
        source = DEPT.read_text(encoding="utf-8")
        fn = re.search(r"^reg_set_pids\(\)\{.*?^\}\n", source, re.S | re.M).group(0)
        for identity, expect in [("4242\t1793966400", [{"pid": 4242, "start": 1793966400}]), ("", None)]:
            with self.subTest(identity=identity), tempfile.TemporaryDirectory() as tmp:
                reg = Path(tmp) / "depts.json"
                reg.write_text(json.dumps({"depts": {"dept-1": {"socket": "s"}}}))
                program = ("REG=" + str(reg) + "\nnative_path(){ printf '%s\\n' \"$1\"; }\nreg_init(){ :; }\n"
                           "daemon_identity(){ printf '%s\\r\\n' '" + identity + "'; }\n") + fn + "\nreg_set_pids dept-1 sock\n"
                r = subprocess.run([BASH, "-c", program], capture_output=True, text=True, encoding="utf-8", timeout=10)
                self.assertEqual(r.returncode, 0, r.stderr)
                self.assertEqual(json.loads(reg.read_text())["depts"]["dept-1"].get("pids"), expect)
                if expect is None: self.assertIn("pid 기록 실패", r.stderr)

    def test_every_spawn_site_records_pids_after_the_daemon_is_up(self):
        source = DEPT.read_text(encoding="utf-8")
        spawns = [m.start() for m in re.finditer(r'nohup "\$CYSD"', source)]
        self.assertEqual(len(spawns), 3)
        for at in spawns:
            tail = source[at:at + 1200]
            self.assertLess(tail.index('ready "$sock"'), tail.index('reg_set_pids "$name" "$sock"'))


if __name__ == "__main__":
    unittest.main()
