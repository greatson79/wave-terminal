#!/usr/bin/env python3
"""Run real down/down-sock/rotate dispatch with only stop results stubbed; no real kill."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

DEPT = Path(__file__).resolve().parents[1] / "cys-dept"
BASH = os.environ.get("CYS_TEST_BASH", "bash")


class CallerTests(unittest.TestCase):
    def run_case(self, verb, stop_status, remove_status=None, windows=False, identify=True):
        with tempfile.TemporaryDirectory(prefix="dept-stop-최형석 ") as directory:
            home = Path(directory)
            bin_dir = home / ".local/bin"
            bin_dir.mkdir(parents=True)
            cys = bin_dir / "cys"
            cys.write_text("#!/bin/sh\ncase \"$1\" in\nidentify) " +
                ("printf '{\"daemon_pid\":101,\"started_at\":1,\"version\":\"0.3.1\"}\\n';;\n" if identify else "exit 1;;\n") +
                "ping) exit 0;;\ntombstone) printf tombstone >> \"$HOME/tombstone-called\";;\nesac\n", encoding="utf-8")
            cys.chmod(0o700)
            cysd = bin_dir / "cysd"
            cysd.write_text('#!/bin/sh\nprintf launched > "$HOME/daemon-launched"\nexit 99\n', encoding="utf-8")
            cysd.chmod(0o700)
            if windows:
                (bin_dir / "uname").write_text("#!/bin/sh\nprintf 'MINGW64_NT\\n'\n", encoding="utf-8")
                (bin_dir / "cygpath").write_text('#!/bin/sh\nprintf "%s\\n" "$2"\n', encoding="utf-8")
                for file in bin_dir.iterdir(): file.chmod(0o700)
            source = DEPT.read_text(encoding="utf-8")
            first = source.index("graceful_kill(){")
            last = source.index("\nreg_init(){", first)
            receipt = 'echo \'CYS_DEPT_STOP_RESULT {}\'; ' if stop_status == 0 else ''
            source = source[:first] + 'graceful_kill(){ echo stop-attempt >&2; ' + receipt + 'return %d; }\n' % stop_status + source[last:]
            if remove_status is not None:
                first = source.index("reg_remove(){")
                last = source.index("\nreg_set_meta(){", first)
                source = source[:first] + 'reg_remove(){ return %d; }\n' % remove_status + source[last:]
            if os.name == "nt":
                # Windows Python cannot execute the shell-script CLI stub as a native .exe.
                identity_stub = 'daemon_identity(){ printf "101\\t1\\n"; }\n' if identify else 'daemon_identity(){ return 1; }\n'
                source = source.replace('cmd="${1:?usage:', identity_stub + 'cmd="${1:?usage:', 1)
            source = source.replace('cmd="${1:?usage:', 'dept_classify_idle(){ echo IDLE; }\ncmd="${1:?usage:', 1)
            script = home / "cys-dept"
            script.write_text(source, encoding="utf-8")
            script.chmod(0o700)
            socket = str(home / ".local/state/cys-dept-dept-1/cys.sock")
            registry = home / ".cys/depts.json"
            registry.parent.mkdir()
            original = json.dumps({"depts":{"dept-1":{"socket":socket}}}).encode()
            registry.write_bytes(original)
            args = [BASH, str(script), verb, socket if verb == "down-sock" else "dept-1"]
            result = subprocess.run(args, env={**os.environ, "HOME":str(home), "CYS_ROLE":"cso",
                "CYS_PACK_DIR":str(home / "pack"), "CYS_DEPTS_JSON":str(registry),
                "PATH":str(bin_dir)+os.pathsep+os.environ.get("PATH", "")},
                stdin=subprocess.DEVNULL, capture_output=True, timeout=10)
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("종료·정리 완료".encode(), result.stdout)
            self.assertNotIn(b"socket teardown", result.stdout)
            self.assertNotIn(b"CYS_DEPT_STOP_RESULT ", result.stdout)
            self.assertEqual(registry.read_bytes(), original)
            self.assertFalse((home / "daemon-launched").exists())
            if remove_status is None:
                self.assertFalse((home / "tombstone-called").exists())
            return result

    def test_stop_failure_propagates_before_metadata_for_every_direct_caller(self):
        for verb in ["down", "down-sock", "rotate"]:
            with self.subTest(verb=verb):
                self.assertEqual(self.run_case(verb, 7).returncode, 7)

    def test_reaper_reports_child_teardown_failure_and_preserves_registry(self):
        self.assertEqual(self.run_case("reap", 7).returncode, 1)

    def test_registry_remove_failure_cannot_report_success(self):
        self.assertEqual(self.run_case("down-sock", 0, remove_status=8).returncode, 8)

    def test_windows_missing_identity_preserves_metadata(self):
        self.assertEqual(self.run_case("down-sock", 0, windows=True, identify=False).returncode, 9)


if __name__ == "__main__":
    unittest.main()
