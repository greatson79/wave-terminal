#!/usr/bin/env bash
# macOS DMG 마지막 단계: 앱을 ad-hoc 으로 다시 봉인(codesign --force --deep -s -)하고
# codesign --verify --deep --strict 가 실패하면 빌드를 중단한 뒤 UDZO DMG 를 만든다.
# 이유: dedup·재패키징이 Tauri 가 봉인한 리소스 서명을 깨뜨려 v0.1.0 DMG 가
#   "code has no resources but signature indicates they must be present" 로 검증 실패했다.
# 사용: scripts/resign-macos-dmg.sh <입력.dmg> <출력.dmg>   (출력 끝에 sha256·CDHash 를 찍는다)
set -euo pipefail
IN="${1:?입력 DMG}"; OUT="${2:?출력 DMG}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/wave-resign.XXXXXX")"; MP="$WORK/mount"; STAGE="$WORK/stage"
cleanup() { hdiutil detach "$MP" >/dev/null 2>&1 || true; rm -rf "$WORK"; }
trap cleanup EXIT
mkdir -p "$MP" "$STAGE"
hdiutil attach -nobrowse -readonly -mountpoint "$MP" "$IN" >/dev/null
ditto "$MP" "$STAGE"
hdiutil detach "$MP" >/dev/null
APP="$(find "$STAGE" -maxdepth 1 -type d -name '*.app' -print -quit)"
[[ -n "$APP" ]] || { echo "앱 없음" >&2; exit 1; }
codesign --force --deep -s - "$APP"
codesign --verify --deep --strict "$APP" || { echo "codesign verify 실패 — 빌드 중단" >&2; exit 1; }
VOL="$(basename "$APP" .app)"
rm -f "$OUT"
hdiutil create -volname "$VOL" -srcfolder "$STAGE" -ov -format UDZO "$OUT" >/dev/null
echo "dmg_sha256=$(shasum -a 256 "$OUT" | awk '{print $1}')"
echo "cdhash=$(codesign -dvvv "$APP" 2>&1 | sed -n 's/^CDHash=//p' | head -1)"
