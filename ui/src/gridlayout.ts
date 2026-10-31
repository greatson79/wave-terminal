// 반응형 격자 배치 — 순수 계산 모듈(DOM·Tauri 없음).
//
// 옛 roleLayout 은 가운데 좌석을 전부 같은 폭의 가로 한 줄로 나눠, 좌석이 늘면 모두 좁아져 글이
// 잘렸다. 여기서는 패널의 실제 크기와 좌석 수로 열·행을 구한다:
//   · 열 수 = 패널 폭에 좌석 최소 폭이 몇 개 들어가는가(역할 열이 쓰는 폭은 먼저 뺀다)
//   · 줄 수 = 좌석 수 / 열 수를 올림, 줄마다 좌석을 고르게 나눠 고아 줄을 피한다
//   · 본부장은 첫 칸이고 같은 줄의 다른 칸보다 headWeight 배 넓다(다른 칸이 최소 폭 밑으로 가지 않는 한도)
//   · master·cso 는 왼쪽 끝 열(3:1), 리뷰어 둘은 오른쪽 끝 열(1:1) — 기존 자리 규칙 유지
// 들어가지 않으면 칸을 최소 크기 밑으로 줄이지 않고 minWidth·minHeight 를 돌려준다 —
// 호출부가 이 크기를 패널에 걸어 스크롤로 보여 준다(탭으로 숨기면 좌석 상태가 안 보이므로 스크롤).
import type { LayoutNode } from "./treededupe";

export interface GridSeat {
  sid: number;
  role?: string | null;
  head?: boolean;
}

export interface GridSpec {
  minW: number; // 좌석 한 칸의 최소 폭(px)
  minH: number; // 좌석 한 칸의 최소 높이(px)
  headWeight: number; // 본부장 칸의 폭 가중치 상한(다른 칸 = 1)
}

export interface GridResult {
  tree: LayoutNode | null;
  minWidth: number; // 0 = 패널 폭에 다 들어감, 아니면 이 폭(px) 이상으로 두고 가로 스크롤
  minHeight: number; // 0 = 패널 높이에 다 들어감, 아니면 이 높이(px) 이상으로 두고 세로 스크롤
}

// 상수 근거: 70열 × 12행은 Codex 안내문·상태줄이 잘리지 않는 최소 읽기 크기(좁은 좌석에서 안내문이 잘려 발신 판독이 거부된 사례).
// xterm 글자 폭 ≈ 0.6em, 줄 높이 ≈ 1.25em. 칸의 군더더기 = 가로 12px(패딩·divider),
// 세로 32px(pane 헤더 24 + 패딩·divider 8). 글꼴 크기를 바꾸면 최소 크기도 같이 변한다.
const MIN_COLS = 70;
const MIN_ROWS = 12;
const CHROME_W = 12;
const CHROME_H = 32;
export const HEAD_WEIGHT = 1.5;

export function gridSpecFor(fontSize: number): GridSpec {
  return {
    minW: Math.ceil(MIN_COLS * fontSize * 0.6 + CHROME_W),
    minH: Math.ceil(MIN_ROWS * fontSize * 1.25 + CHROME_H),
    headWeight: HEAD_WEIGHT,
  };
}

const pane = (sid: number): LayoutNode => ({ type: "pane", sid });

// 가중치대로 나누는 빗 모양 트리: 앞쪽부터 w[i] / (w[i] + 뒤쪽 합).
function comb(nodes: LayoutNode[], weights: number[], dir: "row" | "col"): LayoutNode {
  let acc = nodes[nodes.length - 1];
  let tail = weights[nodes.length - 1];
  for (let i = nodes.length - 2; i >= 0; i--) {
    tail += weights[i];
    acc = { type: "split", dir, ratio: weights[i] / tail, a: nodes[i], b: acc };
  }
  return acc;
}

function pair(top: number, bottom: number, topShare: number): LayoutNode {
  return { type: "split", dir: "col", ratio: topShare, a: pane(top), b: pane(bottom) };
}

function roleColumn(a: GridSeat | undefined, b: GridSeat | undefined, aShare: number): LayoutNode | null {
  if (a && b) return pair(a.sid, b.sid, aShare);
  const one = a ?? b;
  return one ? pane(one.sid) : null;
}

