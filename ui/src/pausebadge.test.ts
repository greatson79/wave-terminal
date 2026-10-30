// pausebadge.ts 순수 함수 회귀 테스트 (bun test — 신규 의존성 0).
//
// [B7] 배지의 핵심은 "멈췄다"가 아니라 **"얼마나 오래 멈춰 있었나"**다. 정적 표시는 방치를
// 못 드러낸다 — 그래서 임계 승급이 실제로 일어나는지를 잰다(임계가 죽으면 배지는 있으나
// 마나 방치 경보가 없는 상태가 되고, 그게 2026-08-04 사건의 재발이다).
import { describe, it, expect } from "bun:test";
import { readFileSync } from "node:fs";
import {
  fmtSecs,
  pauseView,
  pauseViewFrom,
  labelForSocket,
  pauseBadge,
  socketLabelMap,
  worstPauseBadge,
  MAIN_DAEMON_LABEL,
  PAUSE_ABANDONED_SECS,
  PAUSE_STALE_SECS,
} from "./pausebadge";

const NOW = 1_800_000_000;

describe("pauseBadge — 정지 아님", () => {
  it("paused=false 면 배지 없음(빈 텍스트)", () => {
    const b = pauseBadge(false, { since: NOW - 99999, reason: "무시돼야 함" }, NOW);
    expect(b.level).toBe("none");
    expect(b.text).toBe("");
    expect(b.elapsedSecs).toBeNull();
  });
});

describe("pauseBadge — 경과 시간에 따른 등급 승급", () => {
  it("방금 정지 = paused(⛔) + 사유·경과 표시", () => {
    const b = pauseBadge(true, { since: NOW - 90, reason: "배포 게이트 대기" }, NOW);
    expect(b.level).toBe("paused");
    expect(b.text).toContain("⛔");
    expect(b.text).toContain("배포 게이트 대기");
    expect(b.text).toContain("1m30s 경과");
    expect(b.reasonMissing).toBe(false);
  });

  it("1시간 경계에서 stale(⚠️)로 승급 — 경계값 포함", () => {
    const just = pauseBadge(true, { since: NOW - (PAUSE_STALE_SECS - 1), reason: "r" }, NOW);
    expect(just.level).toBe("paused");
    const at = pauseBadge(true, { since: NOW - PAUSE_STALE_SECS, reason: "r" }, NOW);
    expect(at.level).toBe("stale");
    expect(at.text).toContain("⚠️");
    expect(at.text).toContain("해제하거나 사유를 갱신하라");
  });

  it("6시간 경계에서 abandoned(🚨)로 승급 — 방치 경보", () => {
    const just = pauseBadge(true, { since: NOW - (PAUSE_ABANDONED_SECS - 1), reason: "r" }, NOW);
    expect(just.level).toBe("stale");
    const at = pauseBadge(true, { since: NOW - PAUSE_ABANDONED_SECS, reason: "r" }, NOW);
    expect(at.level).toBe("abandoned");
    expect(at.text).toContain("🚨");
    expect(at.text).toContain("방치 여부를 지금 판단하라");
  });
});

describe("pauseBadge — 사유 결손을 숨기지 않는다", () => {
  it("빈 사유·공백 사유는 reasonMissing + 그 사실을 문구로 노출", () => {
    for (const reason of ["", "   ", null, undefined]) {
      const b = pauseBadge(true, { since: NOW - 60, reason }, NOW);
      expect(b.reasonMissing).toBe(true);
      expect(b.text).toContain("사유 미기록");
    }
  });

  it("since 미상이면 경과를 지어내지 않는다(null · 문구에 '경과' 없음)", () => {
    const b = pauseBadge(true, { reason: "사유는 있음" }, NOW);
    expect(b.elapsedSecs).toBeNull();
    expect(b.text).not.toContain("경과");
    expect(b.level).toBe("paused");
  });

  it("pause_info 자체가 없어도 정지 사실은 표시한다", () => {
    const b = pauseBadge(true, null, NOW);
    expect(b.level).toBe("paused");
    expect(b.text).toContain("PAUSED");
    expect(b.reasonMissing).toBe(true);
  });

  it("미래 since(시계 역행)는 음수 경과로 새지 않는다", () => {
    const b = pauseBadge(true, { since: NOW + 500, reason: "r" }, NOW);
    expect(b.elapsedSecs).toBe(0);
    expect(b.level).toBe("paused");
  });
});

