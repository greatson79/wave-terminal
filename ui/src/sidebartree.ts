// 사이드바 2단 트리의 **계산**만 담당한다 — DOM 을 만들지 않는다.
//
// 왜 분리했나(본부장 지적 2026-08-17 · 검토 B3-r2 major REVISE 와 같은 결함 클래스):
// 사양(오너 정본)은 buildTab 안의 조건식 3개에만 살아 있었고, 시험은 그 배선을 통과하지 않았다
// (`main.ts` 는 xterm·Tauri 를 import 하므로 bun test 로 불러올 수 없다 → 사양이 영원히 미검증).
// 결과가 'buildTab 소비 배선 미검증 false-green' — 테스트 334건이 초록인데 최우선 사양은 단 한 줄도
// 검증되지 않은 상태였다. 그래서 판정을 순수 함수로 끌어내고 buildTab 은 **이 결과를 그리기만** 한다.
//
// 사양(주인님 확정 2026-08-17):
//   ① 상위 항목 = 본부 헤더 **1행**(접기 화살표 · 본부 배지 · 이름 · 인원수)
//   ② 그 하위에 들여쓰기로 소속 워커 항목
//   ③ 워커는 **절대 사이드바 최상위에 단독으로 나타나지 않는다**
//
// 그룹 키·본부 판정은 hierarchy.ts(hqOfWsName)가 단일 출처다 — 여기서 재구현하지 않는다.

import { hqOfWsName, hqDisplayName } from "./hierarchy";

// ★R8(주인님 확정 2026-08-22): 헤더 인원수 표기 = `(N명)` — 본부명 옆 같은 행.
export const hqCountLabel = (n: number): string => `(${n}명)`;

export interface SidebarWs {
  name: string;
  // 이 ws 가 보유한 pane(=노드) 목록. buildTab 은 collectSids(ws.tree) 를 넘긴다 —
  // 트리 소유권이 곧 묶임 불변식이라 타 본부 노드가 섞이는 것이 구조적으로 불가능하다.
  sids: readonly number[];
  collapsed?: boolean;
  pending?: boolean;
}

export interface SidebarTreeSpec {
  /** 이 ws 가 속한(또는 자기 자신인) 본부명. 평면 ws 는 null. */
  hq: string | null;
  /** 이 ws 가 **본부 패널 자체**인가(ws 이름이 곧 본부명). 레거시 팀 ws 는 false. */
  isHqPanel: boolean;
  /** 접기 화살표 글리프. 본부 패널만 가진다(그 외 null = 화살표 없음). */
  chevron: "▾" | "▸" | null;
  /** 헤더에 표시할 인원수. 본부 패널만(그 외 null = 표시 없음). */
  count: number | null;
  /**
   * 서브라인을 붙이는가. **pending(부서 데몬 시작 중) 일 때만 true** —
   * 그 경우의 서브라인은 지표가 아니라 진행 문구("부서 데몬 시작 중…")이고, 없으면 사용자가
   * 멈춘 줄로 오해한다(WP-10).
   *
   * ★R1 major 수리: 종전에는 non-pending 탭에도 서브라인을 붙여 **상태 dot** 을 남겼다.
   * 정본 사양은 "탭/사이드바 부가지표 제거"인데 커밋이 dot 을 스스로 예외로 선언한 것이고,
   * **주인님이 그 예외를 승인한 적이 없다**(코드 주석이 사양을 바꾸지 못한다 — 본부장 판정).
   * 생사 신호가 기능상 필요하다는 판단은 승인 경로로 올리고, 기본값은 제거다.
   */
  showSub: boolean;
  /** 헤더 하위에 들여쓰기로 낼 구성원 sid 목록. 빈 배열 = 구성원 목록 미생성. */
  memberSids: readonly number[];
  /**
   * 헤더에 쓸 라벨. `null` = ws 이름을 그대로 쓴다(레거시 팀 ws·빈 ws).
   *
   * ★R2 [7] 수리: 본부를 모르는 ws 가 **노드를 들고 있으면** 명시적 미분류 헤더를 쓴다.
   * 종전에는 그런 ws 의 유일한 워커가 구성원 행 없이 최상위 `ws-tab` 으로만 존재했다 —
   * 주인님 사양("워커는 절대 사이드바 최상위에 단독으로 나타나지 않는다") 정면 위반이다.
   * 워커를 최상위로 승격하는 대신 **컨테이너 헤더를 만들어** 그 아래로 내린다.
   */
  headerLabel: string | null;
}