export function gridLayout(seats: readonly GridSeat[], panel: { w: number; h: number }, spec: GridSpec): GridResult {
  if (!seats.length) return { tree: null, minWidth: 0, minHeight: 0 };

  const first = (role: string) => seats.find((s) => s.role === role);
  const master = first("master");
  const cso = first("cso");
  const agy = first("reviewer-gemini");
  const codex = first("reviewer-codex");
  const corner = new Set([master, cso, agy, codex].filter((s): s is GridSeat => !!s).map((s) => s.sid));
  const middle = seats.filter((s) => !corner.has(s.sid));
  const headIdx = middle.findIndex((s) => s.head);
  const ordered = headIdx > 0 ? [middle[headIdx], ...middle.slice(0, headIdx), ...middle.slice(headIdx + 1)] : middle;

  const left = roleColumn(master, cso, 3 / 4);
  const right = roleColumn(agy, codex, 1 / 2);
  const sides = (left ? 1 : 0) + (right ? 1 : 0);
  const m = ordered.length;

  // 열·줄 수. 줄마다 좌석을 고르게 나눠(앞 줄이 하나 더) 한 줄짜리 고아를 만들지 않는다.
  const cols = m ? Math.min(m, Math.max(1, Math.floor((panel.w - sides * spec.minW) / spec.minW))) : 0;
  const rows = m ? Math.ceil(m / cols) : 0;
  const base = m ? Math.floor(m / rows) : 0;
  const extra = m ? m % rows : 0;
  const counts = Array.from({ length: rows }, (_, i) => base + (i < extra ? 1 : 0));
  const widest = counts[0] ?? 0;

  // 본부장 칸 가중치: 다른 칸이 최소 폭을 지키는 한도 안에서 headWeight 까지.
  const hasHead = headIdx >= 0;
  const availW = panel.w - sides * spec.minW;
  const headW = hasHead && widest > 1 ? Math.min(spec.headWeight, Math.max(1, availW / spec.minW - (widest - 1))) : 1;

  let mid: LayoutNode | null = null;
  if (m) {
    let at = 0;
    const lines = counts.map((n, row) => {
      const cells = ordered.slice(at, at + n).map((s) => pane(s.sid));
      at += n;
      const weights = cells.map((_, i) => (row === 0 && n === widest && i === 0 ? headW : 1));
      return comb(cells, weights, "row");
    });
    mid = comb(lines, lines.map(() => 1), "col");
  }

  const parts: { node: LayoutNode; weight: number }[] = [];
  if (left) parts.push({ node: left, weight: 1 });
  if (mid) parts.push({ node: mid, weight: widest - 1 + headW });
  if (right) parts.push({ node: right, weight: 1 });
  const tree = comb(parts.map((p) => p.node), parts.map((p) => p.weight), "row");

  const needW = (sides + (m ? 1 : 0)) * spec.minW; // 한 열만 놓아도 필요한 폭
  const hasPair = (master && cso) || (agy && codex);
  const needH = Math.max(rows, hasPair ? 2 : 0) * spec.minH; // 역할 열의 위·아래 두 칸도 최소 높이를 지킨다
  return {
    tree,
    minWidth: panel.w < needW ? needW : 0,
    minHeight: panel.h < needH ? needH : 0,
  };
}

// 사람이 divider 를 끌거나 pane 을 옮긴 워크스페이스(manual)는 자동 결과로 덮지 않는다.
export function keepManualTree(manual: boolean | undefined, current: LayoutNode | null, auto: LayoutNode | null): LayoutNode | null {
  return manual ? current : auto;
}

export function treeEquals(a: LayoutNode | null, b: LayoutNode | null): boolean {
  if (a === b) return true;
  if (!a || !b || a.type !== b.type) return false;
  if (a.type === "pane") return b.type === "pane" && a.sid === b.sid;
  if (b.type !== "split") return false;
  return a.dir === b.dir && Math.abs((a.ratio ?? 0.5) - (b.ratio ?? 0.5)) < 1e-9 && treeEquals(a.a, b.a) && treeEquals(a.b, b.b);
}
