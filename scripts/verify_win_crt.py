#!/usr/bin/env python3
"""Windows CRT 발행 차단 게이트 — VCRUNTIME 임포트 스윕(PE 임포트 테이블 파싱).

운영판(cys-terminal-wave 9eacc2e, 3b0f8cf0 도입) 이식 — 설치판 적응:
  · 운영판 검사 B(cysd 수리 마커)는 운영 0.14 라인 전용이라 제거했다(설치판엔 그 마커가 없다).
  · 엄격 대상에 Tauri 앱 cys-app.exe 를 추가했다 — RC v0.3.0 윈 설치본(0.2.0 빌드) 실측에서
    cys.exe·cysd.exe·cys-app.exe 3종 모두 VCRUNTIME140.dll(cys-app 은 _1 포함)을 임포트했다.

무엇을 막는가: 우리 Rust 바이너리가 VCRUNTIME140*.dll 을 동적 임포트하면 VC++ 재배포
  패키지 없는 Windows 에서 기동 불능. 정책 SOT 는 .cargo/config.toml 의 crt-static —
  이 게이트가 산출물에서 그 실효를 증명한다. UCRT(api-ms-win-crt-*)는 Windows 10+ OS 동봉이라
  판정 대상이 아니다(운영판 정책 동일).

판정 방법: PE 임포트 디렉토리 파싱만 사용(원시 바이트 grep 은 오검출로 부적합 — 운영판 실측).
  CI 러너엔 VC 재배포가 프리인스톨돼 런타임 재현이 불가능하므로 정적 검사가 유일한 결정론 검증.

임포트 판정 2단:
  1단(엄격): STRICT_BINARIES 는 vcruntime 임포트 자체가 실패(같은 폴더 DLL 동봉으로도 불허).
  2단(규칙): 그 외 exe(동봉 runtime 등)는 같은 디렉토리에 vcruntime140.dll 동봉 시만 허용.

종료 코드: 0=통과 / 1=CRT 위반 / 3=PE 파싱 실패(fail-closed) / 4=입력·추출 오류.

사용:
  python3 scripts/verify_win_crt.py --setup "target/release/bundle/nsis/<이름>-setup.exe"
  python3 scripts/verify_win_crt.py --tree <추출된 설치 트리 또는 target/release>  [--sevenzip 7zz]
"""

import argparse
import os
import struct
import subprocess
import sys
import tempfile

# Windows 콘솔 기본 cp1252 에서 한국어 판정 출력이 UnicodeEncodeError 로 죽으면 게이트가
# 판정 이전에 crash 한다(상류 run 30311769077 실측 · 0623a0f 백포트). 워크플로우 env(PYTHONUTF8)와
# 별개로 스크립트 자체에서도 방어한다 — 출력 인코딩 문제로 게이트가 죽는 일은 없어야 한다.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

# 정책 회귀를 무조건 차단하는 우리 바이너리(경로 말단 이름, 소문자).
STRICT_BINARIES = {"cys.exe", "cysd.exe", "cys-app.exe"}

NEEDLE = "vcruntime140"  # vcruntime140.dll·vcruntime140_1.dll 모두 접두 매치


def imports_of(path):
    """PE 파일이 임포트하는 DLL 이름 목록. PE 아니면 None(호출측 fail-closed)."""
    with open(path, "rb") as f:
        data = f.read()
    if len(data) < 0x40 or data[:2] != b"MZ":
        return None
    e_lfanew = struct.unpack_from("<I", data, 0x3C)[0]
    if e_lfanew + 24 > len(data) or data[e_lfanew:e_lfanew + 4] != b"PE\0\0":
        return None
    coff = e_lfanew + 4
    nsec = struct.unpack_from("<H", data, coff + 2)[0]
    opt_size = struct.unpack_from("<H", data, coff + 16)[0]
    opt = coff + 20
    magic = struct.unpack_from("<H", data, opt)[0]
    if magic == 0x20B:      # PE32+ (x64)
        dd_off = opt + 112
    elif magic == 0x10B:    # PE32 (32bit — pip 벤더 스텁 등)
        dd_off = opt + 96
    else:
        return None
    # ★경계 선검증(R3 codex high 수용): 선언된 optional header 가 DataDirectory[1](+8..+16)을
    # 담을 만큼 크고 파일 안에 실재해야 한다 — opt_size 를 속인 절단·조작 PE 는 파싱 실패(exit 3).
    if dd_off + 16 > opt + opt_size or opt + opt_size > len(data):
        return None
    ndd = struct.unpack_from("<I", data, dd_off - 4)[0]
    if ndd < 2:
        return []
    imp_rva, _ = struct.unpack_from("<II", data, dd_off + 8)  # DataDirectory[1] = Import Table
    if imp_rva == 0:
        return []
    secs = []
    sec0 = opt + opt_size
    # 섹션 테이블 전체가 파일 경계 안에 있어야 한다 — 밖이면 쓰레기 헤더로 오판정할 수 있다.
    if sec0 + 40 * nsec > len(data):
        return None
    for i in range(nsec):
        s = sec0 + 40 * i
        va = struct.unpack_from("<I", data, s + 12)[0]
        vsz = struct.unpack_from("<I", data, s + 8)[0]
        off = struct.unpack_from("<I", data, s + 20)[0]
        rsz = struct.unpack_from("<I", data, s + 16)[0]
        secs.append((va, max(vsz, rsz), off))

    def rva2off(rva):
        for va, sz, off in secs:
            if va <= rva < va + sz:
                return off + (rva - va)
        return None

    dlls = []
    off = rva2off(imp_rva)
    if off is None:
        # ★fail-closed 승격(R1 codex high 수용 · 2026-07-28): Import Directory 를 선언했는데
        # 그 RVA 가 어느 섹션에도 매핑되지 않으면 손상·비정상 PE 다. 종전 [](빈 임포트=통과)는
        # 조작·손상 PE 가 VCRUNTIME 임포트를 숨긴 채 PASS 하는 fail-open 구멍이었다 → 파싱
        # 실패(None)로 승격해 exit 3 경로에 태운다.
        return None
    terminated = False
    while off + 20 <= len(data):
        ilt, _, _, name_rva, iat = struct.unpack_from("<IIIII", data, off)
        if ilt == 0 and name_rva == 0 and iat == 0:
            terminated = True
            break
        noff = rva2off(name_rva)
        if noff is None:
            # nonzero descriptor 의 name_rva 미매핑도 동일 근거로 fail-closed — 조용히
            # 건너뛰면 그 항목의 DLL 이름(잠재적 vcruntime)이 검사에서 증발한다.
            return None
        end = data.index(b"\0", noff)
        dlls.append(data[noff:end].decode("ascii", "replace"))
        off += 20
    if not terminated:
        # ★R3 codex high 수용: zero terminator 없이 EOF 에 닿아 루프가 끝난 경우 — 절단·조작 PE 가
        # 뒷부분 descriptor(잠재적 vcruntime)를 자르고 '수집분만 정상'으로 통과하던 fail-open. None.
        return None
    return dlls


