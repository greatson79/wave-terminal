// exitpolicy.ts 회귀 테스트 — 설계 C 되돌림 노브·유예 계약 고정 픽스처.
import { describe, it, expect } from "bun:test";
import {
  decideExitAction,
  shouldRemoveAfterGrace,
  shouldDropEmptyWorkspace,
  shouldReassignFocus,
  DEFAULT_EXIT_POLICY,
} from "./exitpolicy";

describe("decideExitAction — closed/reaped는 즉시, exited만 정책 대상", () => {
  it("surface.closed는 autoRemoveExited 무관 항상 즉시 제거", () => {
    expect(decideExitAction("surface.closed", true)).toBe("remove-now");
    expect(decideExitAction("surface.closed", false)).toBe("remove-now");
  });
  it("surface.reaped는 autoRemoveExited 무관 항상 즉시 제거", () => {
    expect(decideExitAction("surface.reaped", true)).toBe("remove-now");
    expect(decideExitAction("surface.reaped", false)).toBe("remove-now");
  });
  it("surface.exited는 autoRemoveExited=true면 유예 뒤 제거", () => {
    expect(decideExitAction("surface.exited", true)).toBe("remove-after-grace");
  });
  it("종료 직후 유예 — exited 이벤트는 즉시가 아니라 유예 경로로만 간다(remove-now 아님)", () => {
    expect(decideExitAction("surface.exited", true)).not.toBe("remove-now");
  });
  it("autoRemoveExited=false 되돌림 노브 — 자동 제거만 중단(hold), 제거 호출 자체가 없다", () => {
    expect(decideExitAction("surface.exited", false)).toBe("hold");
  });
  it("알 수 없는 이벤트명은 보류", () => {
    expect(decideExitAction("surface.unknown-future-event", true)).toBe("hold");
  });
});

describe("shouldRemoveAfterGrace — 유예 종료 시점 재조회 3분기 + 실패/빈/부분 목록", () => {
  it("재조회 결과 여전히 exited=true → 제거 진행", () => {
    expect(shouldRemoveAfterGrace({ ok: true, found: true, exited: true })).toBe(true);
  });
  it("유예 중 recover(재조회 결과 exited=false로 살아있음) → 제거 취소", () => {
    expect(shouldRemoveAfterGrace({ ok: true, found: true, exited: false })).toBe(false);
  });
  it("목록에서 완전히 사라짐(reap 완료) → 제거 진행", () => {
    expect(shouldRemoveAfterGrace({ ok: true, found: false })).toBe(true);
  });
  it("조회 실패 → 마지막으로 아는 상태(exited)를 신뢰해 제거 진행(무기한 방치 방지)", () => {
    expect(shouldRemoveAfterGrace({ ok: false })).toBe(true);
  });
  it("빈 목록(found:false와 동형) → 제거 진행", () => {
    expect(shouldRemoveAfterGrace({ ok: true, found: false })).toBe(true);
  });
});

describe("shouldDropEmptyWorkspace — preserveEmptyWorkspace 독립 축", () => {
  it("preserveEmptyWorkspace=true(기본)면 tree가 비어도 workspace를 드롭하지 않는다", () => {
    expect(shouldDropEmptyWorkspace(true, true)).toBe(false);
  });
  it("preserveEmptyWorkspace=false면 tree가 빈 경우에만 workspace를 드롭한다", () => {
    expect(shouldDropEmptyWorkspace(false, true)).toBe(true);
  });
  it("tree가 아직 안 비었으면 preserveEmptyWorkspace 값과 무관하게 드롭하지 않는다", () => {
    expect(shouldDropEmptyWorkspace(false, false)).toBe(false);
    expect(shouldDropEmptyWorkspace(true, false)).toBe(false);
  });
});

describe("shouldReassignFocus — 비활성 workspace 포커스 불변", () => {
  it("죽은 pane이 포커스 대상이고 활성 ws와 같은 소켓이면 포커스 재배정 필요", () => {
    expect(shouldReassignFocus(42, 42, "/dept.sock", "/dept.sock")).toBe(true);
  });
  it("죽은 pane이 포커스 대상이 아니면(다른 pane에 포커스) 불변", () => {
    expect(shouldReassignFocus(42, 99, "/dept.sock", "/dept.sock")).toBe(false);
  });
  it("죽은 pane이 비활성(다른 소켓) workspace 소속이면 활성 포커스는 불변", () => {
    expect(shouldReassignFocus(42, 42, "/other-dept.sock", "/dept.sock")).toBe(false);
  });
  it("기본 소켓끼리(둘 다 undefined)도 동일 소켓으로 비교된다", () => {
    expect(shouldReassignFocus(42, 42, undefined, undefined)).toBe(true);
  });
});

describe("DEFAULT_EXIT_POLICY — 설계 C 기본 제안값 고정", () => {
  it("기본값은 autoRemoveExited=true, preserveEmptyWorkspace=true", () => {
    expect(DEFAULT_EXIT_POLICY.autoRemoveExited).toBe(true);
    expect(DEFAULT_EXIT_POLICY.preserveEmptyWorkspace).toBe(true);
  });
  it("기본 유예는 0보다 크다(유예 없는 즉시 제거가 기본이 아니다)", () => {
    expect(DEFAULT_EXIT_POLICY.graceMs).toBeGreaterThan(0);
  });
});
