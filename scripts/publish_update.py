#!/usr/bin/env python3
"""Wave Terminal 새 판 발행 점검 — 배포 장소 하나 = greatson79/wave-terminal 릴리스(승인 기록 판정 22:3x).

새 판 릴리스 하나에 앱·설치기·SHA256SUMS·latest.json·pack-manifest·steps.json 을 다 넣고,
waveainetworks.com/mac·/win 은 releases/latest/download/… 로 연결한다 → 그 릴리스를 Latest 로 올리는
한 동작에 설치 링크(새 수강생)와 Update 버튼(깔린 수강생)이 같이 바뀐다.

  python3 publish_update.py --version 0.2.1 --art <산출물 폴더> [--skip-cdhash] [--with-pack]   # 드라이런(기본 팩 제외): 검사·목록·해시·명령 출력
  python3 publish_update.py --check [--version 0.2.1]                              # GET 만: 공개된 두 입구 대조

★이 스크립트는 업로드·초안 생성·push 를 하지 않는다. 발행 명령은 출력만 한다(사람이 실행).
비밀키는 쓰지 않는다 — 서명은 미리 끝낸 .sig/.minisig 를 앱 내장 공개키로 검증만 한다.
"""
import argparse, base64, hashlib, json, os, re, subprocess, sys, tarfile, tempfile, time
from pathlib import Path

REPO = "greatson79/wave-terminal"
DL = f"https://github.com/{REPO}/releases/download"
LATEST = f"https://github.com/{REPO}/releases/latest/download"
HOME = {"/mac": "bootstrap.sh", "/win": "win-start.ps1"}
# rc.4 앱이 찾는 키(tauri-plugin-updater 2.10.1 updater.rs:568-600 get_urls "{os}-{arch}") → 묶음 이름 꼬리
ALL_PLATFORMS = {"darwin-aarch64": "macos-arm64.app.tar.gz", "windows-x86_64": "windows-x64-setup.exe"}
NEED = dict(ALL_PLATFORMS)  # main() 에서 --platforms 로 좁힘
fails = []


def bad(msg):
    fails.append(msg)
    print(f"  ✗ {msg}")


def ok(msg):
    print(f"  ✓ {msg}")


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def semver(v):
    m = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", v or "")
    return tuple(map(int, m.groups())) if m else None


def minisign_ok(pub, data, sig_text):
    with tempfile.NamedTemporaryFile("w", suffix=".minisig", delete=False) as f:
        f.write(sig_text)
    r = subprocess.run(["minisign", "-Vq", "-p", pub, "-m", str(data), "-x", f.name], capture_output=True)
    os.unlink(f.name)
    return r.returncode == 0


def cdhash(app):
    m = re.search(r"CDHash=(\w+)", subprocess.run(["codesign", "-dvvv", str(app)], capture_output=True, text=True).stderr)
    return m.group(1) if m else None


