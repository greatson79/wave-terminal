#!/usr/bin/env python3
"""Fail closed when packaged release assets contain machine-specific paths."""

import argparse
import getpass
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path


def needles(root: str, user: str) -> list[bytes]:
    # Generic home prefixes are emitted by the help-path scrubber; only flag a concrete build account.
    return [pattern for group in needle_groups(root, user).values() for pattern in group]


def needle_groups(root: str, user: str) -> dict[str, list[bytes]]:
    groups = {"checkout-root": [], "mac-home": [], "linux-home": [], "windows-home": []}
    for value in (root,):
        if value:
            normalized = os.path.normpath(value)
            groups["checkout-root"].extend(os.fsencode(v) for v in {
                normalized, normalized.replace("\\", "/"), normalized.replace("/", "\\")
            })
    if user:
        for category, prefix in (("mac-home", "/Users/"), ("linux-home", "/home/"),
                                 ("windows-home", "C:\\Users\\")):
            groups[category].extend(os.fsencode(f"{prefix}{user}{separator}") for separator in ("/", "\\"))
    return {category: list(dict.fromkeys(values)) for category, values in groups.items()}


def scan_tree(tree: Path, patterns: list[bytes] | dict[str, list[bytes]],
              case_insensitive: bool = False) -> tuple[int, list[str], dict[str, int], dict[str, int]]:
    scanned = 0
    hits = []
    groups_count: dict[str, int] = {}
    component_counts: dict[str, int] = {}
    groups = patterns if isinstance(patterns, dict) else {"matched": patterns}
    groups = {name: ([pattern.lower() for pattern in markers] if case_insensitive else markers)
              for name, markers in groups.items()}
    for path in tree.rglob("*"):
        if path.is_symlink():
            data = os.fsencode(os.readlink(path))
        elif path.is_file():
            try:
                data = path.read_bytes()
            except OSError:
                hits.append("unreadable")
                continue
        else:
            continue
        scanned += 1
        if case_insensitive:
            data = data.lower()
        matched = [name for name, markers in groups.items() if any(pattern in data for pattern in markers)]
        if matched:
            hits.append(path.name)
            for name in matched:
                groups_count[name] = groups_count.get(name, 0) + 1
            component = "/".join(path.relative_to(tree).parts[:4])
            component_counts[component] = component_counts.get(component, 0) + 1
    return scanned, hits, groups_count, component_counts


def extract_asset(kind: str, asset: Path, destination: Path) -> Path:
    if not asset.is_file():
        raise ValueError("asset missing")
    destination.mkdir(parents=True)
    if kind == "mac-tar":
        with tarfile.open(asset, "r:gz") as archive:
            archive.extractall(destination, filter="data")
        return destination
    if kind == "mac-dmg":
        if sys.platform != "darwin":
            raise ValueError("DMG scan requires macOS")
        mountpoint = destination / "mount"
        mountpoint.mkdir()
        result = subprocess.run(
            ["hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", str(mountpoint), str(asset)],
            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True,
        )
        if result.returncode:
            raise ValueError("DMG mount failed")
        return mountpoint
    if kind == "win":
        sevenzip = shutil.which("7z") or shutil.which("7zz")
        if not sevenzip:
            raise ValueError("7-Zip is required for NSIS extraction")
        result = subprocess.run(
            [sevenzip, "x", str(asset), f"-o{destination}", "-y"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        if result.returncode:
            raise ValueError("NSIS extraction failed")
        return destination
    raise ValueError("unknown asset kind")


def check_asset(kind: str, asset: Path, patterns: dict[str, list[bytes]]) -> tuple[int, int, dict[str, int], dict[str, int]]:
    with tempfile.TemporaryDirectory(prefix="release-path-gate-") as tmp:
        root = Path(tmp)
        tree = extract_asset(kind, asset, root / "contents")
        try:
            scanned, hits, categories, components = scan_tree(tree, patterns, case_insensitive=kind == "win")
            return scanned, len(hits), categories, components
        finally:
            if kind == "mac-dmg" and sys.platform == "darwin":
                subprocess.run(["hdiutil", "detach", str(tree), "-quiet"], check=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mac-tar", type=Path)
    parser.add_argument("--mac-dmg", type=Path)
    parser.add_argument("--win", type=Path, help="NSIS setup.exe; scans extracted payload")
    parser.add_argument("--deny-root", default=os.getcwd(), help=argparse.SUPPRESS)
    parser.add_argument("--deny-user", default=getpass.getuser())
    args = parser.parse_args()
    assets = [("mac-tar", args.mac_tar), ("mac-dmg", args.mac_dmg), ("win", args.win)]
    assets = [(kind, path) for kind, path in assets if path]
    if not assets:
        parser.error("provide at least one release asset")

    patterns = needle_groups(os.path.realpath(args.deny_root), args.deny_user)
    failed = False
    for kind, asset in assets:
        try:
            scanned, leaks, categories, components = check_asset(kind, asset, patterns)
        except (OSError, ValueError, tarfile.TarError, zipfile.BadZipFile, subprocess.SubprocessError):
            print(f"FAIL {kind}: extraction/scan error (fail-closed)")
            failed = True
            continue
        print(f"{'PASS' if scanned and not leaks else 'FAIL'} {kind}: {scanned} files scanned, {leaks} files matched")
        if leaks:
            summary = ", ".join(f"{name}={count}" for name, count in categories.items() if count)
            print(f"  path marker classes: {summary}")
            bundle_parts = ", ".join(f"{name}={count}" for name, count in components.items())
            print(f"  bundle components: {bundle_parts}")
        failed |= not scanned or bool(leaks)
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
