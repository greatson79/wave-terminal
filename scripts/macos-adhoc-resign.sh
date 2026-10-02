#!/usr/bin/env bash
# macos-adhoc-resign.sh — .app 을 inside-out **ad-hoc** 재서명하고 codesign --verify --deep --strict 로 검증.
# ⚠ 시험·내부 배포용: Developer ID 서명·공증(notarization)이 **아니다**. 다른 맥에서는 Gatekeeper 가
#   막는다(quarantine). 정식 배포는 build-macos-signed.sh 의 Developer ID 경로만.
# 순서(참조 구현 = wave-install-rc tests/rc/mac-resign.sh): ①안쪽 Mach-O 파일(깊은 경로 먼저)
#   ②중첩 번들(.framework/.app/.xpc/.bundle — 깊은 것 먼저) ③앱 자체. --deep 서명 금지(검증 전용).
# 사용: scripts/macos-adhoc-resign.sh <App.app>     종료: 0=검증 통과 · 1=검증 실패 · 2=입력 오류
set -euo pipefail
APP="${1:?usage: macos-adhoc-resign.sh <App.app>}"
[ -d "$APP" ] || { echo "✗ 앱 번들 없음: $APP" >&2; exit 2; }
xattr -cr "$APP"
n=0
while IFS= read -r f; do
  codesign --force --sign - --timestamp=none "$f" >/dev/null 2>&1 || { echo "✗ 서명 실패: $f" >&2; exit 1; }
  n=$((n+1))
done < <(find "$APP" -type f ! -type l -print0 | while IFS= read -r -d '' f; do
           file -b "$f" | grep -q 'Mach-O' && echo "$f"; done | awk '{print length "\t" $0}' | sort -rn | cut -f2-)
b=0
while IFS= read -r d; do
  codesign --force --sign - --timestamp=none "$d" >/dev/null 2>&1 || { echo "✗ 번들 서명 실패: $d" >&2; exit 1; }
  b=$((b+1))
done < <(find "$APP" -mindepth 2 -type d \( -name '*.framework' -o -name '*.app' -o -name '*.xpc' -o -name '*.bundle' \) -print \
           | awk '{print length "\t" $0}' | sort -rn | cut -f2-)
codesign --force --sign - --timestamp=none "$APP"
echo "  ad-hoc 재서명: Mach-O ${n}개 · 중첩 번들 ${b}개 · 앱 1개 (NOT NOTARIZED)"
if codesign --verify --deep --strict --verbose=2 "$APP" 2>&1; then
  echo "  ✓ codesign --verify --deep --strict 통과 (ad-hoc · 공증 아님)"
else
  echo "  ✗ codesign --verify --deep --strict 실패" >&2; exit 1
fi
