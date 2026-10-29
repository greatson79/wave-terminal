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


def needle_groups(ci_root: str, ci_user: str, owner_user: str) -> dict[str, list[bytes]]:
    groups = {"owner-identity": [], "owner-local-home": [], "ci-checkout": [], "ci-home": []}
    if owner_user:
        groups["owner-identity"].append(os.fsencode(owner_user))
        groups["owner-local-home"].extend(os.fsencode(f"/Users/{owner_user}{separator}")
                                          for separator in ("/", "\\"))
    if ci_root:
        normalized = os.path.normpath(ci_root)
        groups["ci-checkout"].extend(os.fsencode(v) for v in {
            normalized, normalized.replace("\\", "/"), normalized.replace("/", "\\")
        })
    if ci_user:
        for prefix in ("/Users/", "/home/", "C:\\Users\\", "D:\\Users\\"):
            groups["ci-home"].extend(os.fsencode(f"{prefix}{ci_user}{separator}")
                                     for separator in ("/", "\\"))
    return {category: list(dict.fromkeys(values)) for category, values in groups.items()}


def scan_tree(tree: Path, patterns: list[bytes] | dict[str, list[bytes]],
              case_insensitive: bool = False) -> tuple[int, list[str], dict[str, int], dict[str, int], dict[str, str]]:
    scanned = 0
    hits = []
    groups_count: dict[str, int] = {}
    class_counts: dict[str, int] = {"private-files": 0, "ci-files": 0, "unreadable": 0}
    component_counts: dict[str, int] = {}
    component_samples: dict[str, str] = {}
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
                class_counts["unreadable"] += 1
                continue
        else:
            continue
        scanned += 1
        if case_insensitive:
            data = data.lower()
        matched = [name for name, markers in groups.items() if any(pattern in data for pattern in markers)]
        if matched:
            hits.append(path.name)
            if any(name.startswith("owner-") for name in matched):
                class_counts["private-files"] += 1
            if any(name.startswith("ci-") for name in matched):
                class_counts["ci-files"] += 1
            for name in matched:
                groups_count[name] = groups_count.get(name, 0) + 1
            component = "/".join(path.relative_to(tree).parts[:7])
            component_counts[component] = component_counts.get(component, 0) + 1
            component_samples.setdefault(component, path.name)
    groups_count.update(class_counts)
    return scanned, hits, groups_count, component_counts, component_samples


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


def check_asset(kind: str, asset: Path, patterns: dict[str, list[bytes]]) -> tuple[int, int, dict[str, int], dict[str, int], dict[str, str]]:
    with tempfile.TemporaryDirectory(prefix="release-path-gate-") as tmp:
        root = Path(tmp)
        tree = extract_asset(kind, asset, root / "contents")
        try:
            scanned, hits, categories, components, samples = scan_tree(tree, patterns, case_insensitive=kind == "win")
            return scanned, len(hits), categories, components, samples
        finally:
            if kind == "mac-dmg" and sys.platform == "darwin":
                subprocess.run(["hdiutil", "detach", str(tree), "-quiet"], check=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mac-tar", type=Path)
    parser.add_argument("--mac-dmg", type=Path)
    parser.add_argument("--win", type=Path, help="NSIS setup.exe; scans extracted payload")
    parser.add_argument("--ci-root", default=os.getcwd(), help=argparse.SUPPRESS)
    parser.add_argument("--ci-user", default=getpass.getuser(), help=argparse.SUPPRESS)
    parser.add_argument("--owner-user", default="kylechoi", help=argparse.SUPPRESS)
    args = parser.parse_args()
    assets = [("mac-tar", args.mac_tar), ("mac-dmg", args.mac_dmg), ("win", args.win)]
    assets = [(kind, path) for kind, path in assets if path]
    if not assets:
        parser.error("provide at least one release asset")

    patterns = needle_groups(os.path.realpath(args.ci_root), args.ci_user, args.owner_user)
    failed = False
    for kind, asset in assets:
        try:
            scanned, leaks, categories, components, samples = check_asset(kind, asset, patterns)
        except (OSError, ValueError, tarfile.TarError, zipfile.BadZipFile, subprocess.SubprocessError):
            print(f"FAIL {kind}: extraction/scan error (fail-closed)")
            failed = True
            continue
        private = categories.get("private-files", 0)
        ci_only = categories.get("ci-files", 0)
        unreadable = categories.get("unreadable", 0)
        print(f"{'PASS' if scanned and not private and not unreadable else 'FAIL'} {kind}: "
              f"{scanned} files scanned; private={private}, CI-path={ci_only}, unreadable={unreadable}")
        if leaks:
            summary = ", ".join(f"{name}={count}" for name, count in categories.items()
                                 if count and name not in {"private-files", "ci-files", "unreadable"})
            print(f"  path classes: {summary}")
            bundle_parts = ", ".join(f"{name}={count}" for name, count in components.items())
            print(f"  bundle components: {bundle_parts}")
            for component, sample in samples.items():
                print(f"  sample: {component}/{sample}")
        failed |= not scanned or bool(private) or bool(unreadable)
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
