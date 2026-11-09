#!/usr/bin/env python3
"""Dept stop identity (WINFIX, Theo decision (a)): the stop compares the daemon image with the identity RECORDED at launch.
Shell level with a simulated-Windows bash + a python3 stub that captures what the real stop is handed (never a real kill),
plus a real unix child for the pre-kill image check, and real stub-daemon launches. CYS_DEPT_UNDER_TEST=<file> reruns
the suite against another cys-dept (used to show T1 RED on the pre-fix file)."""
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

DEPT = Path(os.environ.get("CYS_DEPT_UNDER_TEST") or Path(__file__).resolve().parents[1] / "cys-dept")
BASH = os.environ.get("CYS_TEST_BASH", "bash")
REAL_PY = sys.executable

# python3 stub: the PY_STOP program is recorded (argv -> file) instead of run; every other python3 call runs for real.
STUB = '''#!%s
import json, os, subprocess, sys
src = sys.stdin.read() if sys.argv[1:2] == ["-"] else None
if src is not None and "class WindowsProcesses" in src:
    open(os.environ["STOP_ARGV"], "w").write(json.dumps(sys.argv[2:]))
    print('CYS_DEPT_STOP_RESULT {"schema_version":1,"verified":true,"daemon_pid":%%d,"daemon_start_time":1,"scope":"known-process-tree","descendants":0}' %% int(sys.argv[2]))
    sys.exit(0)
sys.exit(subprocess.run([%r] + sys.argv[1:], input=src.encode() if src is not None else None).returncode)
''' % (REAL_PY, REAL_PY)


def head(source):
    return source.split('cmd="${1:?usage:', 1)[0]


class Env:
    """A throw-away HOME with a registry, a bin dir (stub uname/cygpath/python3) and a stub waved."""
    def __init__(self, tmp, record, windows=True, with_waved=True):
        self.home = Path(tmp)
        self.bin = self.home / "Wave Terminal"
        self.bin.mkdir(parents=True)
        self.registry = self.home / "depts.json"
        entry = {"socket": "s"}
        if record is not None: entry["pids"] = [record]
        self.registry.write_text(json.dumps({"depts": {"dept-1": entry}}))
        self.argv = self.home / "stop_argv.json"
        stubs = self.home / ".local" / "bin"; stubs.mkdir(parents=True)  # cys-dept prepends this dir to PATH
        if windows:
            (stubs / "uname").write_text("#!/bin/sh\nprintf 'MINGW64_NT\\n'\n")
            (stubs / "cygpath").write_text('#!/bin/sh\nprintf "%s\\n" "$2"\n')
            (stubs / "python3").write_text(STUB)
        for f in stubs.iterdir(): f.chmod(0o700)
        if with_waved:
            (self.bin / "waved").write_text("#!/bin/sh\nexit 0\n"); (self.bin / "waved").chmod(0o700)
        self.stubs = stubs

    def run(self, body, windows=True):
        script = self.home / "t.sh"
        script.write_text(head(DEPT.read_text(encoding="utf-8")) + "\n" + body + "\n")
        path = (str(self.stubs) + os.pathsep if windows else "") + str(self.bin) + os.pathsep + os.environ["PATH"]
        return subprocess.run([BASH, str(script)], env={**os.environ, "HOME": str(self.home), "PATH": path,
            "CYS_DEPTS_JSON": str(self.registry), "STOP_ARGV": str(self.argv), "CYS_PACK_DIR": str(self.home / "pack")},
            capture_output=True, text=True, timeout=30, stdin=subprocess.DEVNULL)

    def stop_images(self, pid=101, started="1794189098.5"):
        r = self.run('graceful_kill %s %s dept-1' % (pid, started))
        return r, (json.loads(self.argv.read_text())[2] if self.argv.exists() else None)


