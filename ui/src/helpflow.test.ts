import { test, expect } from "bun:test";
import { sendPreparedHelp } from "./helpflow";

test("help sends in one action and only confirms a numbered receipt", async () => {
  const calls: string[] = [];
  const prepare = async () => { calls.push("prepare"); return { message: "help" }; };
  const send = async () => { calls.push("send"); return { id: "R-123" }; };
  expect(await sendPreparedHelp(null, prepare, send)).toBe("R-123");
  expect(calls).toEqual(["prepare", "send"]);
  await expect(sendPreparedHelp(null, prepare, async () => ({ id: "" }))).rejects.toThrow("접수 번호");
});
