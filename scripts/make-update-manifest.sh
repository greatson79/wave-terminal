#!/bin/sh
# 자동 업데이트 manifest(latest.json) 생성 — 업데이터 서명(.sig)을 Tauri updater 표준 포맷으로 묶는다.
# ★업로드 자산 이름의 단일 출처: wave-terminal-<version>-macos-<arm64|x64>.app.tar.gz(+.sig)
#   (릴리스 DMG 명명 wave-terminal-<ver>-macos-<arch>.dmg 와 같은 규칙 · 공백 없음).
#   latest.json 의 url 은 정확히 이 이름을 가리킨다 — 발행 전 scripts/check-update-manifest.sh 로 대조.
#
# 사용:  sh scripts/make-update-manifest.sh --asset-name <version> <arch>      # 자산 이름만 출력
#        sh scripts/make-update-manifest.sh <version> <owner> <repo> <arch> <tarball.sig> [기존 latest.json]
#   arch: aarch64|arm64|aarch64-apple-darwin  /  x86_64|x64|x86_64-apple-darwin
#   <tarball.sig> 옆에 서명 대상 tar.gz(이름 무관 — 예: "Wave Terminal.app.tar.gz")가 있으면 둘 다
#   $UPDATE_OUT(기본 dist-update)/<자산 이름>[.sig] 로 복사한다. 기존 latest.json 을 주면 그 platforms 에
#   이 아키텍처 항목을 병합한다(다른 플랫폼 항목 보존).
# 예:    sh scripts/make-update-manifest.sh 0.1.2 greatson79 wave-terminal aarch64 \
#          "target/release/bundle/macos/Wave Terminal.app.tar.gz.sig"
set -eu

arch_of() {
  case "$1" in
    aarch64|arm64|aarch64-apple-darwin) DIST_ARCH=arm64; PLATFORM=darwin-aarch64 ;;
    x86_64|x64|x86_64-apple-darwin)     DIST_ARCH=x64;   PLATFORM=darwin-x86_64 ;;
    *) echo "error: 알 수 없는 arch: $1" >&2; exit 2 ;;
  esac
}
asset_name() { arch_of "$2"; echo "wave-terminal-$1-macos-${DIST_ARCH}.app.tar.gz"; }

if [ "${1:-}" = "--asset-name" ]; then
  asset_name "${2:?version}" "${3:?arch}"
  exit 0
fi

VERSION="${1:?usage: make-update-manifest.sh <version> <owner> <repo> <arch> <tarball.sig> [latest.json]}"
OWNER="${2:?owner required}"
REPO="${3:?repo required}"
ARCH="${4:?arch required}"
SIG_FILE="${5:?tarball .sig required}"
BASE_JSON="${6:-}"
OUT="${UPDATE_OUT:-dist-update}"
NOTES="${UPDATE_NOTES:-Wave Terminal $VERSION}"

[ -f "$SIG_FILE" ] || { echo "error: $SIG_FILE 없음 — 업데이터 서명 키로 tar.gz 를 서명하라(RELEASE.md)" >&2; exit 1; }
ASSET="$(asset_name "$VERSION" "$ARCH")"
arch_of "$ARCH"
URL="https://github.com/${OWNER}/${REPO}/releases/download/v${VERSION}/${ASSET}"

mkdir -p "$OUT"
TARBALL="${SIG_FILE%.sig}"
if [ -f "$TARBALL" ]; then
  [ "$TARBALL" -ef "$OUT/$ASSET" ] || cp "$TARBALL" "$OUT/$ASSET"
  [ "$SIG_FILE" -ef "$OUT/$ASSET.sig" ] || cp "$SIG_FILE" "$OUT/$ASSET.sig"
fi

python3 - "$OUT/latest.json" "$BASE_JSON" "$VERSION" "$NOTES" "$PLATFORM" "$URL" "$SIG_FILE" <<'PY'
import json, sys, time
out, base, version, notes, platform, url, sig = sys.argv[1:8]
doc = json.load(open(base, encoding="utf-8")) if base else {}
if doc.get("version") not in (None, version):
    sys.exit("error: 기존 latest.json 버전(%s) != %s" % (doc.get("version"), version))
doc.update(version=version, notes=doc.get("notes") or notes,
           pub_date=doc.get("pub_date") or time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()))
doc.setdefault("platforms", {})[platform] = {"signature": open(sig, encoding="utf-8").read().strip(), "url": url}
with open(out, "w", encoding="utf-8") as f:
    json.dump(doc, f, ensure_ascii=False, indent=2)
    f.write("\n")
PY

echo "생성됨: $OUT/latest.json ($PLATFORM → $ASSET)"
[ -f "$OUT/$ASSET" ] && echo "자산:   $OUT/$ASSET  $OUT/$ASSET.sig"
echo "발행 전 대조: sh scripts/check-update-manifest.sh $OUT/latest.json $OWNER/$REPO v$VERSION"
