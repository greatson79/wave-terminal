#!/usr/bin/env python3
"""test_head_brief_injection.py — 0.3.4 A1 · 본부장 지침 주입 분기 핀 (기준 C2 · C3).

계약:
  C2  제목에 「본부장」 조각이 없는 좌석(평범한 worker · 그 밖의 모든 역할)의 session-start.sh 출력은
      6ebdafb 의 hook(fixtures/session-start.6ebdafb.sh)과 **바이트 동일**하다(stdout·exit code).
  본부장 좌석(role=worker + 제목 조각이 「본부장」으로 시작)은 6ebdafb 출력 **뒤에** HEAD_BRIEF.md 만 덧붙는다.
  판별 불가(cys 없음·cys list 비0 종료·부분 출력·깨진/필드 부족 행·내 행 없음) = stdout 은 평범한 worker 와 동일
  (fail-closed)이고, **stderr 에 한 줄**로 드러낸다(N1). 정상 경로(평범한 worker·본부장)의 stderr 는 비어 있다.
  `cys list` 출력 계약 = src/bin/cys.rs:1534-1546 의 7열 탭: 좌석 · role= · pid= · exited= · 제목 · cwd · label=.
  C3  HEAD_BRIEF.md 는 텍스트만: 개인 경로·명부·wave-spawn/wave-send·새 역할(--role head)이 없다.
  음성 대조군: hook 에 빈 줄 하나만 덧붙인 변조본은 C2 비교가 **반드시 실패**해야 한다(시험이 무는지 증명).

HOOK_UNDER_TEST=<경로> 로 다른 hook 을 시험할 수 있다(의도적 실패 증명용).
"""
import os
import shutil
import subprocess
import sys
import tempfile

SELF = os.path.dirname(os.path.abspath(__file__))
PACK = os.path.abspath(os.path.join(SELF, "..", ".."))
NEW_HOOK = os.environ.get("HOOK_UNDER_TEST") or os.path.join(PACK, "hooks", "session-start.sh")
OLD_HOOK = os.path.join(SELF, "fixtures", "session-start.6ebdafb.sh")
REAL_BRIEF = os.path.join(PACK, "directives", "HEAD_BRIEF.md")
fails = []


def check(name, cond, detail=""):
    print("%s %s%s" % ("PASS" if cond else "FAIL", name, (" — " + detail) if detail else ""))
    if not cond:
        fails.append(name)


def make_env(tmp, title, list_mode="ok", with_brief=True):
    pack = os.path.join(tmp, "pack")
    bindir = os.path.join(tmp, "stubbin")
    os.makedirs(os.path.join(pack, "directives"), exist_ok=True)
    os.makedirs(os.path.join(pack, "memory"), exist_ok=True)
    os.makedirs(bindir, exist_ok=True)
    for d in ("MASTER", "WORKER", "CSO", "REVIEWER"):
        with open(os.path.join(pack, "directives", "%s_DIRECTIVE.md" % d), "w", encoding="utf-8") as f:
            f.write("DIRECTIVE-BODY-%s\n" % d)
    with open(os.path.join(pack, "soul.md"), "w", encoding="utf-8") as f:
        f.write("SOUL-BODY\n")
    with open(os.path.join(pack, "memory", "MEMORY.md"), "w", encoding="utf-8") as f:
        f.write("MEMORY-BODY\n")
    if with_brief:
        with open(os.path.join(pack, "directives", "HEAD_BRIEF.md"), "w", encoding="utf-8") as f:
            f.write("HEAD-BRIEF-BODY\n")
    stub = (
        "#!/bin/sh\n"
        "case \"$1\" in\n"
        "  claim-role) exit 0;;\n"
        "  list)\n"
        "    case \"$STUB_LIST\" in\n"
        "      fail) exit 1;;\n"
        "      norow) printf 'surface:1\\trole=master\\tpid=1\\texited=false\\tmaster-claude\\t/tmp\\tlabel=\\n'; exit 0;;\n"
        "      partial_fail) printf 'surface:3\\trole=worker\\tpid=2\\texited=false\\t%s\\t/tmp\\tlabel=\\n' \"$STUB_TITLE\"; exit 3;;\n"
        "      malformed) printf 'surface:3\\trole=worker\\t%s\\n' \"$STUB_TITLE\"; exit 0;;\n"
        "      missingfield) printf 'surface:3\\trole=worker\\tpid=2\\texited=false\\t%s\\t/tmp\\n' \"$STUB_TITLE\"; exit 0;;\n"
        "      badtag) printf 'surface:3\\tworker\\t2\\tfalse\\t%s\\t/tmp\\t\\n' \"$STUB_TITLE\"; exit 0;;\n"
        "      garbage) printf 'not a table\\n'; exit 0;;\n"
        "    esac\n"
        "    printf 'surface:1\\trole=master\\tpid=1\\texited=false\\tmaster-claude\\t/tmp\\tlabel=\\n'\n"
        "    printf 'surface:3\\trole=worker\\tpid=2\\texited=false\\t%s\\t/tmp\\tlabel=\\n' \"$STUB_TITLE\"\n"
        "    exit 0;;\n"
        "esac\nexit 0\n"
    )
    with open(os.path.join(bindir, "cys"), "w", encoding="utf-8", newline="\n") as f:
        f.write(stub)
    os.chmod(os.path.join(bindir, "cys"), 0o755)
    env = dict(os.environ)
    env.update({"CYS_PACK_DIR": pack, "CYS_SURFACE_ID": "3", "CYS_SURFACE_REF": "surface:3",
                "STUB_TITLE": title, "STUB_LIST": list_mode,
                "PATH": bindir + os.pathsep + env.get("PATH", "")})
    env.pop("CYS_ROLE", None)
    return env


