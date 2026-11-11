#!/usr/bin/env python3
"""test_head_brief_injection.py — 0.3.4 A1 · 본부장 지침 주입 분기 핀 (기준 C2 · C3).

계약:
  C2  제목에 「본부장」 조각이 없는 좌석(평범한 worker · 그 밖의 모든 역할)의 session-start.sh 출력은
      6ebdafb 의 hook(fixtures/session-start.6ebdafb.sh)과 **바이트 동일**하다(stdout·exit code).
  본부장 좌석(role=worker + 제목 조각이 「본부장」으로 시작)은 6ebdafb 출력 **뒤에** HEAD_BRIEF.md 만 덧붙는다.
  판별 불가(cys list 실패·좌석 행 없음)·HEAD_BRIEF.md 부재 = 평범한 worker 와 동일(fail-closed).
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
        "    [ \"$STUB_LIST\" = fail ] && exit 1\n"
        "    printf 'surface:1\\trole=master\\tpid=1\\texited=false\\tmaster-claude\\t/tmp\\tlabel=\\n'\n"
        "    [ \"$STUB_LIST\" = norow ] && exit 0\n"
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
    check("C3 HEAD_BRIEF 맨 위에 주인님 정의문 원문",
          "부서마다 독립된 마스터 아니야. 부서장인데 부서장이 부서에서는 마스터와 같은 역할이지." in "\n".join(text.splitlines()[:12]))
else:
    check("C3 HEAD_BRIEF.md 실재", False, REAL_BRIEF)

print("\n%s — fails=%d" % ("OK" if not fails else "FAIL", len(fails)))
sys.exit(1 if fails else 0)
