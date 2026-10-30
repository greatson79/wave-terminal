import { expect, test } from "bun:test";
import { commitPaneRename } from "./panerename";

test("title rename always refreshes authority and shows a failure", async () => {
  const calls: string[] = [];
  const ok = await commitPaneRename(async () => { calls.push("rename"); }, async () => { calls.push("refresh"); }, () => {});
  expect(ok).toBe(true);
  expect(calls).toEqual(["rename", "refresh"]);
  const fail = await commitPaneRename(async () => { throw new Error("rename denied"); },
    async () => { calls.push("refresh-fail"); }, (detail) => { calls.push(detail); });
  expect(fail).toBe(false);
  expect(calls.at(-1)).toBe("refresh-fail");
  expect(calls.at(-2)).toContain("권한");
});
