// cys-exited 자동 소멸 정책 — 순수 판정 모듈. onDaemonEvent/removeDeadPane 호출부가 이 모듈의
// 판정 결과만 따르고, 실제 setTimeout·invoke 부작용은 main.ts가 소유한다(기존
// scheduleFeedSwitchIfStillPending 패턴과 동일 — 유예 종료 시점에 실제 상태를 재조회해
// "아직도 그 상태인가"를 확인한 뒤에만 행동하고, 타이머 자체를 취소 토큰으로 관리하지 않는다).
//
// 설계 C 원문(2026-09-22 codex-5 조사, 테오 2차 승인) 계약:
//   - surface.closed/reaped: 검증된 동일 소켓·SID만 유예 없이 즉시 제거.
//   - surface.exited: autoRemoveExited가 켜져 있을 때만, 유예 뒤 제거. 유예는 데몬 기준
//     정확한 프로세스 종료시각이 아니라 GUI가 이벤트를 처음 본 시점부터 계산한다.
//   - autoRemoveExited=false → 자동 제거만 중단(hold). preserveEmptyWorkspace는 독립 축이다.
export interface ExitPolicySettings {
  autoRemoveExited: boolean;
  preserveEmptyWorkspace: boolean;
  graceMs: number;
}

export const DEFAULT_EXIT_POLICY: ExitPolicySettings = {
  autoRemoveExited: true,
  preserveEmptyWorkspace: true,
  graceMs: 3000,
};

export type ExitAction = "remove-now" | "remove-after-grace" | "hold";

// closed/reaped는 정책과 무관하게 즉시(설계 원문 "검증된 동일 소켓·SID만 제거"는 이미
// resolveEventSocket이 소켓 검증을 마친 뒤에만 이 함수가 호출된다는 전제다). exited만
// autoRemoveExited 토글의 영향을 받는다.
export function decideExitAction(eventName: string, autoRemoveExited: boolean): ExitAction {
  if (eventName === "surface.closed" || eventName === "surface.reaped") return "remove-now";
  if (eventName === "surface.exited") return autoRemoveExited ? "remove-after-grace" : "hold";
  return "hold";
}

// 유예 종료 시점 재조회 결과 — 실측 3분기(성공+발견, 성공+미발견, 조회 실패)를 그대로 타입화한다.
export type GraceRecheck = { ok: true; found: true; exited: boolean } | { ok: true; found: false } | { ok: false };

// 실패/빈/부분 목록 처리: 조회 실패(ok:false)나 목록에서 사라짐(found:false, 완전 reap)은
// "제거를 막을 근거가 없다"로 보고 제거를 진행한다 — 이게 이 기능의 본래 목적(죽은 pane
// 무기한 잔존 방지)이다. **명시적으로 살아있음(found:true, exited:false)이 확인된 경우만**
// recover로 보고 제거를 취소한다 — 불확실성의 기본값은 "제거", 회복의 기본값은 "명시적 증거
// 필요"로 비대칭이다(의도적 설계 — 불확실할 때 죽은 pane을 방치하는 쪽이 아니라 청소하는
// 쪽이 이 기능의 목적과 일치한다).
export function shouldRemoveAfterGrace(recheck: GraceRecheck): boolean {
  if (!recheck.ok) return true;
  if (!recheck.found) return true;
  return recheck.exited !== false;
}

// preserveEmptyWorkspace=true(기본)면 removeDeadPane은 기존 그대로 pane leaf만 지우고 ws
// 껍데기(id/name/group/order/color/collapsed)는 항상 보존한다. false일 때만 방금 제거로
// tree가 비어버린 workspace를 함께 정리한다 — 두 축(자동제거 여부·빈 패널 보존 여부)이
// 서로 독립임을 실제로 동작에 반영한다(preserveEmptyWorkspace를 항상 무동작 플래그로
// 두지 않는다).
export function shouldDropEmptyWorkspace(preserveEmptyWorkspace: boolean, treeIsNowEmpty: boolean): boolean {
  return !preserveEmptyWorkspace && treeIsNowEmpty;
}

// 비활성 workspace 포커스 불변: 죽은 pane이 '현재 활성 workspace(동일 socket)' 소속이고
// 그 pane이 지금 포커스 대상일 때만 포커스를 재배정한다. 사용자가 보고 있지 않은 다른
// workspace에서 발생한 종료는 현재 포커스를 건드리지 않는다. removeDeadPane의 기존 가드
// 조건을 그대로 순수 함수로 추출한 것 — 동작 변경 없음, 단독 테스트 가능성만 추가.
export function shouldReassignFocus(
  deadSid: number,
  focusedSid: number | null,
  deadSocket: string | undefined,
  activeSocket: string | undefined,
): boolean {
  return focusedSid === deadSid && activeSocket === deadSocket;
}