// ── ★B7-r2(노아 REVISE major-2 반영): 멀티소켓 집계에서 정지 대상을 잃지 않는다 ──
describe("worstPauseBadge — 어느 데몬이 멈췄는지와 무엇을 풀어야 하는지", () => {
  const mk = (opts: {
    paused?: boolean;
    ago?: number | null;
    socket: string;
    label?: string | null;
    reason?: string;
  }) =>
    pauseBadge(
      opts.paused ?? true,
      { since: opts.ago === null ? null : NOW - (opts.ago ?? 60), reason: opts.reason ?? "r" },
      NOW,
      { socket: opts.socket, label: opts.label ?? null },
    );

  it("정지 없음만 있으면 null", () => {
    expect(worstPauseBadge([mk({ paused: false, socket: "/a" })])).toBeNull();
    expect(worstPauseBadge([])).toBeNull();
  });

  it("등급이 다르면 더 심각한 쪽 — 순회 순서와 무관", () => {
    const mild = mk({ ago: 60, socket: "/base" });
    const bad = mk({ ago: PAUSE_ABANDONED_SECS + 5, socket: "/dept" });
    expect(worstPauseBadge([mild, bad])?.socket).toBe("/dept");
    expect(worstPauseBadge([bad, mild])?.socket).toBe("/dept");
  });

  it("★동률이면 가장 오래된 쪽 — 종전엔 순회 순서가 결정했다", () => {
    const older = mk({ ago: PAUSE_ABANDONED_SECS + 3600, socket: "/older" });
    const newer = mk({ ago: PAUSE_ABANDONED_SECS + 60, socket: "/newer" });
    expect(worstPauseBadge([newer, older])?.socket).toBe("/older");
    expect(worstPauseBadge([older, newer])?.socket).toBe("/older");
  });

  it("경과 미상은 동률에서 최하위 — 모르는 것을 가장 오래된 것으로 대접하지 않는다", () => {
    const unknown = mk({ ago: null, socket: "/unknown" });
    const known = mk({ ago: 60, socket: "/known" });
    expect(worstPauseBadge([unknown, known])?.socket).toBe("/known");
    expect(worstPauseBadge([known, unknown])?.socket).toBe("/known");
    // 미상 단독이면 그것이라도 보여준다(정지 사실은 숨기지 않는다)
    expect(worstPauseBadge([unknown])?.socket).toBe("/unknown");
  });

  it("표시명이 배지 문구에 실린다 — 화면만 보고 어디인지 안다", () => {
    const b = mk({ ago: 60, socket: "/dept", label: "개발본부" });
    expect(b.text).toContain("[개발본부]");
    expect(b.socket).toBe("/dept");
  });
});

// ── ★B7-r3(노아 REVISE major-1): 복붙 명령은 실행 가능한 문장이다 ────────────────

