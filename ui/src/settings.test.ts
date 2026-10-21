import { describe, expect, it } from "bun:test";
import { BAR_IDS, DEFAULT_SETTINGS, readSettings } from "./settings";

describe("settings", () => {
  it("moves valid legacy colors and fonts without losing defaults", () => {
    const s = readSettings(null, "#123abc", "Menlo");
    expect(s.look.bg).toBe("#123abc");
    expect(s.look.font).toBe("Menlo");
    expect(s.bar.order).toEqual(BAR_IDS);
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
});
