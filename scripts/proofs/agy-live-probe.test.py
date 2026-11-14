#!/usr/bin/env python3
"""Local tests exercise only fake CLI / fixtures; never run a daemon or app."""
import importlib.util
import json
import os
import socket
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("agy_live", Path(__file__).with_name("agy-live-probe.py"))
probe = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe)


class OwnedProcess:
    def poll(self):
        return None


class ProofTests(unittest.TestCase):
    @unittest.skipUnless(os.name != "nt", "Unix socket fixture")
    def test_retention_copies_regular_and_hidden_files_but_skips_socket(self):
        with tempfile.TemporaryDirectory() as directory:
            source, destination = Path(directory) / "source", Path(directory) / "retained"
            (source / ".hidden").mkdir(parents=True)
            (source / "result.json").write_bytes(b'{"result":"PASS"}\n')
            (source / ".hidden/state").write_bytes(b"fixture state\x00\xff")
            subjects = {str(path.relative_to(source)): probe.sha256(path)
                        for path in source.rglob("*") if path.is_file()}
            probe.write_json(source / "SHA256.json", subjects)
            expected = dict(subjects, **{"SHA256.json": probe.sha256(source / "SHA256.json")})
            owned_socket = socket.socket(socket.AF_UNIX)
            try:
                owned_socket.bind(str(source / "daemon.sock"))
                probe.retain_evidence(source, destination)
                actual = {str(path.relative_to(destination)): probe.sha256(path)
                          for path in destination.rglob("*") if path.is_file()}
                self.assertEqual(actual, expected)
                self.assertFalse((destination / "daemon.sock").exists())
                self.assertTrue((destination / ".hidden/state").is_file())
                retained_manifest = json.loads((destination / "SHA256.json").read_text())
                for subject, digest in retained_manifest.items():
                    self.assertTrue((destination / subject).is_file())
                    self.assertEqual(probe.sha256(destination / subject), digest)
                with patch.object(probe.shutil, "copy2", side_effect=OSError("fixture copy failure")):
                    with self.assertRaises(OSError):
                        probe.retain_evidence(source, Path(directory) / "failed-copy")
            finally:
                owned_socket.close()

    def test_fake_cli_and_wrong_arguments(self):
        fake = Path(os.environ["AGY_TEST_FAKE"]).resolve(strict=True)
        with tempfile.TemporaryDirectory() as directory:
            counter = Path(directory) / "calls.jsonl"
            env = dict(os.environ, AGY_FAKE_COUNTER=str(counter), AGY_FAKE_MODE="normal")
            result = subprocess.run([str(fake), *probe.COLLECTOR_ARGS], env=env, cwd=directory, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0)
            self.assertEqual(json.loads(result.stdout)["status"], "SUCCESS")
            self.assertEqual(json.loads(result.stdout)["usage"]["total_tokens"], 0)
            self.assertEqual(json.loads(result.stdout)["response"].split("\t")[:3], ["Gemini Models", "Five Hour Limit Remaining", "75%"])
            self.assertIn("unauthorized", json.loads(result.stdout)["usage"]["note"])
            env["AGY_FAKE_MODE"] = "auth"
            result = subprocess.run([str(fake), *probe.COLLECTOR_ARGS], env=env, cwd=directory, capture_output=True, text=True)
            self.assertEqual(result.returncode, 1)
            self.assertIn("not logged in", result.stdout)
            result = subprocess.run([str(fake), "wrong"], env=env, cwd=directory, capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            calls = [json.loads(line) for line in counter.read_text().splitlines()]
            self.assertEqual([c["valid_args"] for c in calls], [True, True, False])
            self.assertEqual(calls[0]["argv"], probe.COLLECTOR_ARGS)

    def test_verdict_rejects_short_window_auth_retries_and_fast_cadence(self):
        with tempfile.TemporaryDirectory() as directory:
            cases = {}
            for name, times in [("L1", []), ("L2", [0]), ("L3", [0, 300, 600])]:
                counter = Path(directory) / name
                counter.write_text("".join(json.dumps({"time": t, "argv": probe.COLLECTOR_ARGS, "valid_args": True}) + "\n" for t in times))
                cases[name] = {"counter": counter, "process": OwnedProcess(), "rates": [100, 400, 700],
                               "control_seen": {"claude": {17, 19}, "codex": {23, 29}},
                               "control_current": {"claude": 19, "codex": 29}}
            self.assertEqual(probe.verdict(cases, 660)["result"], "PASS")
            self.assertEqual(probe.verdict(cases, 660)["controls"]["real logged-in agy"], "NOT_EXECUTED")
            cases["L1"]["counter"].unlink()
            with self.assertRaises(FileNotFoundError):
                probe.verdict(cases, 660)
            cases["L1"]["counter"].touch()
            self.assertEqual(probe.verdict(cases, 660)["result"], "PASS")
            self.assertEqual(probe.verdict(cases, 659)["result"], "FAIL")
            for name, case in cases.items():
                case.update(windows=True, config_method="no-agy-account-or-seat" if name == "L1" else "live-agy-metadata-seat")
            self.assertEqual(probe.verdict(cases, 660)["result"], "FAIL")
            self.assertEqual(probe.verdict(cases, 960)["result"], "PASS")
            cases["L2"]["config_method"] = "isolated-accounts-json"
            self.assertEqual(probe.verdict(cases, 960)["result"], "FAIL")
            for case in cases.values():
                case["windows"] = False
            with cases["L2"]["counter"].open("a") as log:
                log.write(json.dumps({"time": 300, "argv": probe.COLLECTOR_ARGS, "valid_args": True}) + "\n")
            self.assertEqual(probe.verdict(cases, 660)["result"], "FAIL")
            cases["L2"]["counter"].write_text(json.dumps({"time": 0, "argv": probe.COLLECTOR_ARGS, "valid_args": True}) + "\n")
            calls = [{"time": t, "argv": probe.COLLECTOR_ARGS, "valid_args": True} for t in [0, 1, 2]]
            cases["L3"]["counter"].write_text("".join(json.dumps(c) + "\n" for c in calls))
            self.assertEqual(probe.verdict(cases, 660)["result"], "FAIL")

    def test_l1_counter_must_exist_even_when_empty(self):
        with tempfile.TemporaryDirectory() as directory:
            counter = Path(directory) / "calls.jsonl"
            with self.assertRaises(FileNotFoundError):
                probe.events({"counter": counter})
            counter.touch()
            self.assertEqual(probe.events({"counter": counter}), [])

    def test_environment_arms_counter_before_launch(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fake = root / "fake"
            fake.write_bytes(b"fixture-only; never execute")
            env, target = probe.environment(root / "case", "fixture-only", fake, "normal")
            counter = Path(env["AGY_FAKE_COUNTER"])
            self.assertTrue(counter.is_file())
            self.assertEqual(counter.read_bytes(), b"")
            self.assertEqual(target.read_bytes(), fake.read_bytes())

    def test_teardown_requires_wait_and_independent_process_absence(self):
        process = unittest.mock.Mock()
        process.poll.return_value = 0
        absent = {"exit_code": 0, "observation_exit": 0, "stdout": "", "stderr": ""}
        with patch.object(probe, "process_evidence", return_value=absent):
            self.assertEqual(probe.verify_stopped(process), absent)
        for evidence in [dict(absent, stdout="123 daemon"), dict(absent, observation_exit=2),
                         dict(absent, exit_code=None), dict(absent, stderr="Get-CimInstance: Access is denied")]:
            with patch.object(probe, "process_evidence", return_value=evidence):
                with self.assertRaises(RuntimeError):
                    probe.verify_stopped(process)

    def test_final_control_disappearance_or_reversion_is_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            cases = {}
            for name, times in [("L1", []), ("L2", [0]), ("L3", [0, 300, 600])]:
                counter = Path(directory) / name
                counter.write_text("".join(json.dumps({"time": t, "argv": probe.COLLECTOR_ARGS, "valid_args": True}) + "\n" for t in times))
                cases[name] = {"counter": counter, "process": OwnedProcess(), "rates": [100, 400, 700],
                               "control_seen": {"claude": {17, 19}, "codex": {23, 29}},
                               "control_current": {"claude": 19, "codex": 29}}
            def rows(claude, codex):
                return [{"provider": provider, "account_id": account, "source": source,
                         "rate": [{"label": "5h", "used_pct": percent}]}
                        for provider, account, source, percent in [
                            ("claude", "proof-claude", "statusline", claude),
                            ("codex", "default", "rollout", codex)] if percent is not None]
            probe.observe_controls(cases["L3"], rows(19, 29))
            self.assertEqual(probe.verdict(cases, 660)["result"], "PASS")
            for claude, codex in [(None, 29), (19, None), (17, 29), (19, 23)]:
                probe.observe_controls(cases["L3"], rows(claude, codex))
                self.assertEqual(probe.verdict(cases, 660)["result"], "FAIL", (claude, codex))
            probe.observe_controls(cases["L3"], rows(19, 29))
            self.assertEqual(probe.verdict(cases, 660)["result"], "PASS")

    def test_windows_eligibility_uses_live_seats_and_preserves_cleanup_ids(self):
        with tempfile.TemporaryDirectory() as directory:
            cases, requests = {}, []
            for name in ["L1", "L2", "L3"]:
                folder = Path(directory) / name
                folder.mkdir()
                case = {"name": name, "windows": True, "directory": folder,
                        "endpoint": "fixture-only", "duration_seconds": 960, "surfaces": [],
                        "fake": folder / "fake", "counter": folder / "calls.jsonl"}
                case["fake"].write_bytes(b"fixture-only")
                probe.declare_account(case)
                self.assertFalse((folder / "home/.cys/accounts.json").exists())
                cases[name] = case
            def fake_rpc(endpoint, method, params=None):
                requests.append((method, params))
                return {"surface_id": len(requests)} if method == "surface.create" else {}
            with patch.object(probe, "rpc", side_effect=fake_rpc):
                for case in cases.values():
                    probe.configure_ready_case(case)
                original_agy_sid = cases["L3"]["surfaces"][0]
                probe.prepare_controls(cases["L3"])
            self.assertEqual(cases["L1"]["surfaces"], [])
            self.assertEqual(cases["L2"]["config_method"], "live-agy-metadata-seat")
            self.assertEqual(cases["L3"]["surfaces"][0], original_agy_sid)
            self.assertEqual(len(cases["L3"]["surfaces"]), 3)
            self.assertTrue(all(params["cmd"] == "Start-Sleep -Seconds 1140"
                                for method, params in requests if method == "surface.create"))
            self.assertEqual([params["agent"] for method, params in requests if method == "surface.set_meta"],
                             ["agy", "agy", "claude", "codex"])

    def test_windows_knownfolder_inventory_is_read_only_and_fails_closed(self):
        clean = {"profile": r"C:\Users\runneradmin", "accounts_file": r"C:\Users\runneradmin\.cys\accounts.json",
                 "exists": False, "providers": [], "custom_command_adapters": 0, "sha256": None}
        reply = subprocess.CompletedProcess([], 0, stdout=json.dumps(clean), stderr="")
        with patch.object(probe.subprocess, "run", return_value=reply) as invoke:
            self.assertEqual(probe.windows_profile_accounts(), clean)
        command = invoke.call_args.args[0][-1]
        self.assertIn("[Environment]::GetFolderPath('UserProfile')", command)
        self.assertIn("Get-Content -LiteralPath", command)
        self.assertNotIn("Set-Content", command)
        probe.require_no_agy_account(clean)
        with self.assertRaises(RuntimeError):
            probe.require_no_agy_account(dict(clean, custom_command_adapters=1))
        for provider in ["agy", "gemini", "antigravity", "AGY"]:
            with self.assertRaises(RuntimeError):
                probe.require_no_agy_account(dict(clean, providers=[provider]))
        with patch.object(probe.subprocess, "run", side_effect=subprocess.CalledProcessError(1, [])):
            with self.assertRaises(subprocess.CalledProcessError):
                probe.windows_profile_accounts()

    def test_control_fixtures_are_metadata_and_local_files_only(self):
        with tempfile.TemporaryDirectory() as directory:
            case = {"directory": Path(directory), "endpoint": "fixture-only"}
            requests = []
            def fake_rpc(endpoint, method, params=None):
                requests.append((method, params))
                return {"surface_id": len(requests)} if method == "surface.create" else {}
            with patch.object(probe, "rpc", side_effect=fake_rpc):
                probe.prepare_controls(case)
            self.assertEqual(len(case["surfaces"]), 2)
            self.assertEqual(json.loads(case["codex_transcript"].read_text())["payload"]["rate_limits"]["primary"]["used_percent"], 23)
            self.assertEqual([params["agent_bin"] for method, params in requests if method == "surface.set_meta"],
                             ["proof-no-real-claude", "proof-no-real-codex"])


if __name__ == "__main__":
    unittest.main()