// ── ★B7-r3(노아 REVISE major-2): 표시명 권위는 레지스트리다 ─────────────────────
describe("socketLabelMap / labelForSocket — 대상을 지어내지 않는다", () => {
  const depts = [
    { name: "_hq", display_name: "본부 · CEO", socket: "/main/cysd.sock" },
    { name: "dev", display_name: "개발본부", socket: "/dept/dev.sock" },
    { name: "pastoral", display_name: null, socket: "/dept/pastoral.sock" },
    { name: "nosock", display_name: "무소켓", socket: null },
  ];

  it("display_name 우선, 없으면 registry name", () => {
    const m = socketLabelMap(depts);
    expect(m.get("/dept/dev.sock")).toBe("개발본부");
    expect(m.get("/dept/pastoral.sock")).toBe("pastoral");
    expect(m.has("")).toBe(false);
  });

  it("★기본 데몬(socket 미상)은 특정 워크스페이스를 지어내지 않는다", () => {
    const m = socketLabelMap(depts);
    // 본부 소켓을 알면 그 등록명
    expect(labelForSocket(undefined, m, "/main/cysd.sock")).toBe("본부 · CEO");
    // 모르면 고정 라벨 — 첫 탭 이름을 빌려오지 않는다(배열 순서 의존 제거)
    expect(labelForSocket(undefined, m, null)).toBe(MAIN_DAEMON_LABEL);
    expect(labelForSocket("", m, "/없는/소켓")).toBe(MAIN_DAEMON_LABEL);
  });

  it("미등록 소켓은 소켓 경로 자체를 보여준다(정직 — 라벨을 창작하지 않는다)", () => {
    const m = socketLabelMap(depts);
    expect(labelForSocket("/dept/unknown.sock", m)).toBe("/dept/unknown.sock");
  });

  it("레지스트리 부재·빈 배열에서도 죽지 않는다", () => {
    expect(socketLabelMap(null).size).toBe(0);
    expect(labelForSocket("/x", socketLabelMap(undefined))).toBe("/x");
  });
});

describe("fmtSecs — CLI fmt_secs 와 같은 표기", () => {
  it("초/분/시 버킷", () => {
    expect(fmtSecs(40)).toBe("40s");
    expect(fmtSecs(125)).toBe("2m5s");
    expect(fmtSecs(3600)).toBe("1h0m");
    expect(fmtSecs(11532)).toBe("3h12m");
  });
  it("음수는 0초로 접는다", () => {
    expect(fmtSecs(-5)).toBe("0s");
  });
});

// ── ★B7-r4(노아 REVISE major ㉠): PowerShell 큰따옴표는 expandable ──────────────

// ── ★B7-r4(major ㉡): 소비자 배선까지 잰다 ────────────────────────────────────
describe("pauseView — 오케스트레이션(순수)", () => {
  const labels = new Map([["/dept/dev.sock", "개발본부"], ["/main.sock", "본부 · CEO"]]);
  const st = (socket: string | null, ago: number) => ({
    socket,
    paused: true,
    pause_info: { since: NOW - ago, reason: "게이트 대기" },
  });

  it("정지 없음 → 배지 null · tooltip 빈 문자열", () => {
    const v = pauseView([{ socket: "/a", paused: false }], labels, NOW);
    expect(v.badge).toBeNull();
    expect(v.tooltip).toBe("");
  });

  it("여러 소켓 중 가장 심각·가장 오래된 것 + 권위 라벨 + 해제 명령이 tooltip 에 실린다", () => {
    const v = pauseView([st("/main.sock", 60), st("/dept/dev.sock", PAUSE_ABANDONED_SECS + 999)], labels, NOW, "/main.sock");
    expect(v.badge?.socket).toBe("/dept/dev.sock");
    expect(v.badge?.level).toBe("abandoned");
    expect(v.tooltip).toContain("개발본부");
    // [B7-r5] tooltip 은 이제 **실행 문자열을 담지 않는다** — 셸 인용으로는 cmd 의 %VAR% 를
    // 막을 수 없어 복붙 설계 자체를 버렸다. 해제는 배지 클릭(구조화 IPC)이다.
    expect(v.tooltip).toContain("클릭");
  });

  it("★어떤 플랫폼에서도 tooltip 에 실행 가능한 셸 문자열이 없다", () => {
    const v = pauseView([st("/dept/dev.sock", 60)], labels, NOW);
    for (const forbidden of ["cys resume --socket", "CYS_SOCKET=", "PowerShell:", "cmd:"]) {
      expect(v.tooltip).not.toContain(forbidden);
    }
  });

  it("미등록 소켓은 라벨을 창작하지 않는다(소켓 경로 표시)", () => {
    const v = pauseView([st("/unknown.sock", 60)], labels, NOW);
    expect(v.badge?.label).toBe("/unknown.sock");
  });
});


