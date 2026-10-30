import { expect, test } from "bun:test";
import { resetTime, sidebarUsage, type UsageAccount } from "./usagebar";

const account = (used_pct: number, updated_at = 1000, resets_at: number | null = 5000): UsageAccount => ({
  provider: "claude", account_id: "account-1", label: "Claude", updated_at,
  rate: [{ label: "5h", used_pct, resets_at }],
});

test("remaining thresholds follow green above 50, yellow 20–50, red below 20", () => {
  expect(sidebarUsage([account(49)], [], 1100)[0]).toMatchObject({ remaining: 51, severity: "ok", age: 1 });
  expect(sidebarUsage([account(50)], [], 1100)[0].severity).toBe("warn");
  expect(sidebarUsage([account(80)], [], 1100)[0]).toMatchObject({ remaining: 20, severity: "warn" });
  expect(sidebarUsage([account(81)], [], 1100)[0]).toMatchObject({ remaining: 19, severity: "crit" });
});

test("reset, stale, hidden, and missing measurements stay distinct", () => {
  expect(sidebarUsage([account(12, 1000, 1100)], [], 1600)[0]).toMatchObject({
    reset: true, remaining: null, severity: "none", age: 10,
  });
  expect(sidebarUsage([account(12, 1000)], ["claude:account-1"], 1600)).toEqual([]);
  expect(sidebarUsage([account(12, 1590)], [], 2200)[0].age).toBe(10);
  expect(sidebarUsage([{ ...account(12), updated_at: null, rate: [] }])).toEqual([]);
  expect(sidebarUsage([])).toEqual([]);
});

test("local account-owner labels replace provider email only in the UI model", () => {
  expect(sidebarUsage([account(20)], [], 1100, { "claude:account-1": "Work" })[0]).toMatchObject({
    accountKey: "claude:account-1", provider: "claude", label: "Work", window: "5h", resetsAt: 5000,
  });
});

test("Antigravity shows Gemini and Claude·GPT 5h/7d without inventing unavailable quota", () => {
  const agy: UsageAccount = { provider: "antigravity", account_id: "default", label: "Agy",
    updated_at: 1000, rate: [{ label: "5h", used_pct: 12, resets_at: 5000 }] };
  expect(sidebarUsage([agy], [], 1100).map((r) => [r.group, r.window, r.remaining, r.resetsAt])).toEqual([
    ["Gemini", "5h", 88, 5000], ["Gemini", "7d", null, null],
    ["Claude·GPT", "5h", null, null], ["Claude·GPT", "7d", null, null],
  ]);
  expect(sidebarUsage([agy], ["antigravity:default"], 1100)).toEqual([]);
  agy.rate.push({ label: "Claude·GPT 7d", used_pct: 75, resets_at: 6000 });
  expect(sidebarUsage([agy], [], 1100)[3]).toMatchObject({ group: "Claude·GPT", window: "7d", remaining: 25, resetsAt: 6000 });
});

test("reset time is local and unknown timestamps are explicit", () => {
  const date = new Date(2030, 10, 2, 9, 5);
  expect(resetTime(date.getTime() / 1000)).toBe("리셋 11/2 09:05");
  expect(resetTime(null)).toBe("리셋 미상");
  expect(resetTime(1e20)).toBe("리셋 미상");
});