@unittest.skipIf(os.name == "nt", "POSIX shell/process harness; the Windows stop path is proven by the Windows proof probe (real down)")
class WindowsStopArgs(unittest.TestCase):
    def test_T1_032_dept_stop_is_handed_the_recorded_waved_not_the_cysd_forwarder(self):
        with tempfile.TemporaryDirectory(prefix="stop-최형석 ") as tmp:
            e = Env(tmp, {"pid": 101, "start": 1794189098, "image": "C:/Wave Terminal/waved.exe"})
            r, images = e.stop_images()
            self.assertEqual(r.returncode, 0, r.stderr)
            allowed = images.split("|")
            self.assertEqual(allowed[0], "C:/Wave Terminal/waved.exe")
            self.assertFalse(any(os.path.basename(a).lower().startswith("cysd") for a in allowed), allowed)

    def test_T3_recorded_032_dept_never_falls_back_to_either_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            e = Env(tmp, {"pid": 101, "start": 1794189098, "image": "C:/W/waved.exe"})
            _, images = e.stop_images()
            self.assertEqual(set(images.split("|")), {"C:/W/waved.exe", "C:/W/waved.prev.exe", "C:/W/waved.prev2.exe", "C:/W/waved.prev3.exe"})

    def test_F2_prev_names_only_in_the_recorded_dir(self):
        with tempfile.TemporaryDirectory() as tmp:
            e = Env(tmp, {"pid": 101, "start": 1794189098, "image": "C:/W/waved.exe"})
            _, images = e.stop_images()
            for other in ("C:/Other/waved.prev.exe", "C:/W/sub/waved.prev.exe", "C:/W/waved.prev4.exe", "C:/W/cysd.prev.exe"):
                self.assertNotIn(other, images.split("|"))
            self.assertIn("C:/W/waved.prev2.exe", images.split("|"))

    def test_T2_031_dept_without_any_record_gets_exactly_the_legacy_cysd_names_and_never_waved(self):
        with tempfile.TemporaryDirectory() as tmp:
            e = Env(tmp, None)
            _, images = e.stop_images()
            names = [os.path.basename(a) for a in images.split("|")]
            self.assertEqual(names, ["cysd", "cysd.exe", "cysd.prev.exe", "cysd.prev2.exe", "cysd.prev3.exe"])
            self.assertTrue(all(os.path.dirname(a) == str(e.bin) for a in images.split("|")))

    def test_T5_legacy_record_without_image_never_accepts_the_new_binary(self):
        with tempfile.TemporaryDirectory() as tmp:
            e = Env(tmp, {"pid": 101, "start": 1794189098})
            _, images = e.stop_images()
            self.assertNotIn("waved", " ".join(os.path.basename(a) for a in images.split("|")))
            self.assertIn("cysd.exe", [os.path.basename(a) for a in images.split("|")])

    def test_record_for_another_pid_or_start_is_refused_with_an_empty_allow_list(self):
        for pid, started in ((101, "1794189999"), (202, "1794189098.5")):
            with self.subTest(pid=pid), tempfile.TemporaryDirectory() as tmp:
                e = Env(tmp, {"pid": 101, "start": 1794189098, "image": "C:/W/waved.exe"})
                _, images = e.stop_images(pid=pid, started=started)
                self.assertEqual(images, "")

    def test_unregistered_dept_name_gets_an_empty_allow_list(self):
        with tempfile.TemporaryDirectory() as tmp:
            e = Env(tmp, None)
            r = e.run('graceful_kill 101 1 ""')
            self.assertEqual(json.loads(e.argv.read_text())[2], "")


@unittest.skipIf(os.name == "nt", "POSIX shell/process harness; the Windows stop path is proven by the Windows proof probe (real down)")
class UnixPreKillImageCheck(unittest.TestCase):
    """T6: real child process; the stop refuses BEFORE kill when its image is not the recorded one."""
    @staticmethod
    def alive(pid):
        try: os.kill(pid, 0); return True
        except OSError: return False

    def run_stop(self, record_image):
        # Orphaned (re-parented) sleep: a real stop can reap-observe it; the test never signals anything else.
        pid = int(subprocess.run(["sh", "-c", "sleep 60 </dev/null >/dev/null 2>&1 & echo $!"], capture_output=True, text=True).stdout)
        self.addCleanup(lambda: self.alive(pid) and os.kill(pid, signal.SIGKILL))
        time.sleep(0.3)
        with tempfile.TemporaryDirectory() as tmp:
            e = Env(tmp, None, windows=False)
            if record_image == "LIVE":
                record_image = e.run('dept_live_image %d' % pid, windows=False).stdout.strip()
                self.assertTrue(record_image, "dept_live_image returned nothing")
            now = int(time.time())
            rec = {"pid": pid, "start": now}
            if record_image: rec["image"] = record_image
            e.registry.write_text(json.dumps({"depts": {"dept-1": {"socket": "s", "pids": [rec]}}}))
            r = e.run('graceful_kill %d %d dept-1' % (pid, now), windows=False)
        return pid, r

    def test_swapped_image_is_refused_and_nothing_is_killed(self):
        pid, r = self.run_stop("/usr/bin/some-other-daemon")
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("부서 프로세스 정보가 바뀌었습니다. 다른 프로세스는 종료하지 않습니다.", r.stderr)
        self.assertTrue(self.alive(pid), "refused stop must not kill")

    def test_recorded_image_is_stopped(self):
        pid, r = self.run_stop("LIVE")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("CYS_DEPT_STOP_RESULT", r.stdout)
        self.assertFalse(self.alive(pid))

    def test_unrecorded_foreign_process_is_refused(self):
        pid, r = self.run_stop("")  # record without image -> legacy cysd names only; `sleep` is not one
        self.assertNotEqual(r.returncode, 0)
        self.assertTrue(self.alive(pid))