def no_cys_env(env, tmp):
    """cys 가 PATH 에 없는 환경(python3 는 남김)."""
    only = os.path.join(tmp, "nocysbin")
    os.makedirs(only, exist_ok=True)
    link = os.path.join(only, "python3")
    if not os.path.exists(link):
        os.symlink(sys.executable, link)
    e = dict(env)
    e["PATH"] = only + os.pathsep + "/usr/bin" + os.pathsep + "/bin"
    return e


def run(hook, env, role):
    e = dict(env)
    if role:
        e["CYS_ROLE"] = role
    r = subprocess.run(["sh", hook], capture_output=True, env=e, stdin=subprocess.DEVNULL, timeout=30)
    return r.returncode, r.stdout, r.stderr


def same_as_old(tmp_label, title, role, list_mode="ok", with_brief=True, hook=None):
    """새 hook 과 6ebdafb hook 의 (exit code, stdout 바이트)가 같은가."""
    with tempfile.TemporaryDirectory() as tmp:
        env = make_env(tmp, title, list_mode, with_brief)
        o = run(OLD_HOOK, env, role)
        n = run(hook or NEW_HOOK, env, role)
        return (o[0], o[1]) == (n[0], n[1]), o, n


# ── C2: 평범한 좌석 = 6ebdafb 와 바이트 동일 ──
PLAIN_TITLES = ["Mina·연구본부·워커A", "worker-claude · my-folder", "", "Mina·연구본부·본부장님의 친구 아님"]
for t in PLAIN_TITLES[:3]:
    ok, o, n = same_as_old("plain", t, "worker")
    check("C2 평범한 worker 바이트 동일 · 제목=%r" % t, ok, "" if ok else "old=%d bytes new=%d bytes" % (len(o[1]), len(n[1])))
for role in ("master", "cso", "reviewer", None):
    ok, o, n = same_as_old("role", "Mina·연구본부·본부장", role)
    check("C2 role=%s 는 본부장 제목이어도 바이트 동일" % role, ok)
ok, o, n = same_as_old("fail", "Mina·연구본부·본부장", "worker", list_mode="fail")
check("C2 cys list 실패 → 평범한 worker 와 동일(fail-closed)", ok)
ok, o, n = same_as_old("norow", "Mina·연구본부·본부장", "worker", list_mode="norow")
check("C2 내 좌석 행 없음 → 평범한 worker 와 동일", ok)
ok, o, n = same_as_old("nobrief", "Mina·연구본부·본부장", "worker", with_brief=False)
check("C2 HEAD_BRIEF.md 없음 → 평범한 worker 와 동일", ok)

# ── 본부장 좌석: 6ebdafb 출력 + 덧붙임 ──
for t in ("Mina·연구본부·본부장", "본부장·s208", "Mina·연구본부·본부장·s208·opus"):
    with tempfile.TemporaryDirectory() as tmp:
        env = make_env(tmp, t)
        o = run(OLD_HOOK, env, "worker")
        n = run(NEW_HOOK, env, "worker")
    starts = n[1].startswith(o[1])
    tail = n[1][len(o[1]):] if starts else b""
    check("본부장 좌석 %r: 6ebdafb 출력으로 시작" % t, starts and n[0] == 0)
    check("본부장 좌석 %r: 덧붙임에 HEAD_BRIEF 본문" % t, b"HEAD-BRIEF-BODY" in tail)
    check("본부장 좌석 %r: 덧붙임은 HEAD_BRIEF 뿐(다른 지침 중복 없음)" % t,
          tail.count(b"DIRECTIVE-BODY-WORKER") == 0 and tail.count(b"SOUL-BODY") == 0)

