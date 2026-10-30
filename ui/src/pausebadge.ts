// pause 배지 — 정지 상태를 GUI에서 눈에 띄게 만드는 순수 함수(main.ts는 배선만 한다).
//
// [B7] 2026-08-04 방치 사건: 문제는 "멈춘 것"이 아니라 **멈춘 채로 잊힌 것**이었다.
// 정적인 한 줄 표시는 어제 멈춘 것과 방금 멈춘 것을 똑같이 보여줘 방치를 드러내지 못한다.
// 그래서 배지는 ⑴경과 시간을 항상 담고 ⑵임계(1h/6h)를 넘으면 등급 자체가 올라간다.
//
// CLI(`cys status`)의 pause_banner와 **같은 임계·같은 어휘**를 쓴다 — 두 표시가 다른 말을 하면
// 어느 쪽을 믿을지가 새 문제가 된다.

export type PauseLevel = "none" | "paused" | "stale" | "abandoned";

export interface PauseInfo {
  since?: number | null; // epoch seconds
  reason?: string | null;
}

export interface PauseBadge {
  level: PauseLevel;
  /** 배지 본문(한 줄). level === "none" 이면 빈 문자열. */
  text: string;
  /** 사유 미기록 여부 — 데몬이 빈 사유를 거부하므로, true면 게이트 이전 정지이거나 우회다. */
  reasonMissing: boolean;
  /** 정지 경과 초. since를 모르면 null. */
  elapsedSecs: number | null;
  /**
   * [B7-r2] 어느 데몬이 멈췄는가 — 소켓 경로. 여러 소켓(본부·부서)을 집계하면서 이것을 버리면
   * 화면엔 "멈췄다"만 남고 **무엇을 resume 해야 하는지** 알 수 없다. 기본 소켓을 잘못 해제하거나
   * 정작 멈춘 부서를 계속 방치하게 된다(이 기능이 막으려던 바로 그 실패).
   */
  socket: string | null;
  /** 사람이 읽는 대상 표시명(본부/부서 라벨). 없으면 소켓 경로로 대체한다. */
  label: string | null;
}

/** 1시간 넘게 정지 = 확인 필요 · 6시간 넘게 정지 = 방치 의심. CLI와 동일 임계. */
export const PAUSE_STALE_SECS = 3600;
export const PAUSE_ABANDONED_SECS = 6 * 3600;

/** "3h12m" / "12m5s" / "40s" — CLI fmt_secs와 같은 표기. */
export function fmtSecs(s: number): string {
  const n = Math.max(0, Math.floor(s));
  if (n >= 3600) return `${Math.floor(n / 3600)}h${Math.floor((n % 3600) / 60)}m`;
  if (n >= 60) return `${Math.floor(n / 60)}m${n % 60}s`;
  return `${n}s`;
}

export function pauseBadge(
  paused: boolean,
  info: PauseInfo | null | undefined,
  nowSecs: number,
  target?: { socket?: string | null; label?: string | null },
): PauseBadge {
  const socket = target?.socket ?? null;
  const label = target?.label ?? null;
  if (!paused) {
    return { level: "none", text: "", reasonMissing: false, elapsedSecs: null, socket, label };
  }
  const reason = (info?.reason ?? "").trim();
  const since = typeof info?.since === "number" ? info.since : null;
  const elapsedSecs = since === null ? null : Math.max(0, Math.floor(nowSecs - since));

  let level: PauseLevel = "paused";
  if (elapsedSecs !== null && elapsedSecs >= PAUSE_ABANDONED_SECS) level = "abandoned";
  else if (elapsedSecs !== null && elapsedSecs >= PAUSE_STALE_SECS) level = "stale";

  const mark = level === "abandoned" ? "🚨" : level === "stale" ? "⚠️" : "⛔";
  const age = elapsedSecs === null ? "" : ` ${fmtSecs(elapsedSecs)} 경과`;
  const shown = reason || "(사유 미기록 — B7 게이트 이전 정지이거나 우회다)";
  const urgency =
    level === "abandoned"
      ? " ★6시간 넘게 정지 — 방치 여부를 지금 판단하라"
      : level === "stale"
        ? " ★1시간 넘게 정지 — 해제하거나 사유를 갱신하라"
        : "";

  // [B7-r2] 대상을 문구에 싣는다 — "어디가 멈췄는지"가 배지 자체에 있어야 한다.
  const who = label ? ` [${label}]` : "";
  return {
    level,
    text: `${mark} PAUSED${who}${age} — ${shown}${urgency}`,
    reasonMissing: reason.length === 0,
    elapsedSecs,
    socket,
    label,
  };
}

