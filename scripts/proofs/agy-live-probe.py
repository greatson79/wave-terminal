#!/usr/bin/env python3
"""Eleven-minute fake-agy proof against an installed candidate sidecar, CI only."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time
import uuid

CANDIDATE_SHA = "c80e2dfb838f9823d5bb413f8cfb8db6dcaa5b53"

COLLECTOR_ARGS = ["-p", "/usage", "--output-format", "json", "--print-timeout", "10s"]


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def rpc_once(endpoint, method, params=None):
    request = json.dumps({"id": 1, "method": method, "params": params or {}}).encode() + b"\n"
    if os.name == "nt":
        stream = open(endpoint, "r+b", buffering=0)
    else:
        connection = socket.socket(socket.AF_UNIX)
        connection.settimeout(4)
        connection.connect(endpoint)
        stream = connection.makefile("rwb", buffering=0)
    try:
        stream.write(request)
        response = stream.readline(1024 * 1024)
        if not response.endswith(b"\n"):
            raise RuntimeError("missing or oversized RPC response")
        value = json.loads(response)
        if value.get("ok") is not True:
            raise RuntimeError(f"RPC rejected: {value}")
        return value["result"]
    finally:
        stream.close()
        if os.name != "nt":
            connection.close()


def rpc(endpoint, method, params=None, timeout=6):
    # Bound named-pipe reads too: only this short-lived, owned RPC helper is killed on timeout.
    result = subprocess.run([sys.executable, str(Path(__file__).resolve()), "--rpc", endpoint, method, json.dumps(params or {})],
                            capture_output=True, text=True, timeout=timeout, check=True)
    return json.loads(result.stdout)


def events(case):
    counter = case["counter"]
    return [json.loads(line) for line in counter.read_text(encoding="utf-8").splitlines()] if counter.exists() else []


def environment(directory, endpoint, fake, mode):
    # Preserve only platform necessities; never inherit live runtime, sockets, auth or real CLI PATH.
    keep = ["SystemRoot", "WINDIR", "COMSPEC", "SYSTEMDRIVE", "PATHEXT", "LANG", "LC_ALL"]
    env = {key: os.environ[key] for key in keep if key in os.environ}
    home = directory / "home"
    for path in [home / ".cys", directory / "bin", directory / "pack", directory / "config",
                 directory / "tmp", home / "local", home / "roaming", home / "state"]:
        path.mkdir(parents=True, exist_ok=True)
    target = directory / "bin" / ("agy.exe" if os.name == "nt" else "agy")
    shutil.copy2(fake, target)
    system_paths = [str(Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32")] if os.name == "nt" else ["/usr/bin", "/bin"]
    env.update({
        "HOME": str(home), "USERPROFILE": str(home), "LOCALAPPDATA": str(home / "local"),
        "APPDATA": str(home / "roaming"), "XDG_STATE_HOME": str(home / "state"),
        "XDG_CONFIG_HOME": str(directory / "config"), "XDG_CACHE_HOME": str(home / "cache"),
        "PATH": os.pathsep.join([str(target.parent), *system_paths]),
        "TMPDIR": str(directory / "tmp"), "TEMP": str(directory / "tmp"), "TMP": str(directory / "tmp"),
        "CYS_SOCKET": endpoint, "CYS_PACK_DIR": str(directory / "pack"),
        "CYS_CONFIG_DIR": str(directory / "config"), "CYS_DEPTS_JSON": str(home / "depts.json"),
        "CYS_NO_AUTOSTART": "1", "CYS_NO_AUTORESTORE": "1", "CYS_AUTORESTORE": "0",
        "CYS_NO_OFFICE_BRIDGE": "1", "CYS_USAGE_EXTERNAL_SECS": "0",
        "CYS_SHELL": str(Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32/WindowsPowerShell/v1.0/powershell.exe") if os.name == "nt" else "/bin/sh",
        "AGY_FAKE_MODE": mode, "AGY_FAKE_COUNTER": str(directory / "calls.jsonl"),
    })
    return env, target



def windows_profile_accounts():
    # dirs 6 uses FOLDERID_Profile, not the child HOME/USERPROFILE environment.
    command = r"""$ErrorActionPreference='Stop'