# ── N1: 정상 경로의 stderr 는 6ebdafb 와 같다(비어 있다) ──
for t in ("Mina·연구본부·워커A", "Mina·연구본부·본부장"):
    with tempfile.TemporaryDirectory() as tmp:
        env = make_env(tmp, t)
        o = run(OLD_HOOK, env, "worker")
        n = run(NEW_HOOK, env, "worker")
    check("N1 정상 경로 stderr 동일 · 제목=%r" % t, o[2] == n[2] == b"", repr(n[2][:80]))

# ── N1: 판별 불가 = stdout 은 6ebdafb 와 동일 + stderr 한 줄(보이게) ──
WARN = "[session-start] 본부장 제목 확인 불가".encode("utf-8")
for mode in ("fail", "partial_fail", "malformed", "missingfield", "badtag", "garbage", "norow"):
    for t in ("Mina·연구본부·워커A", "Mina·연구본부·본부장"):
        with tempfile.TemporaryDirectory() as tmp:
            env = make_env(tmp, t, list_mode=mode)
            o = run(OLD_HOOK, env, "worker")
            n = run(NEW_HOOK, env, "worker")
        same = (o[0], o[1]) == (n[0], n[1])
        one = n[2].count(WARN) == 1 and n[2].count(b"\n") == 1
        check("N1 list=%s 제목=%r → stdout 동일" % (mode, t), same)
        check("N1 list=%s 제목=%r → stderr 한 줄 경고" % (mode, t), one, repr(n[2][:100]))
with tempfile.TemporaryDirectory() as tmp:
    env = no_cys_env(make_env(tmp, "Mina·연구본부·본부장"), tmp)
    o = run(OLD_HOOK, env, "worker")
    n = run(NEW_HOOK, env, "worker")
    check("N1 cys 없음 → stdout 동일", (o[0], o[1]) == (n[0], n[1]))
    check("N1 cys 없음 → stderr 한 줄 경고", n[2].count(WARN) == 1 and n[2].count(b"\n") == 1, repr(n[2][:100]))
with tempfile.TemporaryDirectory() as tmp:
    env = make_env(tmp, "Mina·연구본부·본부장", list_mode="partial_fail")
    n = run(NEW_HOOK, env, "worker")
    check("N1 비0 종료+부분 출력 → 본부장 지침 주입 안 함(부분 출력 불신)", b"HEAD-BRIEF-BODY" not in n[1] and b"exit 3" in n[2], repr(n[2][:100]))
# 본부장인데 지침 파일이 없으면 그것도 보인다(stdout 은 동일)
with tempfile.TemporaryDirectory() as tmp:
    env = make_env(tmp, "Mina·연구본부·본부장", with_brief=False)
    o = run(OLD_HOOK, env, "worker")
    n = run(NEW_HOOK, env, "worker")
    check("N1 본부장+지침 파일 없음 → stdout 동일 · stderr 한 줄", (o[0], o[1]) == (n[0], n[1]) and n[2].count(b"HEAD_BRIEF.md") == 1, repr(n[2][:100]))

# ── N2: 스킬의 naming.json 등록 조각 — 병합(기존 필드·다른 역할 보존) ──
import json
skill = open(os.path.join(PACK, "skills", "department-setup", "SKILL.md"), encoding="utf-8").read()
start = skill.index("<<'PY'\n") + len("<<'PY'\n")
code = skill[start:skill.index("\nPY\n", start)]


def run_snip(home, *args):
    e = dict(os.environ, HOME=home, USERPROFILE=home)
    r = subprocess.run([sys.executable, "-I", "-c", code, *args], env=e, capture_output=True, text=True, encoding="utf-8")
    return r.returncode, r.stdout, r.stderr