// ── ★B7-r5(노아 REVISE major ㉡): 수집 경계를 결과로 잰다 ────────────────────
// r4 는 main.ts 원문 substring 을 봤다. 그래서 IS_WINDOWS 를 false 로 고정해도,
// refreshFleetLabels 를 첫 workspace 이름 맵으로 바꿔도 355건이 전부 초록이었다.
// 여기서는 org_fleet 응답과 소켓별 org.status 를 **입력으로 넣고 최종 PauseView 를 단언**한다.
describe("pauseViewFrom — org_fleet 입력에서 최종 화면까지", () => {
  const fleet = {
    departments: [
      { name: "_hq", display_name: "본부 · CEO", socket: "/main.sock" },
      { name: "dev", display_name: "개발본부", socket: "/dept/dev.sock" },
      { name: "late", display_name: "신설본부", socket: "/dept/late.sock" },
    ],
  };
  const st = (socket: string | null, ago: number) => ({
    socket,
    paused: true,
    pause_info: { since: NOW - ago, reason: "게이트 대기" },
  });

  it("★권위 display_name 이 화면 문구에 실린다(워크스페이스 이름을 빌리지 않는다)", () => {
    const v = pauseViewFrom(fleet, [st("/dept/dev.sock", 60)], NOW);
    expect(v.badge?.label).toBe("개발본부");
    expect(v.badge?.text).toContain("[개발본부]");
    expect(v.tooltip).toContain("개발본부");
  });

  it("★기본 데몬(socket null)은 _hq 등록명 — 첫 workspace 를 지어내지 않는다", () => {
    const v = pauseViewFrom(fleet, [st(null, 60)], NOW);
    expect(v.badge?.label).toBe("본부 · CEO");
  });

  it("★세션 중 등록된 새 부서가 스냅샷에 들어오면 즉시 이름으로 뜬다(㉢ 캐시 무효화의 관측면)", () => {
    const before = pauseViewFrom({ departments: fleet.departments.slice(0, 2) },
      [st("/dept/late.sock", 60)], NOW);
    expect(before.badge?.label).toBe("/dept/late.sock"); // 미등록 → 소켓 경로(정직)
    const after = pauseViewFrom(fleet, [st("/dept/late.sock", 60)], NOW);
    expect(after.badge?.label).toBe("신설본부");
  });

  it("여러 소켓 중 가장 심각·동률이면 가장 오래된 것", () => {
    const v = pauseViewFrom(
      fleet,
      [st("/main.sock", PAUSE_ABANDONED_SECS + 10), st("/dept/dev.sock", PAUSE_ABANDONED_SECS + 9999)],
      NOW,
    );
    expect(v.badge?.socket).toBe("/dept/dev.sock");
    expect(v.badge?.level).toBe("abandoned");
  });

  it("★tooltip 에 실행 가능한 셸 문자열이 없다(어떤 입력에서도)", () => {
    for (const sock of ["/dept/dev.sock", "\\\\.\\pipe\\dev$(x)", "/a b/%USERNAME%/c"]) {
      const v = pauseViewFrom(fleet, [st(sock, 60)], NOW);
      for (const forbidden of ["cys resume --socket", "CYS_SOCKET=", "PowerShell:", "cmd:"]) {
        expect(v.tooltip).not.toContain(forbidden);
      }
      expect(v.tooltip).toContain("클릭");
    }
  });

  it("레지스트리 부재·빈 응답에서도 죽지 않고 소켓 경로로 정직하게 표시", () => {
    expect(pauseViewFrom(null, [st("/x.sock", 60)], NOW).badge?.label).toBe("/x.sock");
    expect(pauseViewFrom({}, [st("/x.sock", 60)], NOW).badge?.label).toBe("/x.sock");
  });

  it("정지 없음 → 배지 null", () => {
    expect(pauseViewFrom(fleet, [{ socket: "/main.sock", paused: false }], NOW).badge).toBeNull();
  });
});
