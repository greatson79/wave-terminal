// GUI 재시작 복원 필터 — 순수 판정 모듈. keepWorkspaceOnRestore 대칭화(설계 D).
//
// 수리 전 결함: 빈(tree===null) workspace를 재시작 때 보존할지 판정하는 조건에
// `ws.socket != null`이 섞여 있어, 기본(default, socket===undefined) 소켓의 빈 본부 패널만
// 데몬이 살아있어도(lb.ok===true) 버려지고 부서 소켓은 보존됐다 — 같은 규칙이어야 할 두
// 소켓 정체성이 서로 다르게 취급됐다. 설계 D 원문: "상류 keepWorkspaceOnRestore는 기본/부서
// 양쪽을 같은 규칙으로 보존한다." 이 함수는 소켓 정체성과 무관하게 데몬 응답 하나로만 판정한다.
export function shouldKeepEmptyWorkspaceOnRestore(daemonOk: boolean | undefined): boolean {
  if (daemonOk === false) return true; // 일시 미응답 — 영구삭제 방지, 보존
  return daemonOk === true; // 명시적으로 살아있다고 확인됐을 때만 보존(정보 없음은 보존 안 함 — fail-closed)
}

import { dedupeTree, type LayoutNode } from "./treededupe";

interface SavedWorkspace {
  id: number;
  name: string;
  socket?: string;
  pending?: boolean;
  tree: LayoutNode | null;
}

export function normalizeWorkspaces<T extends SavedWorkspace>(list: T[]): T[] {
  const seenId = new Set<number>();
  let nextId = Math.max(0, ...list.map((w) => w.id).filter(Number.isSafeInteger)) + 1;
  const out: T[] = [];
  for (const w of list) {
    if (w.pending) continue;
    if (!Number.isSafeInteger(w.id) || seenId.has(w.id)) w.id = nextId++;
    seenId.add(w.id);
    out.push(w);
  }
  const seenBySock = new Map<string, Set<number>>();
  for (const w of out) {
    const key = w.socket ?? "";
    let seen = seenBySock.get(key);
    if (!seen) { seen = new Set(); seenBySock.set(key, seen); }
    w.tree = dedupeTree(w.tree, seen);
  }
  return out;
}
