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
    reset: true, remaining: 88, severity: "ok", age: 10,
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

test("Antigravity merges empty account and normalizes measured English windows", () => {
  const agy: UsageAccount = { provider: "antigravity", account_id: "cli", label: "Antigravity (agy)",
    updated_at: 1000, rate: [
      { label: "Claude and GPT models 7d", used_pct: 75, resets_at: 6000 },
      { label: "Gemini 4 Argon 7d", used_pct: 55, resets_at: 6000 },
      { label: "Gemini Models 5h", used_pct: 12, resets_at: 5000 },
      { label: "Claude and GPT models 5h", used_pct: 30, resets_at: 5000 },
      { label: "Gemini Models 7d", used_pct: 20, resets_at: 6000 },
    ] };
  const empty: UsageAccount = { ...agy, account_id: "default", updated_at: null, rate: [] };
  const rows = sidebarUsage([empty, agy], [], 1100);
  expect(rows.map((r) => [r.accountKey, r.group, r.window, r.remaining])).toEqual([
    ["antigravity:cli", "Gemini", "5h", 88],
    ["antigravity:cli", "Gemini", "7d", 80],
    ["antigravity:cli", "Claude·GPT", "5h", 70],
    ["antigravity:cli", "Claude·GPT", "7d", 25],
    ["antigravity:cli", "Argon", "7d", 45],
  ]);
  expect(sidebarUsage([empty, agy], ["antigravity:default"], 1100)).toEqual([]);
});

test("local account order puts Fable rows inside the matching Claude owner group", () => {
  const simple = (provider: string): UsageAccount => ({ provider, account_id: "default", label: provider,
    updated_at: 1000, rate: [{ label: "7d", used_pct: 10, resets_at: 6000 }] });
  const rows = sidebarUsage([simple("fable"), simple("antigravity"), simple("codex"), simple("claude")], [], 1100,
    { "claude:default": "Main", "fable:default": "Main" },
    ["claude:default", "fable:default", "codex:default", "antigravity:default"]);
  expect(rows.map((r) => r.provider)).toEqual(["claude", "fable", "codex", "antigravity"]);
  expect(rows[1]).toMatchObject({ accountKey: "claude:default", group: "Fable", window: "7d" });
  expect(rows.map((r) => r.remaining)).toEqual([90, 90, 90, 90]);
  expect(sidebarUsage([simple("fable"), simple("codex")], [], 1100).map((r) => r.provider))
    .toEqual(["fable", "codex"]);
});

test("reset time is local and unknown timestamps are explicit", () => {
  const date = new Date(2030, 10, 2, 9, 5);
  expect(resetTime(date.getTime() / 1000)).toBe("리셋 11/2 09:05");
  expect(resetTime(null)).toBe("리셋 미상");
  expect(resetTime(1e20)).toBe("리셋 미상");
});
