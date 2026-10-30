// cys-exited 2차 보완 — GUI 쪽 주기 점검 백스톱. 순수 판정 모듈.
//
// 실측 재현(현장, 2026-09-23): 좌석을 2단 exit로 정상 회수하면 데몬(cys list/ps 전 12좌석
// 기준)에서는 surface가 완전히 사라지는데, GUI 앱 패널에는 죽은 패널이 그대로 남는 사례가
// 확인됐다. onDaemonEvent(§A/§1 수리)는 데몬이 실제로 그 이벤트를 **발신했을 때만** 작동한다
// — 이벤트 자체가 유실되면(네트워크·큐 처리 등 어떤 이유로든) 이벤트 경로만으로는 이 케이스를
// 잡을 수 없다. 그래서 소멸 판정을 이벤트뿐 아니라 refreshPaneTitles의 3초 주기 폴링으로도
// 받쳐야 한다 — v0.14.38 advanceGhostStrikes의 보수적 규약을 그대로 따른다:
//   - 목록 조회 실패 또는 **빈 목록**은 몇 번 관측해도 판정 보류(스트라이크 불변). 빈 목록은
//     "진짜 다 죽음"과 "열거 버그/일시 전체 실패"를 구분할 수 없어 상류도 절대 이걸 근거로
//     제거하지 않는다.
//   - 성공 + 비어있지 않은 목록에서 렌더 중인 SID가 **2회 연속** 빠져야 제거한다(1회는 아직
//     보류 — 폴링 타이밍과 데몬 쪽 순간적 목록 불일치를 흡수하는 여유).
//   - 목록에 다시 나타나면(recover) 스트라이크를 0으로 리셋한다.
export interface GhostObservation {
  listOk: boolean;
  listNonEmpty: boolean;
  presentInList: boolean;
}

export function nextGhostStrike(currentStrikes: number, obs: GhostObservation): { strikes: number; remove: boolean } {
  if (!obs.listOk || !obs.listNonEmpty) return { strikes: currentStrikes, remove: false }; // 실패/빈 목록 — 판정 보류
  if (obs.presentInList) return { strikes: 0, remove: false }; // 다시 관측됨 — 리셋
  const strikes = currentStrikes + 1;
  return { strikes, remove: strikes >= 2 }; // 2회 연속 결측 → 제거
}