@unittest.skipIf(os.name == "nt", "POSIX shell/process harness; the Windows stop path is proven by the Windows proof probe (real down)")
class RecordAndLaunch(unittest.TestCase):
    def test_T8_reg_set_pids_accepts_the_float_started_at_and_records_the_image(self):
        with tempfile.TemporaryDirectory() as tmp:
            e = Env(tmp, None, windows=False)
            r = e.run('daemon_identity(){ printf "%s\\t%s\\n" ' + str(os.getpid()) + ' 1794189098.492273; }\n'
                      'reg_set_pids dept-1 sock')
            self.assertEqual(r.returncode, 0, r.stderr)
            rec = json.loads(e.registry.read_text())["depts"]["dept-1"]["pids"][0]
            self.assertEqual((rec["pid"], rec["start"]), (os.getpid(), 1794189098))
            self.assertTrue(rec.get("image"), rec)
            r = e.run('daemon_identity(){ printf "12\\tx.y\\n"; }\nreg_set_pids dept-1 sock')
            self.assertIn("pid 기록 실패", r.stderr)  # non-numeric start still refused, never recorded

    def test_T7_every_spawn_runs_the_real_daemon_not_the_forwarder(self):
        src = DEPT.read_text(encoding="utf-8")
        self.assertEqual(src.count('nohup "$WAVED"'), 3)
        self.assertNotIn('nohup "$CYSD"', src)
        # Resolution order only (the host may have a real install): run the WAVED= line with the homebrew fallback pointed nowhere.
        line = [l for l in src.splitlines() if l.startswith('WAVED="$(command -v waved')][0].replace("/opt/homebrew/bin/waved", "/nonexistent/waved")
        with tempfile.TemporaryDirectory() as tmp:
            e = Env(tmp, None, windows=False, with_waved=False)
            def resolve():
                return subprocess.run([BASH, "-c", 'CYSD=/legacy/cysd\n' + line + '\nprintf %s "$WAVED"'], capture_output=True, text=True,
                                      env={**os.environ, "PATH": str(e.bin) + os.pathsep + "/usr/bin:/bin"}).stdout
            self.assertEqual(resolve(), "/legacy/cysd", "old 0.3.1 install (no waved) falls back to its real cysd")
            (e.bin / "waved").write_text("#!/bin/sh\n"); (e.bin / "waved").chmod(0o700)
            self.assertEqual(resolve(), str(e.bin / "waved"))


WAVED_STUB = """#!%s
import os, sys, time
home = os.environ["HOME"]
open(home + "/daemon.pid", "w").write(str(os.getpid()))
time.sleep(float(os.environ.get("STUB_READY_DELAY", "0")))
open(home + "/daemon.up", "w").write("1")
time.sleep(120)
""" % REAL_PY
CYS_STUB = """#!/bin/sh
case "$1" in
ping) [ -f "$HOME/daemon.up" ] && exit 0 || exit 1;;
identify) printf '{"daemon_pid":%s,"started_at":%s.25,"version":"0.3.2"}\\n' "$(cat "$HOME/daemon.pid")" "$(date +%s)";;
status) echo '{"surfaces":[]}';;
*) exit 0;;
esac
"""