/** 본부 미상 노드를 담는 명시적 컨테이너 헤더 이름 — 워커를 최상위로 올리지 않기 위한 자리. */
export const UNCLASSIFIED_HQ = "미분류";

/**
 * ws 하나를 사이드바 항목 사양으로 환산한다.
 *
 * 구성원 목록 게이트가 이 함수의 핵심이고, 본부/비본부의 경계가 **1명**에서 갈린다:
 * - **본부 패널은 1명이어도 구성원 행을 낸다.** 헤더는 본부(조직)이고 워커가 아니므로, 유일한
 *   워커도 하위 항목으로 나와야 사양 ③(워커가 최상위에 단독으로 안 뜬다)이 성립한다.
 *   종전 `sids.length > 1` 게이트는 1인 본부의 워커를 사이드바에서 지워 **본부 헤더가 곧 그
 *   워커처럼** 보이게 만들었다 — 이 함수가 막는 회귀가 바로 그것이다.
 * - **비본부 ws 는 2명 이상일 때만.** 그쪽 헤더는 곧 그 노드라, 1인일 때 구성원 행을 내면
 *   같은 노드가 두 줄로 그려진다("2개씩 보인다" 오너 신고).
 */
export function sidebarTreeSpec(ws: SidebarWs): SidebarTreeSpec {
  const hq = hqOfWsName(ws.name);
  const isHqPanel = hq !== null && hq === ws.name.trim();
  const pending = !!ws.pending;
  // pending(부서 데몬 시작 중)은 이름·구성원이 미확정이다 — 헤더 1행화도 구성원 목록도 적용하지
  // 않고 종전 placeholder 렌더를 그대로 둔다(런칭 중 탭이 조용히 빈 줄이 되는 것을 막는다).
  // ★R2 [7] 수리: 노드를 든 ws 는 **종류와 무관하게** 컨테이너 헤더가 된다. 종전에는 비-본부 ws
  //   의 하한이 2명이라, 1인 평면·레거시 ws 의 워커가 구성원 행 없이 **최상위 탭 그 자체**로
  //   남았다(주인님 사양 정면 위반). 하한을 1로 통일해 워커가 늘 헤더 하위에만 나타나게 한다.
  //   "같은 노드 2줄" 우려는 헤더 라벨을 본부/미분류(=컨테이너)로 바꿔 해소한다 — 헤더는 이제
  //   노드의 이름이 아니라 그것을 담는 조직 칸이다.
  const container = !pending && ws.sids.length > 0;
  const showMembers = container && !ws.collapsed;
  return {
    hq,
    isHqPanel,
    // 접기·인원수는 컨테이너 헤더 전부에 준다 — 미분류 칸도 접고 규모를 볼 수 있어야 한다.
    chevron: container ? (ws.collapsed ? "▸" : "▾") : null,
    count: container ? ws.sids.length : null,
    showSub: pending,
    memberSids: showMembers ? ws.sids : [],
    // 본부를 알면 본부명, 노드를 들었는데 본부를 모르면 미분류, 그 밖은 ws 이름 그대로.
    // ★R8: 본부 헤더 = `{본부닉}·{본부}` (닉 단독 ws 이름은 표시명으로 복원 — hqDisplayName).
    headerLabel: isHqPanel ? hqDisplayName(hq) : hq === null && container ? UNCLASSIFIED_HQ : null,
  };
}

/**
 * 사양 ③ 의 직접 판정 — "이 ws 렌더에서 워커가 최상위에 단독으로 나타나는가".
 *
 * 본부 패널이 노드를 들고 있는데 구성원 목록이 비어 있으면, 그 노드는 사이드바에서 헤더 말고는
 * 나타날 곳이 없다 = 헤더가 워커를 대신 표기하는 상태다. 시험이 이 술어를 직접 단언해 사양을
 * 조건식이 아니라 **결과**로 잠근다.
 */
export function hasOrphanTopLevelWorker(ws: SidebarWs): boolean {
  const spec = sidebarTreeSpec(ws);
  // ★R2 [7] 수리: 본부 패널만 보던 게이트를 **모든 ws** 로 넓혔다. 종전에는 평면·레거시 ws 의
  //   고아 워커가 판정에서 아예 제외돼, 사양 위반이 이 술어로도 드러나지 않았다(검토 지적).
  if (ws.pending) return false;
  if (ws.sids.length === 0) return false;
  // 접힘은 사용자가 스스로 감춘 상태이므로 고아가 아니다(펼치면 전원 나온다).
  if (ws.collapsed) return false;
  return spec.memberSids.length !== ws.sids.length;
}