def install_side(a, art, V, tag):
    steps_raw = (art / "steps.json").read_bytes()
    steps = json.loads(steps_raw)
    rel = steps["release"]
    (ok if rel.get("version") == V else bad)(f"steps.json release.version = {V}(버튼과 같은 판)")
    for plat, name in rel["asset_name"].items():
        f = art / name
        if rel["asset_url"].get(plat) != f"{DL}/{tag}/{name}":
            bad(f"asset_url.{plat} 가 같은 릴리스({DL}/{tag}/)가 아님")
        if not f.is_file() or rel["sha256"].get(plat) != sha(f):
            bad(f"sha256.{plat} != 실제 {name}")
    cmd = json.dumps(steps.get("reinstall", {}), ensure_ascii=False)
    (ok if f"{LATEST}/bootstrap.sh" in cmd and f"{LATEST}/bootstrap.ps1" in cmd and "wave-install/releases" not in cmd else bad)(
        "함정3: reinstall.command = releases/latest/download/bootstrap.sh·ps1")
    tgz = next(art.glob("wave-install-*.tar.gz"), None)
    boot = (art / "bootstrap.sh").read_text()
    if not tgz:
        bad("설치팩 tar.gz 없음")
    else:
        with tarfile.open(tgz) as t:
            inner = next((t.extractfile(x).read() for x in t if x.name.endswith("/steps.json") and x.name.count("/") == 1), None)
        (ok if inner == steps_raw else bad)("설치팩 안 steps.json = 릴리스 자산 steps.json(rc.4 bootstrap 은 설치팩 안 것을 읽는다: bootstrap.sh:1186-1187)")
        (ok if f"{DL}/{tag}/{tgz.name}" in boot and sha(tgz) in boot else bad)("bootstrap.sh 가 같은 릴리스 설치팩 주소·sha256 고정(make-release.sh 주입)")
    (ok if f"{LATEST}/steps.json" in boot and "raw.githubusercontent.com/greatson79/wave-install" not in boot else bad)(
        "함정1: bootstrap.sh WAVE_INSTALL_STEPS_URL 기본값 = releases/latest/download/steps.json")
    ws = (art / "win-start.ps1").read_text()
    (ok if f"{DL}/{tag}/bootstrap.ps1" in ws and sha(art / "bootstrap.ps1") in ws else bad)(
        "함정2: win-start.ps1 = 같은 릴리스 bootstrap.ps1 주소(태그 고정)+sha256 고정(render_win_start.py) → /win 은 latest 로 가도 해시 사슬 유지")
    sums = dict(reversed(l.split()) for l in (art / "SHA256SUMS").read_text().splitlines() if l.strip())
    need = ["bootstrap.sh", "bootstrap.ps1", "win-start.ps1", "steps.json", *rel["asset_name"].values()] + ([tgz.name] if tgz else [])
    miss = [n for n in dict.fromkeys(need) if sums.get(n) != (sha(art / n) if (art / n).is_file() else None)]
    (ok if not miss else bad)(f"SHA256SUMS 가 실제 파일과 일치{'' if not miss else ' — 어긋남: ' + ', '.join(miss)}")
    return rel


