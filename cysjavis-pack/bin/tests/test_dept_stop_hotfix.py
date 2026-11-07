#!/usr/bin/env python3
"""Run the real stop state machine with a fake process table, never a real kill."""
from pathlib import Path
import unittest
import os
import json
import subprocess
import io
import contextlib
import textwrap

DEPT = Path(__file__).resolve().parents[1] / "cys-dept"


def load_stop():
    source = DEPT.read_text(encoding="utf-8")
    assert "<<'PY_STOP'\n" in source, "bounded teardown state machine is missing"
    code = source.split("<<'PY_STOP'\n", 1)[1].split("\nPY_STOP", 1)[0]
    namespace = {"__name__": "test_dept_stop"}
    exec(compile(code, str(DEPT), "exec"), namespace)
    return namespace


class Backend:
    def __init__(self, soft=False, force=True, reused=False, spawn=False):
        self.live = {101, 102}
        self.soft, self.force, self.reused, self.spawn = soft, force, reused, spawn
        self.calls, self.closed = [], False
        self.time = 0.0
    def capture(self, pid):
        return {101:{"pid":101}, 102:{"pid":102}}
    def refresh(self, known):
        if self.spawn and self.calls:
            known[103] = {"pid":103}
            self.live.add(103)
            self.spawn = False
        if self.reused and self.calls:
            self.live.discard(101)
    def alive(self, record):
        return record["pid"] in self.live
    def current(self, record):
        return self.alive(record) and not (self.reused and record["pid"] == 101 and self.calls)
    def kill(self, record, force, timeout):
        assert 0 < timeout <= 2
        self.calls.append((record["pid"], force))
        if (force and self.force) or (not force and self.soft):
            if not force: self.live.clear()
            else: self.live.discard(record["pid"])
    def close(self, known):
        self.closed = True
    def clock(self): return self.time
    def pause(self, amount): self.time += amount