/**
 * [B7-r2] 여러 소켓의 정지 중 **하나**를 골라 상단바에 띄운다.
 *
 * 종전 집계는 등급만 비교해 동률이면 **먼저 만난 것**을 유지했다. 그래서 6시간 멈춘 부서와
 * 6시간 1분 멈춘 본부가 같이 있으면 어느 쪽이 뜰지가 소켓 순회 순서에 달렸다.
 * 등급이 같으면 **가장 오래된 것**을 고른다 — 방치는 오래된 쪽이 더 급하다.
 * 경과 미상(elapsedSecs === null)은 최하위로 둔다(모르는 것을 가장 오래된 것으로 대접하지 않는다).
 */
export function worstPauseBadge(badges: readonly PauseBadge[]): PauseBadge | null {
  const rank: Record<PauseLevel, number> = { none: 0, paused: 1, stale: 2, abandoned: 3 };
  let best: PauseBadge | null = null;
  for (const b of badges) {
    if (b.level === "none") continue;
    if (!best) {
      best = b;
      continue;
    }
    if (rank[b.level] !== rank[best.level]) {
      if (rank[b.level] > rank[best.level]) best = b;
      continue;
    }
    // 동률 — 더 오래된 쪽. null(경과 미상)은 지지 않게 -1 로 두어 최하위가 된다.
    const age = (x: PauseBadge) => (x.elapsedSecs === null ? -1 : x.elapsedSecs);
    if (age(b) > age(best)) best = b;
  }
  return best;
}

/**
 * [B7-r5] 해제 **안내 문구** — 실행 가능한 셸 명령을 만들지 않는다.
 *
 * r4 는 cmd/PowerShell 을 갈라 각자 인용을 붙였지만, cmd 는 큰따옴표 안에서도 `%VAR%` 를
 * 치환하고 `/v:on` 에서는 `!VAR!` 까지 확장한다 — **정적 문자열로 cmd 의 `%` 를 안전하게
 * 막을 방법이 없다.** 라벨을 같은 줄에 붙인 것도 문제였다(줄 전체를 복붙하면 첫 토큰이
 * `cys` 가 아니다). 사용자가 복붙해 셸을 거치는 설계 자체가 원인이다.
 *
 * 그래서 문구는 **사람이 읽는 설명**만 담고, 실제 해제는 배지 클릭 → system_resume IPC 가
 * 소켓을 argv 로 넘겨 집행한다(어떤 셸도 개입하지 않는다). 소켓 경로는 명령이 아니라
 * **표시 데이터**로만 나온다.
 */
export function resumeGuidance(badge: PauseBadge | null): string {
  if (!badge || badge.level === "none") return "";
  return "해제: 이 배지를 클릭하세요(확인 후 해당 데몬만 재개합니다).";
}

/**
 * [B7-r3] 소켓 → 표시명. **권위는 org_fleet 레지스트리**(display_name)다.
 *
 * 초판은 `workspaces.find(w => w.socket === sock)` 로 첫 탭 이름을 썼다. 기본 데몬
 * (socket === undefined)을 여러 본부 워크스페이스가 공유하면 **배열 순서**에 따라
 * 경영본부로도 목회사역본부로도 표시됐고, 사용자가 탭을 rename 하면 데몬 정체와 라벨이
 * 어긋났다. 정지 대상을 알리는 것이 목적인데 대상을 지어내면 안 된다.
 *
 * 해소 순서: display_name → registry name → 소켓 경로. 기본 데몬은 특정 워크스페이스를
 * 지어내지 않고 레지스트리의 본부 항목(없으면 고정 라벨 "메인 데몬")으로 표시한다.
 */
export interface FleetDept {
  name?: string | null;
  display_name?: string | null;
  socket?: string | null;
  socket_slug?: string | null;
}

export const MAIN_DAEMON_LABEL = "메인 데몬";

export function socketLabelMap(depts: readonly FleetDept[] | null | undefined): Map<string, string> {
  const m = new Map<string, string>();
  for (const d of depts ?? []) {
    const sock = (d?.socket ?? "").trim();
    if (!sock) continue;
    const label = (d?.display_name ?? "").trim() || (d?.name ?? "").trim();
    if (label) m.set(sock, label);
  }
  return m;
}