def dryrun(a):
    art, V = Path(a.art), a.version
    tag = f"v{V}"
    print(f"■ 버전: 새 판 {V} · 태그 {tag}")
    if not semver(V):
        bad(f"버전은 X.Y.Z 만(updater semver·make-update-manifest 태그 규칙): {V}")
    elif semver(V) <= semver(a.current):
        bad(f"{V} ≤ 깔린 판 {a.current} — 버튼에 안 뜬다(updater.rs:532 release.version > current_version)")
    else:
        ok(f"{V} > 깔린 판 {a.current}")

    conf = json.loads((Path(a.src) / "src-tauri/tauri.conf.json").read_text())
    pubtxt = base64.b64decode(conf["plugins"]["updater"]["pubkey"]).decode()
    pub = tempfile.NamedTemporaryFile("w", suffix=".pub", delete=False)
    pub.write(pubtxt)
    pub.close()
    print(f"■ 앱 내장 공개키: {pubtxt.split()[-2]} (tauri.conf.json:49)")
    if conf["version"] != V:
        bad(f"소스 tauri.conf.json version {conf['version']} != {V}(버전 6곳 범프 확인)")

    print("■ Update 버튼 쪽(latest.json)")
    lj = json.loads((art / "latest.json").read_text())
    if lj.get("version") != V:
        bad(f"latest.json version {lj.get('version')} != {V}")
    extra = sorted(set(lj.get("platforms") or {}) - set(NEED))
    (ok if not extra else bad)(f"platforms = {sorted(NEED)} 만{'' if not extra else ' — 여분: ' + ', '.join(extra)}")
    for key, tail in NEED.items():
        p = (lj.get("platforms") or {}).get(key)
        name = f"wave-terminal-{V}-{tail}"
        if not p:
            bad(f"platforms.{key} 없음 — 그 OS 수강생은 '업데이트 확인 실패'(TargetsNotFound)")
            continue
        if p.get("url") != f"{DL}/{tag}/{name}":
            bad(f"{key} url != {DL}/{tag}/{name}")
        f = art / name
        if not f.is_file():
            bad(f"묶음 없음: {name}")
        elif minisign_ok(pub.name, f, base64.b64decode(p["signature"]).decode()):
            ok(f"{key} 서명 = 내장 공개키로 검증됨")
        else:
            bad(f"{key} 서명 검증 실패")

    pack = [art / n for n in ("pack.tar.gz", "pack-manifest.json", "pack-manifest.json.minisig")]
    print("■ 팩(pack-manifest)")
    if any(p.exists() for p in pack) and not a.with_pack:
        bad("이번 판은 팩 제외(승인 기록 2026-10-14 2250 판정 1) — 팩 3종을 폴더에서 빼거나, 다음 판부터 --with-pack")
    elif not any(p.exists() for p in pack):
        print("  · 팩 3종 없음 — 깔린 rc.4 앱은 본체 업데이트만 보고(updateplan.ts:39 binary), 수리판 이후 앱은 404 = '팩 없음'")
    elif not all(p.exists() for p in pack):
        bad("팩 3종은 셋 다 있거나 셋 다 없어야 한다")
    else:
        m = json.loads(pack[1].read_text())
        (ok if minisign_ok(pub.name, pack[1], pack[2].read_text()) else bad)("pack-manifest 서명(내장 공개키)")
        (ok if m.get("key_id") == "39E60A702949D6C3" else bad)("key_id = 키링 이름 39E60A702949D6C3(trusted-keys.json)")
        (ok if m.get("digest") == sha(pack[0]) else bad)("digest = sha256(pack.tar.gz)")
        mb = m.get("min_binary_version", "")
        (ok if not mb or semver(mb) and semver(mb) <= semver(V) else bad)(
            f"min_binary_version {mb!r} ≤ {V} (release.yml PACK_MIN_BINARY 0.12.48 그대로면 모든 0.2.x 앱에 '본체 필요')")
        (ok if m["signed_at"] <= time.time() <= m["expires_at"] else bad)("서명 유효창 안")

    print("■ 설치 링크 쪽(steps.json·설치기)")
    if a.button_only:
        print("  · 검사 안 함(--button-only) — 설치기 산출물은 이 폴더에 없다. 발행 전엔 빼고 다시 돌린다")
        rel = {"asset_name": {"macos_arm64": f"wave-terminal-{V}-macos-arm64.dmg"}, "cdhash": {}}
    else:
        rel = install_side(a, art, V, tag)

    print("■ 맥 앱 정합(cdhash)")
    if a.skip_cdhash:
        print("  · 건너뜀(--skip-cdhash) — 실제 발행 전엔 반드시 켠다")
    else:
        dmg, tar_gz = art / rel["asset_name"]["macos_arm64"], art / f"wave-terminal-{V}-{NEED['darwin-aarch64']}"
        with tempfile.TemporaryDirectory() as mnt, tempfile.TemporaryDirectory() as x:
            subprocess.run(["hdiutil", "attach", "-nobrowse", "-readonly", "-noverify", "-mountpoint", mnt, str(dmg)], check=True, capture_output=True)
            try:
                d = cdhash(next(Path(mnt).glob("*.app")))
            finally:
                subprocess.run(["hdiutil", "detach", mnt], capture_output=True)
            subprocess.run(["tar", "-xzf", str(tar_gz), "-C", x], check=True)
            u = cdhash(next(Path(x).glob("*.app")))
        pin = d if a.button_only else rel["cdhash"]["macos_arm64"]
        (ok if d and d == u == pin else bad)(
            f"DMG 앱 {d} = 업데이트 묶음 앱 {u}{'' if a.button_only else ' = steps.json 핀'} — 다르면 버튼으로 받은 앱이 설치기 재실행 때 '핀 불일치'(bootstrap.sh installed_app_matches_pin)")
    os.unlink(pub.name)

    files = sorted(p for p in art.iterdir() if p.is_file())
    print(f"\n■ 릴리스 {REPO} {tag} 에 올릴 자산 {len(files)}개 (sha256 · 바이트 · 이름)")
    for f in files:
        print(f"  {sha(f)}  {f.stat().st_size:>11}  {f.name}")
    print("\n■ 사람이 실행할 명령(★승인 기록 2차 → 주인님 실기 → 주인님 「올려」 뒤에만 · 이 스크립트는 실행 안 함)")
    print(f"  1) 초안 조립:  gh release create {tag} -R {REPO} --draft --title 'Wave Terminal {V}' --notes-file NOTES.md {art}/*")
    print(f"  2) 초안 자산 대조:  gh release view {tag} -R {REPO} --json assets --jq '.assets[]|[.name,.size]|@tsv'")
    print(f"  3) 발행(한 동작):  gh release edit {tag} -R {REPO} --draft=false --latest")
    print(f"  4) 끝 검사:  python3 {Path(__file__).name} --check --version {V} --repo {REPO}{' --button-only' if a.button_only else ''}")
    print(f"  되돌리기:  gh release edit {tag} -R {REPO} --draft=true   (Latest 가 직전 판으로 — 이미 받은 수강생은 그대로)")
    print("\n" + ("드라이런 PASS — 업로드 0" if not fails else f"드라이런 FAIL {len(fails)}건 — 업로드 0"))
    return not fails


