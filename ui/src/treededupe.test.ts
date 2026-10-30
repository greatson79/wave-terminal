import { expect, test } from "bun:test";
import { dedupeTree, socketOwnsSid, treeSids } from "./treededupe";

test("duplicate pane is collapsed and ownership checks all workspaces", () => {
  const duplicate = { type: "split" as const, dir: "row" as const, a: { type: "pane" as const, sid: 1 }, b: { type: "pane" as const, sid: 1 } };
  expect(treeSids(dedupeTree(duplicate))).toEqual([1]);
  expect(socketOwnsSid([{ socket: "s", tree: { type: "pane", sid: 1 } }, { socket: "s", tree: { type: "pane", sid: 2 } }], "s", 2)).toBe(true);
  expect(socketOwnsSid([{ socket: "other", tree: { type: "pane", sid: 2 } }], "s", 2)).toBe(false);
});
