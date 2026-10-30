import { expect, test } from "bun:test";
import { transferTrees, treeSids } from "./transfer";

test("pane transfer moves one sid atomically and rejects duplicates", () => {
  const pane = (sid: number) => ({ type: "pane" as const, sid });
  const moved = transferTrees(pane(1), pane(2), 1);
  expect(treeSids(moved?.src ?? null)).toEqual([]);
  expect(treeSids(moved?.dest ?? null)).toEqual([2, 1]);
  expect(transferTrees(pane(1), pane(1), 1)).toBeNull();
});