with tempfile.TemporaryDirectory() as home:
    os.makedirs(os.path.join(home, ".cys"))
    npath = os.path.join(home, ".cys", "naming.json")
    existing = {"roles": {"master": {"name": "Mina", "label": "CEO", "note": "keep-me"},
                          "worker-2": {"name": "Old", "label": "옛본부·워커", "extra": 7}},
                "other": {"x": 1}}
    json.dump(existing, open(npath, "w", encoding="utf-8"), ensure_ascii=False)
    rc, out, err = run_snip(home, "worker-2", "Mina", "연구본부·본부장")
    d = json.load(open(npath, encoding="utf-8"))
    check("N2 등록 성공", rc == 0, err)
    check("N2 다른 역할 보존", d["roles"]["master"] == existing["roles"]["master"] and d["other"] == {"x": 1})
    check("N2 같은 역할의 기존 필드 보존(extra) + 이름·라벨만 갱신",
          d["roles"]["worker-2"] == {"name": "Mina", "label": "연구본부·본부장", "extra": 7}, str(d["roles"]["worker-2"]))
    rc, out, err = run_snip(home, "worker-3", "Mina", "연구본부·CSO")
    d2 = json.load(open(npath, encoding="utf-8"))
    check("N2 새 역할 추가해도 앞선 등록 유지", rc == 0 and d2["roles"]["worker-2"] == d["roles"]["worker-2"] and "worker-3" in d2["roles"])
    open(npath, "w", encoding="utf-8").write('{"roles": []}')
    rc, out, err = run_snip(home, "worker-4", "Mina", "연구본부·워커")
    check("N2 roles 가 객체가 아니면 덮어쓰지 않음", rc != 0 and open(npath, encoding="utf-8").read() == '{"roles": []}', err)
    open(npath, "w", encoding="utf-8").write('{"roles": {"worker-4": "oops"}}')
    rc, out, err = run_snip(home, "worker-4", "Mina", "연구본부·워커")
    check("N2 기존 항목이 객체가 아니면 덮어쓰지 않음", rc != 0 and open(npath, encoding="utf-8").read() == '{"roles": {"worker-4": "oops"}}', err)

# ── 스킬 문장 핀 (T2 · N3 · N4) ──
check("T2 스킬: 살아 있는 리뷰어는 다시 띄우지 않고 공유", "리뷰어1·2 = 기존 좌석 공유" in skill and "다시 띄우지 않는다" in skill)
check("N3 스킬: 모델 폴백은 묻고 「예」일 때만", "「예」" in skill and "묻지 않고 바꾸지 않고" in skill)
check("N4 스킬: 재시작 없는 묶음을 주장하지 않는다(미확인으로 적는다)", "다시 읽어 와야" in skill and "확인되지 않음" in skill)

# ── 음성 대조군: 변조본(빈 줄 1개 추가)은 C2 비교가 실패해야 한다 ──
with tempfile.TemporaryDirectory() as tmp:
    mutated = os.path.join(tmp, "mutated-hook.sh")
    src = open(NEW_HOOK, encoding="utf-8").read().rstrip("\n").split("\n")
    # 마지막 exit 0 직전에 무조건 빈 줄 1개 출력
    idx = max(i for i, line in enumerate(src) if line.strip() == "exit 0")
    src.insert(idx, "echo")
    open(mutated, "w", encoding="utf-8", newline="\n").write("\n".join(src) + "\n")
    ok, o, n = same_as_old("mutant", "Mina·연구본부·워커A", "worker", hook=mutated)
    check("음성 대조군: 빈 줄 하나 덧붙인 변조본은 C2 비교에서 걸린다", not ok)

# ── C3: HEAD_BRIEF.md 텍스트 검사(실제 파일) ──
if os.path.isfile(REAL_BRIEF):
    text = open(REAL_BRIEF, encoding="utf-8").read()
    for bad in ("/Users/", "C:\\Users", "wave-spawn", "wave-send", "tower_roster", "--role head", "cmux", "Ai_works"):
        check("C3 HEAD_BRIEF 에 %r 없음" % bad, bad not in text)
    check("C3 HEAD_BRIEF 3행 = 중립 정의문(T1)",
          text.splitlines()[2] == "정의: 「본부장은 부서마다 따로 있는 독립된 마스터가 아니다. 부서장이며, 부서 안에서는 마스터와 같은 역할을 한다.」")
    check("C3 HEAD_BRIEF 에 말투 원문(주인님 구어체) 없음(T1)", "마스터 아니야" not in text and "같은 역할이지" not in text)
else:
    check("C3 HEAD_BRIEF.md 실재", False, REAL_BRIEF)

print("\n%s — fails=%d" % ("OK" if not fails else "FAIL", len(fails)))
sys.exit(1 if fails else 0)
