import { expect, test } from "bun:test";
import { sidebarUsage, type UsageAccount } from "./usagebar";

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
  expect(sidebarUsage([account(20)], [], 1100, { "claude:account-1": "Work" })[0].label).toBe("Work");
});
