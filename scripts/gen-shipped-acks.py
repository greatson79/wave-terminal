#!/usr/bin/env python3
"""C60 출하 승인 기록 생성·대조 (CEO 결정 0445 · 2026-10-03).

팩에 동봉되는 두 기록을 **설치본 형태**로 계산한다 — build.rs가 임베드하는 집합(git 추적 파일 중
경로 컴포넌트가 '.'로 시작·tests·__pycache__ 인 것 제외)과 같은 파일만 스테이징해 핀을 뽑는다.
  cysjavis-pack/round/skillscan_acknowledged.json   — skillscan BLOCK 승인(card fingerprint 핀)
  cysjavis-pack/round/mcp_approved/<skill>.json     — mcpgate 승인 스냅샷(rug-pull diff 기준선)
승인 스킬의 파일이 바뀌면 핀이 어긋나 C60이 다시 막는다 — 재승인 후 --write로 갱신한다.

사용:  python3 scripts/gen-shipped-acks.py --check   # 커밋된 기록 == 현재 트리 설치본 (0=일치 1=드리프트)
       python3 scripts/gen-shipped-acks.py --write   # 기록 재생성
"""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "cysjavis-pack")
DECISION = "CEO 결정 0445 (2026-10-03) — C60 미승인스킬 결정표 승인 6건"
APPROVED = ["brainstorming", "insane-search", "kosis-stats",
            "skillscan-semantic", "systematic-debugging", "transcription"]
ACK_REL = "round/skillscan_acknowledged.json"
MCP_REL = "round/mcp_approved"


def shipped_rels():
    """build.rs와 같은 규칙의 임베드 대상 rel 목록(cysjavis-pack/ 접두 제거)."""
    out = subprocess.run(["git", "ls-files", "cysjavis-pack"], cwd=ROOT,
                         capture_output=True, text=True, check=True).stdout
    rels = set()
    for line in out.splitlines():
        line = line.strip()
        if not line.startswith("cysjavis-pack/"):
            continue
        rel = line[len("cysjavis-pack/"):]
        if any(c.startswith(".") or c in ("tests", "__pycache__") for c in rel.split("/")):
            continue
        rels.add(rel)
    return sorted(rels)


def install_pack(dst, prefix=""):
    """앱 init-pack 설치 형태 재현: 임베드 대상 파일 복사 + .install-manifest.json(rel→sha256).
    prefix가 있으면 그 아래 파일만 스테이징(매니페스트 미작성)."""
    manifest = {}
    for rel in shipped_rels():
        if prefix and not rel.startswith(prefix):
            continue
        src = os.path.join(PACK, *rel.split("/"))
        if not os.path.isfile(src):
            continue
        tgt = os.path.join(dst, *rel.split("/"))
        os.makedirs(os.path.dirname(tgt), exist_ok=True)
        shutil.copyfile(src, tgt)
        if os.access(src, os.X_OK):
            os.chmod(tgt, 0o755)
        manifest[rel] = hashlib.sha256(open(src, "rb").read()).hexdigest()
    if not prefix:
        with open(os.path.join(dst, ".install-manifest.json"), "w", encoding="utf-8") as f:
            json.dump(manifest, f, indent=2, sort_keys=True)
    return manifest


def fingerprint(skill_dir):
    r = subprocess.run([sys.executable, os.path.join(PACK, "bin", "javis_skillscan.py"),
                        "card", skill_dir, "--json"], capture_output=True, text=True, check=True)
    return json.loads(r.stdout)["fingerprint"]


def build_records():
    """{rel: bytes} — 출하 기록 전부(설치본 기준)."""
    recs = {}
    with tempfile.TemporaryDirectory() as td:
        install_pack(td, prefix="skills/")
        acks = {"_schema": "cys-skillscan-ack/v1",
                "_basis": "설치본 형태(build.rs 임베드 집합) card fingerprint — scripts/gen-shipped-acks.py"}
        for s in APPROVED:
            acks[s] = {"fingerprint": fingerprint(os.path.join(td, "skills", s)), "approved_by": DECISION}
        recs[ACK_REL] = (json.dumps(acks, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
        store = os.path.join(td, "_mcp")
        for s in APPROVED:
            subprocess.run([sys.executable, os.path.join(PACK, "bin", "javis_mcpgate.py"), "snapshot",
                            os.path.join(td, "skills", s), "--store", store, "--json"],
                           capture_output=True, text=True, check=True)
            recs["%s/%s.json" % (MCP_REL, s)] = open(os.path.join(store, s + ".json"), "rb").read()
    return recs


def main():
    ap = argparse.ArgumentParser()
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--write", action="store_true")
    g.add_argument("--check", action="store_true")
    a = ap.parse_args()
    recs = build_records()
    mcp_dir = os.path.join(PACK, *MCP_REL.split("/"))
    on_disk = {"%s/%s" % (MCP_REL, f) for f in (os.listdir(mcp_dir) if os.path.isdir(mcp_dir) else [])}
    if a.write:
        os.makedirs(mcp_dir, exist_ok=True)
        for rel in on_disk - set(recs):
            os.remove(os.path.join(PACK, *rel.split("/")))
        for rel, data in recs.items():
            with open(os.path.join(PACK, *rel.split("/")), "wb") as f:
                f.write(data)
        print("written: %d records" % len(recs))
        return 0
    drift = sorted(rel for rel, data in recs.items()
                   if not os.path.isfile(os.path.join(PACK, *rel.split("/")))
                   or open(os.path.join(PACK, *rel.split("/")), "rb").read() != data)
    drift += sorted(on_disk - set(recs))
    for rel in drift:
        print("DRIFT %s" % rel)
    print("OK" if not drift else "드리프트 %d건 — 재승인 후 --write" % len(drift))
    return 1 if drift else 0


if __name__ == "__main__":
    sys.exit(main())
