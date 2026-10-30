// 레이아웃 트리의 sid 중복 제거 — 한 pane은 한 워크스페이스에 한 번만 존재한다(불변식).
//
// ★2026-08-08 실측 결함(오너 신고 "2개씩 표시된다 · pane이 full이 안 된다"):
// 앱이 저장한 cys-layout-v2 에 이런 트리가 남아 있었다.
//   {"type":"split","dir":"row","a":{"type":"pane","sid":164},"b":{"type":"pane","sid":164}}
// 같은 surface가 split 양쪽에 들어가 자기 자신과 화면을 반씩 나눠 가진 상태다. 사이드바 팀원
// 목록도 같은 노드를 두 줄로 그린다. 입양·재배치 경로가 "이미 이 ws에 있는가"를 보지 않으면
// 언제든 다시 생길 수 있으므로, 붙이는 쪽 가드(호출부)와 별개로 **저장·복원 양쪽 관문**에서
// 트리 자체를 정규화한다(normalizeWorkspaces와 같은 철학 — 유령이 증식하지 못하게 두 그물).

export type LayoutNode =
  | { type: "split"; dir: "row" | "col"; ratio?: number; a: LayoutNode; b: LayoutNode }
  | { type: "pane"; sid: number };

// 이미 본 sid가 또 나오면 그 가지를 드러낸다(먼저 나온 것이 살아남는다 — 위치 안정).
// 한쪽만 남으면 split을 접어 형제로 대체한다(replaceNode의 붕괴 규칙과 동일).
export function dedupeTree(node: LayoutNode | null, seen: Set<number> = new Set()): LayoutNode | null {
  if (!node) return null;
  if (node.type === "pane") {
    if (seen.has(node.sid)) return null;
    seen.add(node.sid);
    return node;
  }
  const a = dedupeTree(node.a, seen);
  const b = dedupeTree(node.b, seen);
  if (a && b) return { ...node, a, b };
  return a ?? b;
}

// 트리에 담긴 sid 목록(중복 포함) — 진단·테스트용.
export function treeSids(node: LayoutNode | null, out: number[] = []): number[] {
  if (!node) return out;
  if (node.type === "pane") out.push(node.sid);
  else {
    treeSids(node.a, out);
    treeSids(node.b, out);
  }
  return out;
}

export function socketOwnsSid(
  workspaces: readonly { socket?: string; tree: LayoutNode | null }[],
  socket: string | undefined,
  sid: number,
): boolean {
  return workspaces.some(
    (ws) => (ws.socket ?? undefined) === (socket ?? undefined) && treeSids(ws.tree).includes(sid),
  );
}