def check_imports(tree):
    """검사 A. 반환: (위반 목록, 파싱실패 목록, 허용 내역, 검사한 exe 수)."""
    violations, unparsed, allowed = [], [], []
    total = 0
    for dirpath, _, files in os.walk(tree):
        for fn in files:
            if not fn.lower().endswith(".exe"):
                continue
            total += 1
            p = os.path.join(dirpath, fn)
            rel = os.path.relpath(p, tree)
            try:
                dlls = imports_of(p)
            except Exception as e:  # 손상 PE 등 — fail-closed
                dlls = None
                print(f"  parse-error: {rel}: {e}")
            if dlls is None:
                unparsed.append(rel)
                continue
            if not any(NEEDLE in d.lower() for d in dlls):
                continue
            if fn.lower() in STRICT_BINARIES:
                violations.append((rel, "정책 바이너리 — 예외 불허(crt-static 회귀)"))
            elif os.path.isfile(os.path.join(dirpath, "vcruntime140.dll")):
                allowed.append(rel)
            else:
                violations.append((rel, "동봉 vcruntime140.dll 없음 — 클린 머신에서 기동 불능"))
    return violations, unparsed, allowed, total


def extract_setup(setup, sevenzip):
    tmp = tempfile.mkdtemp(prefix="cys-crt-gate-")
    cmd = [sevenzip, "x", setup, f"-o{tmp}", "-y"]
    r = subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
    if r.returncode != 0:
        print(f"NSIS 추출 실패({sevenzip} exit {r.returncode}): {r.stderr.strip()[:500]}", file=sys.stderr)
        sys.exit(4)
    return tmp


def main():
    ap = argparse.ArgumentParser(description="Windows CRT 발행 차단 게이트")
    src = ap.add_mutually_exclusive_group(required=True)
    src.add_argument("--setup", help="NSIS setup.exe (내부에서 7z 추출)")
    src.add_argument("--tree", help="이미 추출된 설치 트리")
    ap.add_argument("--sevenzip", default="7z", help="7-Zip 실행 파일 (기본 7z, macOS 는 7zz)")
    args = ap.parse_args()

    if args.setup:
        if not os.path.isfile(args.setup):
            print(f"setup 파일 없음: {args.setup}", file=sys.stderr)
            sys.exit(4)
        tree = extract_setup(args.setup, args.sevenzip)
        print(f"== 입력: {args.setup} (추출: {tree})")
    else:
        tree = args.tree
        if not os.path.isdir(tree):
            print(f"트리 없음: {tree}", file=sys.stderr)
            sys.exit(4)
        print(f"== 입력 트리: {tree}")

    violations, unparsed, allowed, total = check_imports(tree)

    print(f"== 임포트 스윕: exe {total}개 검사")
    for rel in allowed:
        print(f"  allow(app-local DLL 동봉): {rel}")
    for rel, why in violations:
        print(f"  VIOLATION: {rel} — {why}")
    for rel in unparsed:
        print(f"  UNPARSED(fail-closed): {rel}")
    if violations:
        print("결과: FAIL(1) — CRT 정책 위반. 릴리스 차단.")
        sys.exit(1)
    if unparsed:
        print("결과: FAIL(3) — PE 파싱 실패 존재(fail-closed).")
        sys.exit(3)
    print("결과: PASS — VCRUNTIME 자기완결.")
    sys.exit(0)


if __name__ == "__main__":
    main()
