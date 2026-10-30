import { describe, expect, it } from "bun:test";
import { BAR_IDS, DEFAULT_SETTINGS, readSettings } from "./settings";

describe("settings", () => {
  it("moves valid legacy colors and fonts without losing defaults", () => {
    const s = readSettings(null, "#123abc", "Menlo");
    expect(s.look.bg).toBe("#123abc");
    expect(s.look.font).toBe("Menlo");
    expect(s.bar.order).toEqual(BAR_IDS);
  });

  it("keeps changes made in 0.2.4 during a rollback and return to 0.2.5", () => {
    const saved = JSON.stringify({ ver: 1, look: { bg: "#123abc", font: "Menlo" } });
    const changed = readSettings(saved, "#abcdef", "Monaco");
    expect(changed.look.bg).toBe("#abcdef");
    expect(changed.look.font).toBe("Monaco");
    const reset = readSettings(saved, null, null);
    expect(reset.look.bg).toBeNull();
    expect(reset.look.font).toBeNull();
  });

  it("uses defaults for broken JSON and rejects individual out-of-range values", () => {
    expect(readSettings("{", null, null)).toEqual(DEFAULT_SETTINGS);
    const s = readSettings(JSON.stringify({ ver: 1, look: {
      bg: "red", font: "not installed", titleSize: 100, titleWeight: 700,
      titleIcon: false, bodyWeight: 500, menuWeight: 900, menuScale: 151,
    } }), null, null);
    expect(s.look).toEqual({ ...DEFAULT_SETTINGS.look,
      titleWeight: 700, titleIcon: false, bodyWeight: 500 });
  });

  it("keeps saved order, appends new buttons visibly, and drops removed buttons", () => {
    const s = readSettings(JSON.stringify({ ver: 1, bar: {
      order: ["close", "removed", "new", "close"], hidden: ["new", "removed"],
    } }), null, null);
    expect(s.bar.order).toEqual(["close", "new", ...BAR_IDS.filter((id) => id !== "close" && id !== "new")]);
    expect(s.bar.hidden).toEqual(["new"]);
    expect(readSettings(JSON.stringify({ ver: 1, bar: { order: ["new"], hidden: [] } }), null, null,
      [...BAR_IDS, "future"]).bar.order.at(-1)).toBe("future");
  });

  it("migrates and validates layout and help-panel preferences", () => {
    expect(readSettings(null, null, null)).toMatchObject({
      layout: { sidebarSide: "left" }, panels: { helpCollapsed: false },
      header: { daemonCollapsed: true },
      usage: { enabled: true, hiddenAccounts: [] },
    });
    expect(readSettings(JSON.stringify({ ver: 1, layout: { sidebarSide: "right" },
      panels: { helpCollapsed: true }, header: { daemonCollapsed: false },
      usage: { enabled: false, hiddenAccounts: ["claude:id", "codex:id"] } }), null, null)).toMatchObject({
      layout: { sidebarSide: "right" }, panels: { helpCollapsed: true },
      header: { daemonCollapsed: false },
      usage: { enabled: false, hiddenAccounts: ["claude:id", "codex:id"] },
    });
    expect(readSettings(JSON.stringify({ ver: 1, layout: { sidebarSide: "outside" },
      panels: { helpCollapsed: "true" }, header: { daemonCollapsed: "yes" } }), null, null)).toMatchObject({
      layout: { sidebarSide: "left" }, panels: { helpCollapsed: false },
      header: { daemonCollapsed: true },
    });
  });
});