class StopTests(unittest.TestCase):
    def test_windows_receipt_uses_captured_creation_time_not_identify_time(self):
        ns = load_stop()
        class Processes:
            root_started = 123
            def __init__(self, started, image):
                self.started = started
        ns["WindowsProcesses"] = Processes
        ns["stop_tree"] = lambda backend, pid: 2
        source = DEPT.read_text(encoding="utf-8").split("<<'PY_STOP'\n", 1)[1].split("\nPY_STOP", 1)[0]
        main = textwrap.dedent(source.split('if __name__ == "__main__":\n', 1)[1])
        original = ns["sys"].argv
        output = io.StringIO()
        try:
            ns["sys"].argv = ["test", "101", "999", "cysd.exe"]
            with contextlib.redirect_stdout(output):
                exec(compile(main, str(DEPT), "exec"), ns)
        finally:
            ns["sys"].argv = original
        receipt = json.loads(output.getvalue().removeprefix("CYS_DEPT_STOP_RESULT "))
        self.assertEqual(receipt, {"schema_version":1, "verified":True, "daemon_pid":101,
                         "daemon_start_time":123, "scope":"known-process-tree", "descendants":2})

    def test_unix_receipt_binds_process_start_without_real_signals(self):
        source = DEPT.read_text(encoding="utf-8")
        function = source.split("graceful_kill(){", 1)[1].split("\nreg_init(){", 1)[0]
        program = "graceful_kill(){" + function + '''
dept_windows=0
ps(){ printf 'Fri Nov  6 12:00:00 2026\\n'; }
kill(){ return 1; }
sleep(){ :; }
graceful_kill 101
'''
        result = subprocess.run([os.environ.get("CYS_TEST_BASH", "bash"), "-c", program],
                                env={**os.environ, "TZ":"UTC"}, capture_output=True, text=True,
                                encoding="utf-8", timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(result.stdout.removeprefix("CYS_DEPT_STOP_RESULT "))
        self.assertEqual(receipt, {"schema_version":1, "verified":True, "daemon_pid":101,
                         "daemon_start_time":1793966400, "scope":"known-process-tree", "descendants":0})

    def test_native_kill_targets_only_the_verified_pid(self):
        ns = load_stop()
        backend = ns["WindowsProcesses"].__new__(ns["WindowsProcesses"])
        backend.taskkill = "taskkill.exe"
        backend.current = lambda record: True
        commands = []
        original = ns["subprocess"].run
        def run(command, **kwargs):
            commands.append(command)
            return type("Result", (), {"returncode": 0})()
        ns["subprocess"].run = run
        try:
            backend.kill({"pid":101}, False, 1)
            backend.kill({"pid":101}, True, 1)
        finally:
            ns["subprocess"].run = original
        self.assertEqual(commands, [["taskkill.exe", "/PID", "101"],
                                    ["taskkill.exe", "/PID", "101", "/F"]])

    def test_native_refresh_excludes_child_created_after_its_original_parent_exited(self):
        ns = load_stop()
        backend = ns["WindowsProcesses"].__new__(ns["WindowsProcesses"])
        class Kernel:
            def __init__(self): self.closed = []
            def GetProcessTimes(self, handle, created, exited, kernel, user):
                exited._obj.dwLowDateTime = 20
                return True
            def CloseHandle(self, handle): self.closed.append(handle)
        backend.kernel = Kernel()
        backend.rows = lambda: ([(102, 101)], 40)
        backend.open = lambda pid: {"pid":pid, "created":30, "handle":1020}
        known = {101:{"pid":101, "created":10, "handle":1010}}
        backend.refresh(known)
        self.assertEqual(list(known), [101])
        self.assertEqual(backend.kernel.closed, [1020])

    def test_native_capture_refuses_reused_root_identity_and_foreign_executable(self):
        ns = load_stop()
        for created, image in [(116444736020000000, "cysd.exe"), (116444736005000000, "foreign.exe")]:
            with self.subTest(created=created, image=image):
                backend = ns["WindowsProcesses"].__new__(ns["WindowsProcesses"])
                class Kernel:
                    def __init__(self): self.closed = []
                    def QueryFullProcessImageNameW(self, handle, flags, buffer, size):
                        buffer.value = os.path.abspath(image)
                        return True
                    def CloseHandle(self, handle): self.closed.append(handle)
                backend.kernel, backend.started_at = Kernel(), 1.0
                backend.image = os.path.normcase(os.path.abspath("cysd.exe"))
                backend.open = lambda pid: {"pid":pid, "created":created, "handle":55}
                with self.assertRaises(ns["StopFailure"]): backend.capture(101)
                self.assertEqual(backend.kernel.closed, [55])

    def test_native_capture_keeps_verified_identity_handle_until_proof_finishes(self):
        ns = load_stop()
        backend = ns["WindowsProcesses"].__new__(ns["WindowsProcesses"])
        class Kernel:
            def __init__(self): self.closed = []
            def QueryFullProcessImageNameW(self, handle, flags, buffer, size):
                buffer.value = os.path.abspath("cysd.exe")
                return True
            def CloseHandle(self, handle): self.closed.append(handle)
        backend.kernel, backend.started_at = Kernel(), 1.0
        backend.image = os.path.normcase(os.path.abspath("cysd.exe"))
        backend.open = lambda pid: {"pid":pid, "created":116444736005000000, "handle":55}
        known = backend.capture(101)
        self.assertEqual(backend.kernel.closed, [])
        backend.close(known)
        self.assertEqual(backend.kernel.closed, [55])

    def test_graceful_success_never_forces(self):
        ns = load_stop()
        backend = Backend(soft=True)
        self.assertEqual(ns["stop_tree"](backend, 101, backend.clock, backend.pause), 1)
        self.assertEqual(backend.calls, [(101, False)])
        self.assertTrue(backend.closed)

    def test_refused_graceful_force_proves_daemon_and_descendants(self):
        ns = load_stop()
        backend = Backend(spawn=True)
        self.assertEqual(ns["stop_tree"](backend, 101, backend.clock, backend.pause), 2)
        self.assertIn((103, True), backend.calls)
        self.assertIn((102, True), backend.calls)
        self.assertIn((101, True), backend.calls)
        self.assertFalse(backend.live)
        self.assertTrue(backend.closed)

    def test_survivor_timeout_is_failure_not_success(self):
        ns = load_stop()
        backend = Backend(force=False)
        with self.assertRaises(ns["StopFailure"]):
            ns["stop_tree"](backend, 101, backend.clock, backend.pause)
        self.assertLess(backend.time, 18.2)
        self.assertEqual([call for call in backend.calls if call[1]], [(102, True), (101, True)], "force each captured identity at most once")
        self.assertTrue(backend.closed)

    def test_reused_pid_is_not_forced(self):
        ns = load_stop()
        backend = Backend(reused=True)
        ns["stop_tree"](backend, 101, backend.clock, backend.pause)
        self.assertNotIn((101, True), backend.calls)
        self.assertIn((102, True), backend.calls)


if __name__ == "__main__":
    unittest.main()