$profileRoot=[Environment]::GetFolderPath('UserProfile')
if (-not [IO.Path]::IsPathRooted($profileRoot)) { throw 'KnownFolder profile is not absolute' }
$accountFile=Join-Path $profileRoot '.cys\accounts.json'
$exists=Test-Path -LiteralPath $accountFile
$providers=@(); $digest=$null; $customAdapters=0
if ($exists) {
  $content=Get-Content -LiteralPath $accountFile -Raw
  if ([string]::IsNullOrWhiteSpace($content)) { throw 'KnownFolder accounts.json is empty or malformed' }
  $declaration=$content | ConvertFrom-Json
  if ($null -eq $declaration) { throw 'KnownFolder accounts.json has no object' }
  $providers=@($declaration.accounts | ForEach-Object { if ($null -ne $_.provider) { [string]$_.provider } })
  $customAdapters=@($declaration.accounts | Where-Object { $_.adapter -eq 'cmd' }).Count
  $digest=(Get-FileHash -LiteralPath $accountFile -Algorithm SHA256).Hash.ToLowerInvariant()
}
@{profile=$profileRoot;accounts_file=$accountFile;exists=$exists;providers=$providers;custom_command_adapters=$customAdapters;sha256=$digest} | ConvertTo-Json -Compress
"""
    result = subprocess.run(["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", command],
                            capture_output=True, text=True, timeout=15, check=True)
    return json.loads(result.stdout)


def require_no_agy_account(proof):
    if proof["custom_command_adapters"] != 0:
        raise RuntimeError("actual KnownFolder runner accounts.json has command adapters; isolated proof cannot launch them")
    if any(provider.lower() in {"agy", "gemini", "antigravity"} for provider in proof["providers"]):
        raise RuntimeError("actual KnownFolder runner accounts.json already declares agy; proof must not modify it")


def declare_account(case):
    if case["name"] != "L1" and not case["windows"]:
        write_json(case["directory"] / "home/.cys/accounts.json", {"accounts": [{"provider": "agy", "adapter": "none"}]})
        case["config_method"] = "isolated-accounts-json"
    else:
        case["config_method"] = "no-agy-account-or-seat"


def passive_surface(case, agent):
    hold = case.get("duration_seconds", 660) + 180
    reply = rpc(case["endpoint"], "surface.create", {"title": f"proof-{agent}", "cwd": str(case["directory"]),
                "cmd": f"Start-Sleep -Seconds {hold}" if case.get("windows", False) else f"/bin/sleep {hold}",
                "rows": 24, "cols": 80})
    sid = reply["surface_id"]
    case.setdefault("surfaces", []).append(sid)
    rpc(case["endpoint"], "surface.set_meta", {"surface_id": sid, "agent": agent, "agent_bin": f"proof-no-real-{agent}"})
    case[f"{agent}_sid"] = sid
    return sid


def configure_ready_case(case):
    if case["windows"] and case["name"] != "L1":
        passive_surface(case, "agy")
        case["config_method"] = "live-agy-metadata-seat"
    write_json(case["directory"] / "configuration.json", {"method": case["config_method"],
               "owned_surface_ids": case.get("surfaces", []), "duration_seconds": case["duration_seconds"],
               "windows_knownfolder_account_proof": "../windows-knownfolder-accounts-before.json" if case["windows"] else None})


def process_evidence(process):
    if os.name == "nt":
        command = f"Get-CimInstance Win32_Process -Filter 'ProcessId = {process.pid}' | Select-Object ProcessId,ParentProcessId,ExecutablePath,CommandLine | ConvertTo-Json -Compress"
        args = ["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", command]
    else:
        args = ["/bin/ps", "-p", str(process.pid), "-o", "pid=,ppid=,command="]
    value = subprocess.run(args, capture_output=True, text=True, timeout=10)
    return {"pid": process.pid, "exit_code": process.poll(), "command": args,
            "observation_exit": value.returncode, "stdout": value.stdout, "stderr": value.stderr}



def codex_event(percent):
    return {"type": "event_msg", "payload": {"type": "token_count",
            "info": {"last_token_usage": {"total_tokens": 100, "reasoning_output_tokens": 0}, "model_context_window": 1000},
            "rate_limits": {"primary": {"used_percent": percent, "window_minutes": 300, "resets_at": 2000000000}}}}


def control_report(case, percent):
    return rpc(case["endpoint"], "usage.report", {"surface_id": case["claude_sid"],
               "ctx_pct": 10, "ctx_tokens": 100, "ctx_window": 1000,
               "session_file": str(case["claude_session"]),
               "rate": [{"label": "5h", "used_pct": percent, "resets_at": 2000000000}]})


def prepare_controls(case):
    directory, endpoint = case["directory"], case["endpoint"]
    for agent in ["claude", "codex"]:
        passive_surface(case, agent)
    profile = directory / "home/proof-profile"
    case["claude_session"] = profile / "projects/proof/session.jsonl"
    case["claude_session"].parent.mkdir(parents=True)
    case["claude_session"].write_text("", encoding="utf-8")
    write_json(profile / ".claude.json", {"oauthAccount": {"accountUuid": "proof-claude", "emailAddress": "proof@example.invalid"}})
    control_report(case, 17)
    case["codex_transcript"] = directory / "codex.jsonl"
    case["codex_transcript"].write_text(json.dumps(codex_event(23)) + "\n", encoding="utf-8")
    rpc(endpoint, "usage.register", {"surface_id": case["codex_sid"], "transcript": str(case["codex_transcript"])})
    case["control_seen"] = {"claude": set(), "codex": set()}
    case["control_current"] = {}
    case["control_midpoint_updated"] = False


def observe_controls(case, rows):
    case["control_current"] = {}
    for row in rows:
        provider = row.get("provider")
        expected = ((provider == "claude" and row.get("account_id") == "proof-claude" and row.get("source") == "statusline")
                    or (provider == "codex" and row.get("source", "").startswith("rollout")))
        if expected:
            for window in row.get("rate", []):
                if window.get("label") == "5h":
                    case["control_seen"][provider].add(window.get("used_pct"))
                    case["control_current"][provider] = window.get("used_pct")


def verdict(cases, elapsed):
    reasons = []
    minimum = 960 if any(case.get("windows", False) for case in cases.values()) else 660
    if elapsed < minimum:
        reasons.append(f"observation shorter than {minimum} seconds")
    counts = {name: len(events(case)) for name, case in cases.items()}
    for name, case in cases.items():
        if case["process"].poll() is not None:
            reasons.append(f"{name}: daemon exited during observation")
        if case.get("windows") and name != "L1" and case.get("config_method") != "live-agy-metadata-seat":
            reasons.append(f"{name}: Windows eligibility was not proven by a live agy metadata seat")
        for event in events(case):
            if event.get("argv") != COLLECTOR_ARGS or not event.get("valid_args"):
                reasons.append(f"{name}: unexpected fake agy invocation")
    if counts["L1"] != 0:
        reasons.append("L1: unconfigured daemon launched agy")
    if counts["L2"] != 1:
        reasons.append("L2: auth failure did not stop after exactly one launch")
    normal = events(cases["L3"])
    intervals = [b["time"] - a["time"] for a, b in zip(normal, normal[1:])]
    if len(normal) < 3 or any(not 299 <= gap <= 330 for gap in intervals):
        reasons.append("L3: fewer than three launches or measured cadence outside 300s + startup allowance")
    rates = cases["L3"].get("rates", [])
    if len(set(rates)) < 3:
        reasons.append("L3: fewer than three distinct collected usage timestamps")
    seen = cases["L3"].get("control_seen", {})
    if not {17, 19}.issubset(seen.get("claude", set())) or not {23, 29}.issubset(seen.get("codex", set())):
        reasons.append("L3: synthetic Claude statusline / Codex rollout controls did not preserve both updates")
    current = cases["L3"].get("control_current", {})
    if current != {"claude": 19, "codex": 29}:
        reasons.append("L3: final synthetic Claude/Codex control rows disappeared or reverted")
    return {"result": "PASS" if not reasons else "FAIL", "elapsed_seconds": elapsed,
            "launch_counts": counts, "normal_intervals_seconds": intervals,
            "configuration_methods": {name: case.get("config_method") for name, case in cases.items()},
            "normal_collected_timestamps": sorted(set(rates)), "failures": reasons,
            "controls": {"final percentages": current, "Claude statusline fixture": sorted(seen.get("claude", set())),
                         "Codex rollout fixture": sorted(seen.get("codex", set())),
                         "real logged-in Claude/Codex CLIs": "NOT_EXECUTED", "rendered usage panel": "NOT_EXECUTED"}}


def run(args):
    minimum = 960 if os.name == "nt" else 660
    if args.duration_secs < minimum:
        raise ValueError(f"live proof requires at least {minimum} seconds on this platform; cadence must not be accelerated")
    daemon, fake, out = args.daemon.resolve(strict=True), args.fake.resolve(strict=True), args.out.resolve()
    if not daemon.is_file() or not fake.is_file():
        raise ValueError("daemon and fake must be installed executable files")
    if not os.environ.get("GITHUB_ACTIONS") == "true":
        raise RuntimeError("installed daemon execution is CI-only; local use is limited to unit tests")
    if os.environ.get("CANDIDATE_SHA") != CANDIDATE_SHA:
        raise RuntimeError("CANDIDATE_SHA must match the frozen reviewed candidate")
    out.mkdir(parents=True, exist_ok=True)
    write_json(out / "identity.json", {"daemon": str(daemon), "daemon_sha256": sha256(daemon),
               "fake": str(fake), "fake_sha256": sha256(fake), "platform": sys.platform,
               "duration_seconds": args.duration_secs, "candidate_sha": os.environ.get("CANDIDATE_SHA"),
               "github_run_id": os.environ.get("GITHUB_RUN_ID")})
    cases, result = {}, {"result": "FAIL", "failures": ["run incomplete"]}
    try:
        if os.name == "nt":
            account_proof = windows_profile_accounts()
            write_json(out / "windows-knownfolder-accounts-before.json", account_proof)
            require_no_agy_account(account_proof)
        for name, mode in [("L1", "normal"), ("L2", "auth"), ("L3", "normal")]:
            directory = out / name
            directory.mkdir()
            endpoint = rf"\\.\pipe\agy-proof-{uuid.uuid4().hex}" if os.name == "nt" else str(directory / "daemon.sock")
            if os.name != "nt" and len(endpoint.encode()) >= 100:
                raise ValueError("use a short --out path so the Unix socket stays below platform limits")
            env, target = environment(directory, endpoint, fake, mode)
            case = {"name": name, "windows": os.name == "nt", "directory": directory,
                    "duration_seconds": args.duration_secs, "endpoint": endpoint, "surfaces": []}
            declare_account(case)
            log = (directory / "daemon.log").open("wb")
            try:
                process = subprocess.Popen([str(daemon)], cwd=directory, env=env, stdout=log, stderr=subprocess.STDOUT)
            except Exception:
                log.close()
                raise
            case.update({"process": process, "log": log, "counter": directory / "calls.jsonl", "rates": []})
            cases[name] = case
            write_json(directory / "process.json", {"command": [str(daemon)], "cwd": str(directory),
                       "executable_sha256": sha256(daemon), "pid": process.pid,
                       "fake_executable": str(target), "fake_sha256": sha256(target), "environment": env})
        deadline = time.monotonic() + 60
        pending = set(cases)
        while pending:
            for name in list(pending):
                case = cases[name]
                if case["process"].poll() is not None:
                    raise RuntimeError(f"{name}: daemon exited before ready")
                try:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise TimeoutError("startup readiness deadline exceeded")
                    if rpc(case["endpoint"], "system.ping", timeout=min(6, remaining)) == "pong":
                        pending.remove(name)
                except (OSError, ValueError, subprocess.SubprocessError):
                    pass
            if pending and time.monotonic() >= deadline:
                raise TimeoutError(f"daemons not ready within 60 seconds: {sorted(pending)}")
            if pending:
                time.sleep(1)
        for case in cases.values():
            status = rpc(case["endpoint"], "org.status")
            write_json(case["directory"] / "org-status-ready.json", status)
            if status.get("daemon", {}).get("build_id") != CANDIDATE_SHA[:12]:
                raise RuntimeError("installed daemon build_id does not match reviewed candidate")
            write_json(case["directory"] / "process-ready.json", process_evidence(case["process"]))
            configure_ready_case(case)
        prepare_controls(cases["L3"])
        write_json(cases["L3"]["directory"] / "control-fixtures.json", {
            "owned_surface_ids": cases["L3"]["surfaces"],
            "claude_surface_id": cases["L3"]["claude_sid"], "codex_surface_id": cases["L3"]["codex_sid"],
            "passive_shell_lifetime_seconds": args.duration_secs + 180})
        started, last_progress = time.monotonic(), -30
        while True:
            elapsed = time.monotonic() - started
            control = cases["L3"]
            if elapsed >= 330 and not control["control_midpoint_updated"]:
                control_report(control, 19)
                with control["codex_transcript"].open("a", encoding="utf-8") as log:
                    log.write(json.dumps(codex_event(29)) + "\n")
                control["control_midpoint_updated"] = True
            for name, case in cases.items():
                if case["process"].poll() is not None:
                    raise RuntimeError(f"{name}: owned daemon exited")
                snapshot = rpc(case["endpoint"], "usage.accounts")
                with (case["directory"] / "snapshots.jsonl").open("a", encoding="utf-8") as log:
                    log.write(json.dumps({"elapsed_seconds": elapsed, "time": time.time(), "accounts": snapshot}) + "\n")
                rows = snapshot.get("accounts", [])
                if name == "L3":
                    observe_controls(case, rows)
                for row in rows:
                    if row.get("provider") == "antigravity" and row.get("source") == "agy-cli":
                        rate = row.get("rate", [])
                        if not any(w.get("label") == "Gemini Models 5h" and w.get("used_pct") == 25 for w in rate):
                            raise RuntimeError(f"{name}: wrong collected usage payload")
                        case["rates"].append(row["updated_at"])
            elapsed = time.monotonic() - started
            if elapsed - last_progress >= 30:
                print(json.dumps({"elapsed_seconds": round(elapsed, 1), "launch_counts": {n: len(events(c)) for n, c in cases.items()}}), flush=True)
                last_progress = elapsed
            if elapsed >= args.duration_secs:
                break
            time.sleep(min(10, args.duration_secs - elapsed))
        result = verdict(cases, time.monotonic() - started)
        if os.name == "nt":
            final_account_proof = windows_profile_accounts()
            write_json(out / "windows-knownfolder-accounts-after.json", final_account_proof)
            require_no_agy_account(final_account_proof)
            if final_account_proof != account_proof:
                result["result"] = "FAIL"
                result.setdefault("failures", []).append("actual KnownFolder runner accounts declaration changed during proof")
        for case in cases.values():
            write_json(case["directory"] / "process-final.json", process_evidence(case["process"]))
    except Exception as error:
        result = {"result": "FAIL", "failures": [f"{type(error).__name__}: {error}"]}
    finally:
        for case in cases.values():
            process = case["process"]
            cleanup = {"pid": process.pid, "owned_popen": True, "before_exit_code": process.poll(),
                       "owned_surface_ids": list(case.get("surfaces", [])), "closed_surface_ids": []}
            for sid in case.get("surfaces", []):
                try:
                    reply = rpc(case["endpoint"], "surface.close", {"surface_id": sid})
                    if reply.get("closed") is not True:
                        raise RuntimeError(f"owned surface {sid} did not report closed")
                    cleanup["closed_surface_ids"].append(sid)
                except Exception as error:
                    cleanup.setdefault("surface_close_errors", []).append(str(error))
                    result["result"] = "FAIL"
                    result.setdefault("failures", []).append("owned control surface cleanup failed")
            try:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=5)
                        cleanup["forced_owned_child_kill"] = True
                cleanup["after_exit_code"] = process.poll()
            except Exception as error:
                cleanup["error"] = str(error)
                result["result"] = "FAIL"
                result.setdefault("failures", []).append(f"owned daemon cleanup failed: {error}")
            finally:
                case["log"].close()
                write_json(case["directory"] / "cleanup.json", cleanup)
        write_json(out / "result.json", result)
        files = {str(p.relative_to(out)): sha256(p) for p in out.rglob("*") if p.is_file() and p.name != "SHA256.json"}
        write_json(out / "SHA256.json", files)
    print(json.dumps(result), flush=True)
    return 0 if result["result"] == "PASS" else 1


if __name__ == "__main__":
    if len(sys.argv) == 5 and sys.argv[1] == "--rpc":
        print(json.dumps(rpc_once(sys.argv[2], sys.argv[3], json.loads(sys.argv[4]))))
    else:
        parser = argparse.ArgumentParser(description=__doc__)
        parser.add_argument("--daemon", type=Path, required=True)
        parser.add_argument("--fake", type=Path, required=True)
        parser.add_argument("--out", type=Path, required=True)
        parser.add_argument("--duration-secs", type=int, default=660)
        raise SystemExit(run(parser.parse_args()))
