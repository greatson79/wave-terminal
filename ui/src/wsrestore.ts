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
import { transferTrees, treeSids } from "./transfer";

interface SavedWorkspace {
  id: number;
  name: string;
  socket?: string;
  pending?: boolean;
  groupId?: number;
  collapsed?: boolean;
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

// 같은 소켓의 구/신 본부명 패널을 합친다. 빈 중복도 저장 전에 제거한다.
export function coalesceHqWorkspaces<T extends SavedWorkspace>(
  workspaces: T[],
  samePanel: (a: string, b: string) => boolean,
): Map<number, number> {
  const removed = new Map<number, number>();
  for (let i = 0; i < workspaces.length; i++) {
    if (!workspaces[i].tree) {
      const occupied = workspaces.findIndex((ws, j) => j > i && ws.tree && !ws.pending
        && ws.socket === workspaces[i].socket && samePanel(ws.name, workspaces[i].name));
      if (occupied >= 0) [workspaces[i], workspaces[occupied]] = [workspaces[occupied], workspaces[i]];
    }
    const keep = workspaces[i];
    for (let j = i + 1; j < workspaces.length;) {
      const extra = workspaces[j];
      if (keep.pending || extra.pending || keep.socket !== extra.socket || !samePanel(keep.name, extra.name)) { j++; continue; }
      if (extra.tree) keep.tree = dedupeTree(keep.tree
        ? { type: "split", dir: "row", a: keep.tree, b: extra.tree }
        : extra.tree);
      if (keep.groupId == null) keep.groupId = extra.groupId;
      if (keep.collapsed == null) keep.collapsed = extra.collapsed;
      removed.set(extra.id, keep.id);
      workspaces.splice(j, 1);
    }
  }
  return removed;
}

// 확인된 기본 데몬의 빈 본부 탭만 정리한다. 사용자 빈 탭·부서 소켓은 보존한다.
export function pruneEmptyHqWorkspaces<T extends SavedWorkspace>(
  workspaces: T[], isHqPanel: (name: string) => boolean,
): number[] {
  const removed: number[] = [];
  for (let i = workspaces.length - 1; i >= 0 && workspaces.length > 1; i--) {
    const ws = workspaces[i];
    if (ws.tree || ws.pending || ws.socket !== undefined || !isHqPanel(ws.name)) continue;
    removed.push(ws.id);
    workspaces.splice(i, 1);
  }
  return removed;
}

// 저장된 큰 격자는 첫 렌더 전에 살아 있는 제목으로 분리한다. 미응답 소켓의 트리는 그대로 둔다.
export function rehomeRestoredSeats<T extends SavedWorkspace>(
  workspaces: T[],
  live: readonly { socket?: string; surface_id: number; title: string }[],
  target: (workspaceName: string, title: string) => string | null,
  makeWorkspace: (name: string, socket?: string) => T,
  samePanel: (a: string, b: string) => boolean = (a, b) => a === b,
): void {
  for (const seat of live) {
    const from = workspaces.find((ws) => ws.socket === seat.socket && treeSids(ws.tree).includes(seat.surface_id));
    if (!from) continue;
    const name = target(from.name, seat.title);
    if (!name) continue;
    let to = workspaces.find((ws) => ws.socket === seat.socket && (ws.name === name || samePanel(ws.name, name)));
    const moved = transferTrees(from.tree, to?.tree ?? null, seat.surface_id);
    if (!moved) continue;
    if (!to) { to = makeWorkspace(name, seat.socket); workspaces.push(to); }
    from.tree = moved.src;
    to.tree = moved.dest;
  }
}
