import { expect, test } from "bun:test";
import { sidebarUsage } from "./usagebar";

const sample = (used_pct: number, label = "5h", agent = "claude", updated_at = 1000, resets_at: number | null = 2000) =>
  ({ agent, updated_at, rate: [{ label, used_pct, resets_at }] });

test("remaining thresholds and most constrained active surface", () => {
  const rows = sidebarUsage([sample(49), sample(50), sample(80, "7d"), sample(81, "7d")], 1100)[0].rows;
  expect(rows[0]).toMatchObject({ remaining: 50, severity: "warn", age: 1 });
  expect(rows[1]).toMatchObject({ remaining: 19, severity: "crit" });
  expect(sidebarUsage([sample(49)], 1100)[0].rows[0].severity).toBe("ok");
  expect(sidebarUsage([sample(80)], 1100)[0].rows[0].severity).toBe("warn");
});

test("missing, stale and reset measurements are distinct", () => {
  const rows = sidebarUsage([sample(12, "5h", "codex", 1000, 1100)], 1600)[0].rows;
  expect(rows[0]).toMatchObject({ remaining: 88, reset: true, age: 10 });
  expect(rows[1]).toMatchObject({ remaining: null, reset: false, age: null });
  expect(sidebarUsage([])).toEqual([]);
  expect(sidebarUsage([{ ...sample(12), updated_at: 1590, rate_updated_at: 1000 }], 1600)[0].rows[0].age).toBe(10);
  expect(sidebarUsage([sample(99, "5h", "codex", 1000, 1100), sample(5, "5h", "codex", 1600, 2000)], 1600)[0].rows[0])
    .toMatchObject({ remaining: 95, reset: false });
});

test("equally constrained samples use the freshest rate update", () => {
  const older = sample(50, "5h", "claude", 1800);
  const newer = { ...sample(50, "5h", "claude", 3000), rate_updated_at: 2940 };
  expect(sidebarUsage([older, newer], 3000)[0].rows[0].age).toBe(1);
});