def get(url, follow=True):
    args = ["curl", "-sS", "-r", "0-65535", "-o", "-", "-w", "\n%{http_code} %{redirect_url}", url]
    if follow:
        args.insert(1, "-L")
    out = subprocess.run(args, text=True, capture_output=True).stdout
    body, _, tail = out.rpartition("\n")
    code, _, loc = tail.partition(" ")
    return int(code or 0), body, loc


def check(a):
    """GET 만. 지금처럼 버튼 쪽만 404(또는 두 입구 판이 다름)이면 FAIL 로 잡힌다."""
    print("■ Update 버튼 입구")
    c, body, _ = get(f"{LATEST}/latest.json")
    bv = json.loads(body).get("version") if c == 200 else None
    (ok if c == 200 else bad)(f"latest.json {c} version={bv}")
    if c == 200:
        for key in NEED:
            url = (json.loads(body).get("platforms", {}).get(key) or {}).get("url", "")
            cc = get(url)[0] if url else 0
            (ok if cc in (200, 206) else bad)(f"{key} 묶음 {cc} {url}")
    c, _, _ = get(f"{LATEST}/pack-manifest.json")
    print(f"  · pack-manifest.json {c} (404 = 팩 없음 · 수리판 앱은 '팩 업데이트 없음')")
    if a.button_only:  # 시험 저장소 = 버튼 경로만(설치 링크·홈페이지 연결 0 · 승인 기록 2250 판정 4)
        if a.version:
            (ok if bv == a.version else bad)(f"기대 판 {a.version}")
        print("\n" + ("CHECK PASS(버튼만)" if not fails else f"CHECK FAIL {len(fails)}건"))
        return not fails
    print("■ 설치 링크 입구")
    c, body, _ = get(f"{LATEST}/steps.json")
    sv = json.loads(body)["release"]["version"] if c == 200 else None
    (ok if c == 200 else bad)(f"steps.json {c} release.version={sv}")
    for path, asset in HOME.items():
        c, _, loc = get(f"https://waveainetworks.com{path}", follow=False)
        (ok if loc == f"{LATEST}/{asset}" else bad)(f"waveainetworks.com{path} → {loc or c}")
        c2 = get(f"https://waveainetworks.com{path}")[0]
        (ok if c2 in (200, 206) else bad)(f"waveainetworks.com{path} 끝까지 {c2}")
    if bv and sv:
        (ok if bv == sv else bad)(f"두 입구 같은 판: 버튼 {bv} · 설치 {sv}")
    if a.version:
        (ok if bv == sv == a.version else bad)(f"기대 판 {a.version}")
    print("\n" + ("CHECK PASS" if not fails else f"CHECK FAIL {len(fails)}건"))
    return not fails


def main():
    global REPO, DL, LATEST, NEED
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=REPO, help="대상 릴리스 저장소(시험 = greatson79/wave-terminal-test)")
    ap.add_argument("--platforms", default="darwin-aarch64", help="latest.json 에 있어야 할 키(쉼표) · 이외 키 = FAIL(첫 판 = 맥만)")
    ap.add_argument("--button-only", action="store_true", help="설치기 쪽 검사 생략(버튼 경로 시험용 산출물)")
    ap.add_argument("--version")
    ap.add_argument("--art", help="릴리스에 올릴 파일만 담은 폴더")
    ap.add_argument("--current", default="0.2.0", help="수강생에게 깔린 앱 판(rc.4)")
    ap.add_argument("--src", default=str(Path(__file__).resolve().parents[1]), help="앱 소스(기본 = 이 저장소)")
    ap.add_argument("--skip-cdhash", action="store_true")
    ap.add_argument("--with-pack", action="store_true", help="팩 3종도 올림(기본 = 팩 제외 · 첫 판 판정)")
    ap.add_argument("--check", action="store_true")
    a = ap.parse_args()
    REPO = a.repo
    DL, LATEST = f"https://github.com/{REPO}/releases/download", f"https://github.com/{REPO}/releases/latest/download"
    NEED = {k: ALL_PLATFORMS[k] for k in a.platforms.split(",")}
    if a.check:
        sys.exit(0 if check(a) else 1)
    if not (a.version and a.art):
        sys.exit("--version --art 필요")
    sys.exit(0 if dryrun(a) else 1)


if __name__ == "__main__":
    main()