@unittest.skipIf(os.name == "nt", "POSIX shell/process harness; the Windows stop path is proven by the Windows proof probe (real down)")
class Lifecycle(unittest.TestCase):
    """Real launch/down with a stub daemon: F3 — no orphan daemon whether the launcher dies mid-run or the dept is torn down."""
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="dl-", dir="/tmp")  # short: unix socket paths are limited to ~103 bytes
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name)
        b = self.home / ".local" / "bin"; b.mkdir(parents=True)
        for name, body in (("waved", WAVED_STUB), ("cysd", WAVED_STUB), ("cys", CYS_STUB)):
            (b / name).write_text(body); (b / name).chmod(0o700)
        self.env = {**os.environ, "HOME": str(self.home), "CYS_DEPTS_JSON": str(self.home / "depts.json"),
                    "CYS_PACK_DIR": str(self.home / "pack"), "PATH": str(b) + os.pathsep + os.environ["PATH"]}
        self.env.pop("CYS_ROLE", None)

    @staticmethod
    def alive(pid):
        try: os.kill(pid, 0); return True
        except OSError: return False

    def daemon_pid(self, wait=5.0):
        end = time.time() + wait
        f = self.home / "daemon.pid"
        while time.time() < end:
            if f.exists() and f.read_text().strip(): return int(f.read_text())
            time.sleep(0.05)
        self.fail("stub daemon never started")

    def cleanup_pid(self, pid):
        self.addCleanup(lambda: self.alive(pid) and os.kill(pid, signal.SIGKILL))

    def test_F3_launcher_killed_mid_run_leaves_no_orphan_daemon(self):
        launcher = subprocess.Popen([BASH, str(DEPT), "launch", "dept-x"], env={**self.env, "STUB_READY_DELAY": "4"},
                                    stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        pid = self.daemon_pid(wait=40); self.cleanup_pid(pid)   # the launcher preamble takes a few seconds before it spawns
        launcher.send_signal(signal.SIGTERM)   # the launcher dies before the daemon is ready / recorded
        launcher.wait(timeout=10)
        time.sleep(1.0)
        self.assertFalse(self.alive(pid), "daemon survived the death of its launcher (orphan)")

    def test_F3_down_after_launch_leaves_no_daemon_and_registry_empty(self):
        r = subprocess.run([BASH, str(DEPT), "launch", "dept-x"], env=self.env, capture_output=True, text=True, timeout=60, stdin=subprocess.DEVNULL)
        self.assertEqual(r.returncode, 0, r.stderr)
        pid = self.daemon_pid(); self.cleanup_pid(pid)
        rec = json.loads((self.home / "depts.json").read_text())["depts"]["dept-x"]["pids"][0]
        self.assertEqual(rec["pid"], pid); self.assertTrue(rec.get("image"))
        self.assertTrue(self.alive(pid))
        r = subprocess.run([BASH, str(DEPT), "down", "dept-x"], env=self.env, capture_output=True, text=True, timeout=60, stdin=subprocess.DEVNULL)
        self.assertEqual(r.returncode, 0, r.stderr)
        time.sleep(0.5)
        self.assertFalse(self.alive(pid), "daemon survived down")
        self.assertEqual(json.loads((self.home / "depts.json").read_text())["depts"], {})

    def test_down_refuses_a_daemon_whose_image_is_not_the_recorded_one(self):
        r = subprocess.run([BASH, str(DEPT), "launch", "dept-x"], env=self.env, capture_output=True, text=True, timeout=60, stdin=subprocess.DEVNULL)
        self.assertEqual(r.returncode, 0, r.stderr)
        pid = self.daemon_pid(); self.cleanup_pid(pid)
        reg = self.home / "depts.json"; d = json.loads(reg.read_text())
        d["depts"]["dept-x"]["pids"][0]["image"] = "/usr/bin/not-this-daemon"; reg.write_text(json.dumps(d))
        r = subprocess.run([BASH, str(DEPT), "down", "dept-x"], env=self.env, capture_output=True, text=True, timeout=60, stdin=subprocess.DEVNULL)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("다른 프로세스는 종료하지 않습니다", r.stderr)
        self.assertTrue(self.alive(pid))
        self.assertIn("dept-x", json.loads(reg.read_text())["depts"], "registry must stay when the stop is refused")


class CaptureSetCompare(unittest.TestCase):
    """T4 at the real capture(): the live image must be IN the allowed set — same name in another dir, foreign exe, or a
    process created after the recorded start are all refused (closes the handle, nothing killed)."""
    def capture(self, live, allowed, created=116444736005000000):
        src = DEPT.read_text(encoding="utf-8").split("<<'PY_STOP'\n", 1)[1].split("\nPY_STOP", 1)[0]
        ns = {"__name__": "t"}; exec(compile(src, str(DEPT), "exec"), ns)
        b = ns["WindowsProcesses"].__new__(ns["WindowsProcesses"])
        closed = []
        class K:
            def QueryFullProcessImageNameW(self, handle, flags, buf, size): buf.value = live; return True
            def CloseHandle(self, h): closed.append(h)
        b.kernel, b.started_at = K(), 1.0
        b.images = {os.path.normcase(os.path.abspath(a)) for a in allowed}
        b.open = lambda pid: {"pid": pid, "created": created, "handle": 55}
        try: b.capture(101); return True, closed
        except ns["StopFailure"]: return False, closed

    def test_T4_set_membership_is_strict(self):
        root = os.path.abspath("W")
        allowed = [os.path.join(root, "waved.exe")]
        self.assertTrue(self.capture(os.path.join(root, "waved.exe"), allowed)[0])
        for live in (os.path.join(os.path.abspath("Other"), "waved.exe"), os.path.join(root, "cysd.exe"), os.path.join(root, "waved.prev.exe")):
            with self.subTest(live=live):
                ok, closed = self.capture(live, allowed)
                self.assertFalse(ok); self.assertEqual(closed, [55])
        self.assertFalse(self.capture(os.path.join(root, "waved.exe"), allowed, created=116444736020000000)[0], "newer process (pid reuse)")
        self.assertFalse(self.capture(os.path.join(root, "waved.exe"), [])[0], "empty allow list refuses everything")


if __name__ == "__main__":
    unittest.main()
