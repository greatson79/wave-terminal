import { expect, test } from "bun:test";
import { dedupeTree, socketOwnsSid, treeSids } from "./treededupe";

test("duplicate pane is collapsed and ownership checks all workspaces", () => {
  const duplicate = { type: "split" as const, dir: "row" as const, a: { type: "pane" as const, sid: 1 }, b: { type: "pane" as const, sid: 1 } };
  expect(treeSids(dedupeTree(duplicate))).toEqual([1]);
  expect(socketOwnsSid([{ socket: "s", tree: { type: "pane", sid: 1 } }, { socket: "s", tree: { type: "pane", sid: 2 } }], "s", 2)).toBe(true);
  expect(socketOwnsSid([{ socket: "other", tree: { type: "pane", sid: 2 } }], "s", 2)).toBe(false);
});

test("unchanged tree keeps node identity so a dragged ratio survives save", () => {
  const tree = { type: "split" as const, dir: "row" as const, ratio: 0.6, a: { type: "pane" as const, sid: 1 }, b: { type: "pane" as const, sid: 2 } };
  const out = dedupeTree(tree);
  expect(out).toBe(tree);
  tree.ratio = 0.5; // 드래그가 노드를 직접 바꾼다
  expect((dedupeTree(tree) as typeof tree).ratio).toBe(0.5);
});

test("a collapsed duplicate still yields a fresh node with the surviving side", () => {
  const dup = { type: "split" as const, dir: "row" as const, a: { type: "pane" as const, sid: 1 }, b: { type: "pane" as const, sid: 1 } };
  expect(dedupeTree(dup)).toEqual({ type: "pane", sid: 1 });
});
