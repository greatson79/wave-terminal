import { describe, expect, test } from "bun:test";
import { gridLayout, keepManualTree, treeEquals, type GridSeat } from "./gridlayout";
import type { LayoutNode } from "./treededupe";

// 상수 근거(gridlayout.ts 와 동일): 13px 글꼴 기준 70열·12행이 최소 읽기 크기.
const SPEC = { minW: 560, minH: 240, headWeight: 1.5 };
// 창 폭 1280·1728·2560 에서 사이드바(≈280)를 뺀 패널 폭. 높이는 900 기준.
const WIDTHS = [1000, 1450, 2280];
const H = 900;
const COUNTS = [1, 2, 3, 4, 6, 9, 13];

const seats = (n: number, extra: Partial<Record<number, Partial<GridSeat>>> = {}): GridSeat[] =>
  Array.from({ length: n }, (_, i) => ({ sid: i + 1, ...extra[i + 1] }));

interface Rect { sid: number; x: number; y: number; w: number; h: number }

// 트리를 ratio 대로 쪼개 각 pane 의 사각형을 구한다(divider 두께 무시 — 상한 검산용).
function rects(node: LayoutNode, x: number, y: number, w: number, h: number, out: Rect[] = []): Rect[] {
  if (node.type === "pane") { out.push({ sid: node.sid, x, y, w, h }); return out; }
  const r = node.ratio ?? 0.5;
  if (node.dir === "row") {
    rects(node.a, x, y, w * r, h, out);
    rects(node.b, x + w * r, y, w * (1 - r), h, out);
  } else {
    rects(node.a, x, y, w, h * r, out);
    rects(node.b, x, y + h * r, w, h * (1 - r), out);
  }
  return out;
}

describe("gridLayout — 좌석 수 × 폭", () => {
  for (const n of COUNTS) {
    for (const w of WIDTHS) {
      test(`좌석 ${n} · 패널 폭 ${w}: 전원 1회 배치·순서 보존·최소 크기`, () => {
        const res = gridLayout(seats(n), { w, h: H }, SPEC);
        expect(res.tree).not.toBeNull();
        const rs = rects(res.tree!, 0, 0, w, H);
        expect(rs.map((r) => r.sid).sort((a, b) => a - b)).toEqual(seats(n).map((s) => s.sid));
        // 읽는 순서(좌→우, 위→아래)가 좌석 순서와 같다
        const reading = [...rs].sort((a, b) => (Math.abs(a.y - b.y) > 1 ? a.y - b.y : a.x - b.x)).map((r) => r.sid);
        expect(reading).toEqual(seats(n).map((s) => s.sid));
        // 겹침·빈틈 없음: 면적 합 = 패널 면적
        const area = rs.reduce((s, r) => s + r.w * r.h, 0);
        expect(Math.round(area)).toBe(w * H);
        // 폭은 항상 최소 이상(좁으면 minWidth 로 스크롤을 약속)
        for (const r of rs) expect(r.w).toBeGreaterThanOrEqual(SPEC.minW - 1);
        // 높이가 모자라면 minHeight 가 그 부족분을 약속한다
        const need = res.minHeight || H;
        const rs2 = rects(res.tree!, 0, 0, w, Math.max(H, need));
        for (const r of rs2) expect(r.h).toBeGreaterThanOrEqual(SPEC.minH - 1);
      });
    }
  }

  test("열 수는 폭으로만 정해진다: 폭 1000→1열, 1450→2열, 2280→3열 (9석, 4+4+1 보다 3×3)", () => {
    const cols = (w: number) => {
      const rs = rects(gridLayout(seats(9), { w, h: H }, SPEC).tree!, 0, 0, w, H);
      return new Set(rs.map((r) => Math.round(r.y))).size && Math.max(...rs.map((r) => rs.filter((q) => Math.abs(q.y - r.y) < 1).length));
    };
    expect(cols(1000)).toBe(1);
    expect(cols(1450)).toBe(2);
    expect(cols(2280)).toBe(3);
  });

  test("고아 줄 방지: 5석·4열 후보는 3+2 로 균형", () => {
    const rs = rects(gridLayout(seats(5), { w: 2280, h: H }, SPEC).tree!, 0, 0, 2280, H);
    const rowsOf = new Map<number, number>();
    for (const r of rs) rowsOf.set(Math.round(r.y), (rowsOf.get(Math.round(r.y)) ?? 0) + 1);
    expect([...rowsOf.values()].sort()).toEqual([2, 3]);
  });
});

