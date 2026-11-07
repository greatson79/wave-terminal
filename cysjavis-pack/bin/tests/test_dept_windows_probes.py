#!/usr/bin/env python3
"""Real-Windows exercise of the crashed-department probes (dept_proc_start ctypes, dept_scan_procs PowerShell).
They never ran on Windows before the proof; this runs them against real processes. Windows only (the helpers are Windows-only)."""
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
import unittest
import uuid

DEPT = Path(__file__).resolve().parents[1] / "cys-dept"
BASH = os.environ.get("CYS_TEST_BASH", "bash")


def call(function, *args):
    source = DEPT.read_text(encoding="utf-8")
    body = "\n".join(re.search(r"^%s\(\)\{.*?^\}\n" % name, source, re.S | re.M).group(0)
                     for name in ("dept_proc_start", "dept_scan_procs"))
    # CI runners expose python.exe; the shipped bundle provides python3.exe.
    program = 'command -v python3 >/dev/null 2>&1 || python3(){ command python "$@"; }\n' + body + "\n" + function + " " + " ".join(args) + "\n"
    with tempfile.TemporaryDirectory() as tmp:
        script = Path(tmp) / "probe.sh"
        script.write_bytes(program.encode("utf-8"))
        result = subprocess.run([BASH, str(script)], capture_output=True, text=True, encoding="utf-8",
                                stdin=subprocess.DEVNULL, timeout=90)
    return result.returncode, [line.rstrip("\r") for line in result.stdout.splitlines()], result.stderr


@unittest.skipUnless(os.name == "nt", "Windows-only helpers; executed in the Windows proof lib job")
class WindowsProbeTests(unittest.TestCase):
    def setUp(self):
        self.key = "cys-dept-probe-" + uuid.uuid4().hex[:12]
        self.child = subprocess.Popen([sys.executable, "-c", "import time;time.sleep(120)  # " + self.key],
                                      stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.addCleanup(self.child.kill)
        time.sleep(1.5)

    def test_proc_start_of_a_live_process_is_a_recent_epoch(self):
        rc, out, err = call("dept_proc_start", str(self.child.pid))
        self.assertEqual(rc, 0, err)
        self.assertEqual(len(out), 1, out)
        self.assertRegex(out[0], r"^\d+$", "got %r" % out)
        self.assertLess(abs(int(out[0]) - time.time()), 300)

    def test_proc_start_of_an_exited_process_is_none(self):
        quick = subprocess.Popen([sys.executable, "-c", "pass"]); quick.wait()
        time.sleep(0.5)
        rc, out, err = call("dept_proc_start", str(quick.pid))
        self.assertEqual((rc, out), (0, ["none"]), err)

    def test_scan_finds_a_process_by_runtime_key_and_ignores_unrelated_keys(self):
        rc, out, err = call("dept_scan_procs", self.key)
        self.assertEqual(rc, 0, err)
        self.assertIn(str(self.child.pid), [line.split("\t")[0] for line in out], "scan output: %r" % out)
        rc, out, err = call("dept_scan_procs", "cys-dept-probe-does-not-exist-" + uuid.uuid4().hex)
        self.assertEqual((rc, out), (0, []), err)


if __name__ == "__main__":
    unittest.main()
