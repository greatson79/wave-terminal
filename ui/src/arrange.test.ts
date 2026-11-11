import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { readSettings } from "./settings";

// Exercise the real wiring without loading the app or contacting a daemon.
const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const transpiler = new Bun.Transpiler({ loader: "ts" });
function block(start: string, end: string) {
  const at = source.indexOf(start);
  expect(at).toBeGreaterThanOrEqual(0);
  return transpiler.transformSync(source.slice(at, source.indexOf(end, at)));
}

test("arrange menu saves each mode and re-arranges; standard and sidebar actions remain available", async () => {
  const settings = readSettings(null, null, null);
  const calls: string[] = [];
  let listener: (event: any) => void = () => {};
  let items: { label: string; action: () => unknown }[] = [];
  new Function("document", "showCtxMenu", "settings", "saveSettings", "actionEqualize", "applyLayoutSettings",
    block('document.getElementById("btn-equalize")!', 'document.getElementById("btn-close")!'))(
    { getElementById: () => ({ addEventListener: (_: string, cb: typeof listener) => { listener = cb; } }) },
    (_x: number, _y: number, menu: typeof items) => { items = menu; }, settings,
    () => calls.push(`save:${settings.layout.arrange}`), () => calls.push(`arrange:${settings.layout.arrange}`),
    () => calls.push("sidebar"));
  listener({ currentTarget: { getBoundingClientRect: () => ({ left: 0, bottom: 10 }) } });
  expect(items.map((i) => i.label)).toEqual(["세로 열(기본)", "격자", "역할별 표준 배치", "사이드바 접기"]);
  await items[1].action();
  expect(settings.layout.arrange).toBe("grid");
  await items[0].action();
  expect(settings.layout.arrange).toBe("columns");
  expect(calls).toEqual(["save:grid", "arrange:grid", "save:columns", "arrange:columns"]);
  await items[2].action();
  await items[3].action();
  expect(settings.layout.sidebarCollapsed).toBe(true);
  expect(calls.slice(-3)).toEqual(["arrange:columns", "save:columns", "sidebar"]);
});

test("autoGrid forwards the saved preference to the pure layout module", () => {
  for (const arrange of ["columns", "grid"]) {
    let received: unknown;
    const run = new Function("root", "paneKey", "paneTitleState", "surfaceTitles", "isHqLeadName", "gridLayout", "gridSpecFor", "fontSize", "settings",
      `${block("function autoGrid(", "\nfunction applyGridSize")} return autoGrid({}, undefined, [1]);`);
    run({ offsetWidth: 1500, offsetHeight: 900 }, () => "one", new Map(), new Map(), () => false,
      (...args: unknown[]) => { received = args[3]; }, () => ({}), 13, { layout: { arrange } });
    expect(received).toBe(arrange);
  }
});

test("saved trees are protected until arrange is explicitly requested", () => {
  const restore = source.slice(source.indexOf("workspaces = saved.workspaces;"), source.indexOf("groups = Array.isArray(saved.groups)"));
  const saved = { workspaces: [{ tree: { type: "pane", sid: 1 }, manual: false }, { tree: { type: "pane", sid: 2 }, manual: true }] };
  const restored = new Function("saved", `let workspaces; ${restore} return workspaces;`)(saved);
  expect(restored.map((ws: any) => ws.manual)).toEqual([true, true]);
  expect(restored[0].tree).toBe(saved.workspaces[0].tree);
  const ws = { tree: saved.workspaces[0].tree, manual: true };
  let calculated = false;
  new Function("current", "autoGrid", block("function reflowGrid()", "\nnew ResizeObserver") + "reflowGrid();")(
    () => ws, () => { calculated = true; });
  expect(calculated).toBe(false);
});

test("explicit arrange releases a saved tree and renders the selected automatic layout", async () => {
  const ws = { tree: { type: "pane", sid: 1 }, manual: true, socket: undefined };
  const automatic = { type: "pane", sid: 2 };
  let rendered = false;
  const run = new Function("current", "collectSids", "panes", "paneKey", "invoke", "autoGrid", "render",
    block("async function actionEqualize()", "\n// ---------- workspace tabs") + "return actionEqualize();");
  await run(() => ws, () => [1], new Map([["one", {}]]), () => "one",
    async () => { throw new Error("offline: use cached roles"); }, () => ({ tree: automatic }), () => { rendered = true; });
  expect(ws.manual).toBe(false);
  expect(ws.tree).toBe(automatic);
  expect(rendered).toBe(true);
});
