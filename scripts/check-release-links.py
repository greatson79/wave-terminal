#!/usr/bin/env python3
"""Reject upstream identity references outside exact license and fixture lines."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def main():
    allowed = json.loads(Path("scripts/release-path-allowlist.json").read_text())
    result = subprocess.run(["git", "grep", "-n", "-i", "cys" + "insight", "--"],
                            text=True, encoding="utf-8", capture_output=True)
    if result.returncode not in (0, 1):
        print("FAIL: git grep could not inspect tracked source", file=sys.stderr)
        return 2
    failures = []
    for row in result.stdout.splitlines():
        path, number, line = row.split(":", 2)
        digest = hashlib.sha256(line.encode()).hexdigest()
        entry = allowed.get(path, {})
        if not entry.get("reason") or digest not in entry.get("line_sha256", []):
            failures.append(f"{path}:{number}")
    if failures:
        print("FAIL: unapproved release references: " + ", ".join(failures))
        return 1
    print(f"PASS: release references checked ({len(result.stdout.splitlines())} approved lines)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