/** 소켓 하나의 표시명. 미등록 소켓은 지어내지 않고 소켓 경로 자체를 보여준다(정직). */
export function labelForSocket(
  socket: string | null | undefined,
  map: Map<string, string>,
  mainSocket?: string | null,
): string {
  const sock = (socket ?? "").trim();
  if (!sock) {
    // 기본 데몬 — 레지스트리에 본부 항목이 있으면 그 이름, 없으면 고정 라벨.
    const main = (mainSocket ?? "").trim();
    return (main && map.get(main)) || MAIN_DAEMON_LABEL;
  }
  return map.get(sock) ?? sock;
}


/**
 * [B7-r4] 정지 배지 갱신의 **오케스트레이션 전부**를 순수함수로 모은다.
 *
 * r3 의 시험은 resumeHint·labelForSocket 을 **직접** 불렀다. refreshSidebarStatus 와
 * renderPauseBadge 가 그것들을 어떻게 쓰는지는 아무도 안 봤고, 그래서 ⑴플랫폼을 POSIX 로
 * 하드코딩하는 변이 ⑵라벨을 첫 workspace 에서 가져오는 구 배선 변이가 **둘 다 336건을 통과**
 * 했다(노아 실증 false-green). 계산이 DOM·비동기 루프 안에 있으면 그 배선을 잴 수 없다.
 *
 * 그래서 소켓별 상태 → 화면에 실제로 그릴 것(문구·등급·대상·해제 명령)까지를 여기서 다 만든다.
 * 소비자는 이 결과를 **그대로** 그리기만 한다.
 */
export interface PauseSocketStatus {
  socket?: string | null;
  paused?: boolean;
  pause_info?: PauseInfo | null;
}

export interface PauseView {
  /** 그릴 배지. 정지가 하나도 없으면 null(배지 숨김). */
  badge: PauseBadge | null;
  /** tooltip 전문 — 대상과 해제 명령을 포함한다. 배지가 없으면 빈 문자열. */
  tooltip: string;
}

export function pauseView(
  statuses: readonly PauseSocketStatus[],
  labels: Map<string, string>,
  nowSecs: number,
  mainSocket?: string | null,
): PauseView {
  const badges = statuses.map((st) =>
    pauseBadge(st.paused === true, st.pause_info, nowSecs, {
      socket: st.socket ?? null,
      label: labelForSocket(st.socket, labels, mainSocket),
    }),
  );
  const badge = worstPauseBadge(badges);
  if (!badge) return { badge: null, tooltip: "" };
  const where = badge.label
    ? `대상: ${badge.label}${badge.socket ? ` (${badge.socket})` : ""}`
    : badge.socket
      ? `대상 소켓: ${badge.socket}`
      : "대상: (소켓 미상)";
  const tooltip = [
    badge.text,
    "",
    where,
    "큐 배달·스케줄 발화만 동결됩니다. 실행 중 에이전트 행동은 계속됩니다.",
    resumeGuidance(badge),
  ].join("\n");
  return { badge, tooltip };
}


/**
 * [B7-r5] 데이터 수집 → 화면에 그릴 것까지의 **경계 전부**를 import 가능한 함수로 만든다.
 *
 * r4 의 소비자 시험은 main.ts 원문 substring 을 봤다. 그래서 ⑴전역 IS_WINDOWS 를 false 로
 * 고정해도 ⑵refreshFleetLabels 를 첫 workspace 이름 맵으로 바꿔도 **둘 다 통과**했다 —
 * 막으려던 두 결함을 실제로 복원하는데 355건이 전부 초록이었다(노아 실증).
 *
 * 여기서는 org_fleet 결과와 소켓별 org.status 를 **입력으로 받아** 최종 PauseView 를 만든다.
 * 테스트는 그 입력을 직접 넣고 결과를 단언하므로, 수집 경로를 어떻게 바꿔치기해도 결과가
 * 달라지면 잡힌다(소스에 어떤 글자가 있는지와 무관하다).
 */
export interface FleetSnapshot {
  departments?: FleetDept[] | null;
}

export function pauseViewFrom(
  fleet: FleetSnapshot | null | undefined,
  statuses: readonly PauseSocketStatus[],
  nowSecs: number,
): PauseView {
  const depts = fleet?.departments ?? [];
  const labels = socketLabelMap(depts);
  // 본부(_hq) 항목이 기본 데몬의 권위 이름이다 — 워크스페이스 탭 이름을 빌려오지 않는다.
  const mainSocket = depts.find((d) => d?.name === "_hq")?.socket ?? null;
  return pauseView(statuses, labels, nowSecs, mainSocket);
}
