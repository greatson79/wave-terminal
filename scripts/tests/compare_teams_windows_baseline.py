#!/usr/bin/env python3
"""Run three unchanged library tests at the pinned baseline and candidate, without changing assertions."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

BASELINE = "85f160fde191593c290293e6295961f712ff3e47"
CASES = {
    "pack::tests::install_from_iter_equivalent_to_install": "A 산출물에 skills/korean-humanizer/SKILL.md 부재",
    "tests::resolve_claude_config_dir_is_deterministic_env_not_scan": "기본 해소:",
    "tests::runtime_prefixed_path_prepends_exe_dir_and_dedups": "기존 PATH 보존(제거 없음)",
}


def classify(name, returncode, output):
    if returncode == 0 and re.search(r"test result: ok\. 1 passed; 0 failed;", output):
        return "PASS"
    if (returncode == 101 and f"test {name} ... FAILED" in output
            and "test result: FAILED. 0 passed; 1 failed;" in output
            and CASES[name] in output):
        return "EXPECTED_ASSERTION_FAILURE"
    raise AssertionError(f"{name}: execution/build error or unexpected assertion; see retained log")


def run_case(repo, name, target, log):
    environment = dict(os.environ)
    actual_home = Path.home()
    environment.setdefault("CARGO_HOME", str(actual_home / ".cargo"))
    environment.setdefault("RUSTUP_HOME", str(actual_home / ".rustup"))
    with tempfile.TemporaryDirectory(prefix="teams-baseline-") as directory:
        environment.update(HOME=directory, USERPROFILE=directory, CYS_HOME=directory,
                           CYS_PACK_DIR=str(Path(directory) / "pack"), CYS_NO_AUTOSTART="1", CARGO_TERM_COLOR="never")
        command = ["cargo", "test", "--locked", "--no-default-features", "--lib", name,
                   "--target-dir", str(target), "--", "--exact", "--test-threads=1"]
        result = subprocess.run(command, cwd=repo, env=environment, stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                text=True, encoding="utf-8", errors="replace", timeout=1200)
    log.write_text(result.stdout, encoding="utf-8")
    print(f"{repo.name}: {name}: exit={result.returncode}; log={log}")
    print(result.stdout)
    return {"returncode": result.returncode, "outcome": classify(name, result.returncode, result.stdout), "log": str(log)}


def compare(baseline, candidate, output):
    baseline = baseline.resolve()
    candidate = candidate.resolve()
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=baseline, text=True).strip()
    assert source == BASELINE, "baseline checkout must be the unchanged pinned v0.3.1 commit"
    assert not subprocess.check_output(["git", "diff", "--name-only", "HEAD"], cwd=baseline, text=True).strip(), "baseline has tracked modifications"
    report = {"baseline": source, "candidate": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=candidate, text=True).strip(),
              "candidate_tracked_changes": subprocess.check_output(["git", "diff", "--name-only", "HEAD"], cwd=candidate, text=True).splitlines(), "cases": {}}
    for name in CASES:
        slug = name.replace("::", "_")
        results = {label: run_case(repo, name, output / "cargo-target", output / f"{label}-{slug}.log")
                   for label, repo in [("baseline", baseline), ("candidate", candidate)]}
        assert results["baseline"]["outcome"] == results["candidate"]["outcome"], f"{name}: candidate differs from baseline"
        if os.name == "nt":
            assert results["baseline"]["outcome"] == "EXPECTED_ASSERTION_FAILURE", f"{name}: expected Windows assertion did not reproduce"
        results["classification"] = "KNOWN_PRE_EXISTING_WINDOWS_ASSERTION" if results["baseline"]["outcome"] != "PASS" else "BOTH_PASS_ON_THIS_HOST"
        report["cases"][name] = results
    (output / "comparison.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False, indent=2))
    print("Baseline comparison completed; original failing tests remain failures in the full-suite gate.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    compare(args.baseline, args.candidate, args.output)