describe("gridLayout — 본부장·역할 자리", () => {
  test("본부장은 첫 칸이고 같은 줄 다른 칸보다 넓다", () => {
    const w = 2280;
    const rs = rects(gridLayout(seats(4, { 3: { head: true } }), { w, h: H }, SPEC).tree!, 0, 0, w, H);
    const head = rs.find((r) => r.sid === 3)!;
    expect(head.x).toBe(0);
    expect(head.y).toBe(0);
    const sameRow = rs.filter((r) => Math.abs(r.y - head.y) < 1 && r.sid !== 3);
    expect(sameRow.length).toBeGreaterThan(0);
    for (const r of sameRow) expect(head.w).toBeGreaterThan(r.w);
    for (const r of sameRow) expect(r.w).toBeGreaterThanOrEqual(SPEC.minW - 1);
  });

  test("본부장이 중간에 있어도 순서 보존 규칙 속에서 첫 칸으로 올라온다", () => {
    const rs = rects(gridLayout(seats(3, { 2: { head: true } }), { w: 1450, h: H }, SPEC).tree!, 0, 0, 1450, H);
    expect(rs.find((r) => r.sid === 2)!.x).toBe(0);
    expect(rs.find((r) => r.sid === 2)!.y).toBe(0);
  });

  test("한 열이면 본부장 가중치는 쓰이지 않는다(폭 전체)", () => {
    const rs = rects(gridLayout(seats(3, { 1: { head: true } }), { w: 1000, h: H }, SPEC).tree!, 0, 0, 1000, H);
    for (const r of rs) expect(r.w).toBe(1000);
  });

  test("master/cso 는 왼쪽 끝 열(3:1), 리뷰어 둘은 오른쪽 끝 열(1:1), 나머지는 가운데", () => {
    const w = 2280;
    const s = seats(6, {
      1: { role: "master" }, 2: { role: "cso" },
      5: { role: "reviewer-gemini" }, 6: { role: "reviewer-codex" },
    });
    const rs = rects(gridLayout(s, { w, h: H }, SPEC).tree!, 0, 0, w, H);
    const by = (sid: number) => rs.find((r) => r.sid === sid)!;
    expect(by(1).x).toBe(0);
    expect(by(2).x).toBe(0);
    expect(by(1).h / by(2).h).toBeCloseTo(3, 5);
    expect(by(5).x + by(5).w).toBeCloseTo(w, 5);
    expect(by(6).x).toBe(by(5).x);
    expect(by(5).h).toBeCloseTo(by(6).h, 5);
    for (const sid of [3, 4]) expect(by(sid).x).toBeGreaterThan(by(1).x + by(1).w - 1);
  });
});

describe("gridLayout — 너무 작은 창·빈 패널", () => {
  test("폭이 최소 폭보다 좁으면 줄이지 않고 minWidth 로 가로 스크롤을 약속", () => {
    const res = gridLayout(seats(3), { w: 400, h: H }, SPEC);
    expect(res.minWidth).toBe(SPEC.minW);
    const rs = rects(res.tree!, 0, 0, res.minWidth, Math.max(H, res.minHeight));
    for (const r of rs) expect(r.w).toBeGreaterThanOrEqual(SPEC.minW - 1);
  });

  test("높이가 모자라면 줄 수 × 최소 높이를 minHeight 로 약속(세로 스크롤)", () => {
    const res = gridLayout(seats(13), { w: 1000, h: 500 }, SPEC);
    expect(res.minHeight).toBe(13 * SPEC.minH);
  });

  test("다 들어가면 minWidth·minHeight 는 0", () => {
    const res = gridLayout(seats(2), { w: 1450, h: H }, SPEC);
    expect(res.minWidth).toBe(0);
    expect(res.minHeight).toBe(0);
  });

  test("빈 패널 → tree null", () => {
    const res = gridLayout([], { w: 1450, h: H }, SPEC);
    expect(res.tree).toBeNull();
    expect(res.minWidth).toBe(0);
    expect(res.minHeight).toBe(0);
  });

  test("패널 크기 0(숨김) → 계산하지 않고 null 이 아닌 단순 한 줄로 안전하게", () => {
    const res = gridLayout(seats(3), { w: 0, h: 0 }, SPEC);
    expect(res.tree).not.toBeNull();
  });
});

describe("수동 비율 보존", () => {
  const manual: LayoutNode = { type: "split", dir: "row", ratio: 0.7, a: { type: "pane", sid: 1 }, b: { type: "pane", sid: 2 } };
  const auto = gridLayout(seats(2), { w: 2280, h: H }, SPEC).tree;

  test("manual=true 면 자동 결과가 있어도 기존 트리(사람이 끈 비율)를 그대로 둔다", () => {
    expect(keepManualTree(true, manual, auto)).toBe(manual);
  });
  test("manual=false/undefined 면 자동 결과를 쓴다(정렬 버튼 = manual 을 내리는 동작)", () => {
    expect(keepManualTree(false, manual, auto)).toBe(auto);
    expect(keepManualTree(undefined, manual, auto)).toBe(auto);
  });
  test("treeEquals: 같은 구조는 true · ratio 하나만 달라도 false", () => {
    expect(treeEquals(auto, JSON.parse(JSON.stringify(auto)))).toBe(true);
    expect(treeEquals(manual, { ...manual, ratio: 0.71 })).toBe(false);
    expect(treeEquals(null, null)).toBe(true);
    expect(treeEquals(null, manual)).toBe(false);
  });
  test("같은 입력이면 같은 트리(재계산이 불필요한 재렌더를 만들지 않는다)", () => {
    const a = gridLayout(seats(6), { w: 1450, h: H }, SPEC).tree;
    const b = gridLayout(seats(6), { w: 1450, h: H }, SPEC).tree;
    expect(treeEquals(a, b)).toBe(true);
  });
});
