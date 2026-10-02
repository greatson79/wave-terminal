#!/bin/sh
# latest.json 의 모든 url 이 실제 릴리스 자산을 가리키는지 대조 — 발행 전 필수(docs/RELEASE.md §4).
# 각 url 은 정확히 https://github.com/<repo>/releases/download/<tag>/<자산 이름> 이어야 하고,
# 그 <자산 이름>이 릴리스(또는 [assets.txt] 목록)에 있어야 한다. 공백·%20 이 든 이름은 거부한다
# (GitHub 는 업로드 시 공백을 '.'으로 바꿔 url 과 실제 자산이 어긋난다).
# 사용:  sh scripts/check-update-manifest.sh <latest.json> <owner/repo> <tag> [assets.txt]
#        assets.txt 생략 시 `gh release view <tag> --repo <owner/repo>` 의 자산 목록을 쓴다.
# 종료:  0=전부 일치 · 1=불일치(목록 출력) · 2=사용법/입력 오류
set -eu
JSON="${1:?usage: check-update-manifest.sh <latest.json> <owner/repo> <tag> [assets.txt]}"
REPO="${2:?owner/repo required}"
TAG="${3:?tag required}"
if [ -n "${4:-}" ]; then
  NAMES="$(cat "$4")"
else
  NAMES="$(gh release view "$TAG" --repo "$REPO" --json assets --jq '.assets[].name')"
fi
printf '%s\n' "$NAMES" | python3 -c '
import json, sys
doc = json.load(open(sys.argv[1], encoding="utf-8"))
repo, tag = sys.argv[2], sys.argv[3]
names = {l.strip() for l in sys.stdin if l.strip()}
prefix = "https://github.com/%s/releases/download/%s/" % (repo, tag)
plats = doc.get("platforms") or {}
bad = []
for key, p in sorted(plats.items()):
    url = (p or {}).get("url", "")
    name = url[len(prefix):] if url.startswith(prefix) else None
    if name is None or not name or "/" in name or " " in name or "%20" in name or name not in names:
        bad.append("%s: %s" % (key, url))
for b in bad:
    print("MISMATCH " + b)
if not plats:
    print("MISMATCH platforms 비어 있음")
print("OK %d/%d" % (len(plats) - len(bad), len(plats)) if plats and not bad else "FAIL")
sys.exit(1 if bad or not plats else 0)
' "$JSON" "$REPO" "$TAG"
