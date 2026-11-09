#!/usr/bin/env python3
"""Update-proof manifest for a v<X.Y.Z>-rc.<N> PRERELEASE — never the channel manifest.
Writes <out>/latest-rc.json (NOT latest.json) whose url points at the rc release's own assets, so
releases/latest/download/latest.json (the real 0.3.1 update channel) is never touched.
usage: make-rc-manifest.py <rc_tag> <owner/repo> <out_dir> windows-x86_64=<setup.exe.sig> [darwin-aarch64=<app.tar.gz.sig>]
The .sig files come from the owner's local updater key (docs/RELEASE.md); the assets must be uploaded under the names printed."""
import json, re, sys, time
from pathlib import Path

ASSET = {"windows-x86_64": "wave-terminal-{v}-windows-x64-setup.exe", "darwin-aarch64": "wave-terminal-{v}-macos-arm64.app.tar.gz"}


def build(tag, repo, sigs):
    m = re.fullmatch(r"v(\d+\.\d+\.\d+)-rc\.(\d+)", tag)
    if not m: raise SystemExit("error: tag must be vX.Y.Z-rc.N, got %r" % tag)
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repo): raise SystemExit("error: bad repo")
    version = m.group(1)   # the installer's own version (0.3.2): the updater compares it with the installed 0.3.1
    platforms = {}
    for key, sig in sigs.items():
        if key not in ASSET: raise SystemExit("error: unknown platform %r" % key)
        text = Path(sig).read_text(encoding="utf-8").strip()
        if not text: raise SystemExit("error: empty signature for %s" % key)
        platforms[key] = {"signature": text, "url": "https://github.com/%s/releases/download/%s/%s" % (repo, tag, ASSET[key].format(v=version))}
    if not platforms: raise SystemExit("error: at least one platform=<sig> is required")
    return {"version": version, "notes": "update proof %s (prerelease, not the channel)" % tag,
            "pub_date": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "platforms": platforms}


def main(argv):
    if len(argv) < 5: raise SystemExit(__doc__)
    tag, repo, out = argv[1], argv[2], Path(argv[3])
    sigs = dict(a.split("=", 1) for a in argv[4:])
    doc = build(tag, repo, sigs)
    out.mkdir(parents=True, exist_ok=True)
    target = out / "latest-rc.json"
    target.write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")
    print(target)
    for key in doc["platforms"]: print("upload as:", doc["platforms"][key]["url"].rsplit("/", 1)[1])
    print("upload as: latest-rc.json")


if __name__ == "__main__":
    main(sys.argv)
