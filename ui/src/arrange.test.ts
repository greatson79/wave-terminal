import { expect, mock, test } from "bun:test";
import { readFileSync } from "node:fs";
import { readSettings } from "./settings";
import * as layouts from "./gridlayout";
import { normalizeWorkspaces } from "./wsrestore";
import { dedupeTree, treeSids, type LayoutNode } from "./treededupe";

// Exercise actual app functions without loading Tauri or contacting a daemon.
const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const transpiler = new Bun.Transpiler({ loader: "ts" });
function block(start: string, end: string) {
  const at = source.indexOf(start), until = source.indexOf(end, at);
  expect(at).toBeGreaterThanOrEqual(0);
  expect(until).toBeGreaterThan(at);
  return transpiler.transformSync(source.slice(at, until));
}
const oldColumns = () => layouts.gridLayout(Array.from({ length: 7 }, (_, i) => ({ sid: i + 1 })),
  { w: 1512, h: 900 }, layouts.gridSpecFor(13), "columns");
const mixed = (): LayoutNode => ({ type: "split", dir: "row", ratio: .8,
  a: { type: "split", dir: "col", ratio: .75, a: { type: "pane", sid: 1 }, b: { type: "pane", sid: 2 } },
  b: { type: "pane", sid: 3 } });
type Workspace = { tree: LayoutNode | null; manual?: boolean; socket?: string };
function harness(initial: Workspace, arrange = "columns", cachedRoles = new Map<number, { role: string }>()) {
  let ws = initial, calls = 0;
  const modes = new Function(block("const GRID_RESIZE_DEBOUNCE_MS", "\nfunction autoGrid") + "return arrangeModes;")();
  const root = { offsetWidth: 1512, offsetHeight: 900, style: { overflow: "auto" },
    firstElementChild: { style: { minWidth: "3906px", minHeight: "999px" } },
    innerHTML: "", appendChild: (_: unknown) => {} };
  const equalWidthLayout = mock(layouts.equalWidthLayout), gridLayout = mock(layouts.gridLayout);
  const autoGrid = mock(new Function("root", "collectSids", "arrangeModes", "equalWidthLayout", "paneKey", "paneTitleState", "surfaceTitles", "isHqLeadName", "gridLayout", "gridSpecFor", "fontSize", "settings",
    block("function autoGrid(", "\nfunction applyGridSize") + "return autoGrid;")(
    root, treeSids, modes, equalWidthLayout, (sid: number) => sid, cachedRoles, new Map(), () => false,
    gridLayout, layouts.gridSpecFor, 13, { layout: { arrange } }));
  const applyGridSize = new Function("root", block("function applyGridSize(", "\n// 창·패널") + "return applyGridSize;")(root);
  const panes = new Map(treeSids(ws.tree).map(sid => [sid, {}]));
  const render = new Function("coalesceHqPanels", "panes", "root", "current", "dedupeTree", "autoGrid", "keepManualTree", "renderNode", "applyGridSize", "hierarchyEnabled", "renderWsTabs", "requestAnimationFrame", "saveLayout",
    block("function render()", "\n// WP-10:") + "return render;")(
    () => {}, new Map(), root, () => ws, dedupeTree, autoGrid, layouts.keepManualTree, () => ({}), applyGridSize, false, () => {}, () => {}, () => {});
  const actionEqualize = new Function("current", "arrangeModes", "collectSids", "panes", "paneKey", "invoke", "autoGrid", "render",
    block("async function actionEqualize(", "\n// ---------- workspace tabs") + "return actionEqualize;")(
    () => ws, modes, treeSids, panes, (sid: number) => sid,
    async () => { calls++; return { surfaces: [] }; }, autoGrid, render);
  const reflow = new Function("current", "autoGrid", "treeEquals", "applyGridSize", "document", "render",
    block("function reflowGrid()", "\nnew ResizeObserver") + "return reflowGrid;")(
    () => ws, autoGrid, layouts.treeEquals, applyGridSize, { activeElement: null }, render);
  const actionNew = new Function("current", "newSurface", "render", "setFocus",
    block("async function actionNew()", "\nasync function actionSplit") + "return actionNew;")(
    () => ws, async () => 8, render, () => {});
  return { root, modes, autoGrid, equalWidthLayout, gridLayout, applyGridSize, render, actionEqualize, reflow, actionNew,
    calls: () => calls, switchTo: (next: Workspace) => { ws = next; }, panes };
}
function restore(raw: string | null, workspaces: Workspace[]) {
  const settings = readSettings(raw, null, null);
  const code = source.slice(source.indexOf("workspaces = saved.workspaces;"), source.indexOf("groups = Array.isArray(saved.groups)"));
  const legacyFlag = (() => { try { return JSON.parse(raw ?? "null")?.layout?.arrange === undefined; } catch { return true; } })();
  const restored = new Function("saved", "preserveSavedLayoutOnUpgrade", `let workspaces; ${code} return workspaces;`)({ workspaces }, legacyFlag);
  return { settings, restored: restored as Workspace[] };
}

test("fresh workspace uses real equal widths despite legacy global preference (behavioral RED)", () => {
  const ws = { tree: oldColumns().tree };
  const result = harness(ws).autoGrid(ws);
  console.log(`fresh autoGrid minWidth=${result.minWidth}`);
  expect(result.minWidth).toBe(0);
  expect(result.minHeight).toBe(0);
  expect(result.tree.ratio).toBeCloseTo((1512 - 6) / 7 / 1511, 9);
});
for (const raw of [null, "{", ...["columns", "grid", "equal"].map(arrange => JSON.stringify({ ver: 1, layout: { arrange } }))]) {
  test(`every restored tree stays locked on first and repeated restore: ${raw}`, () => {
    const workspaces = [false, true, undefined].map(manual => ({ tree: mixed(), manual }));
    const geometry = workspaces.map(ws => JSON.stringify(ws.tree));
    const first = restore(raw, workspaces);
    const second = restore(JSON.stringify(first.settings), first.restored);
    expect(first.restored.map(ws => ws.manual)).toEqual([true, true, true]);
    expect(second.restored.map(ws => ws.manual)).toEqual([true, true, true]);
    expect(second.restored.map(ws => JSON.stringify(ws.tree))).toEqual(geometry);
    harness(second.restored[0]).reflow();
    expect(JSON.stringify(second.restored[0].tree)).toBe(geometry[0]);
  });
}

test("Theo condition: restored locked 0.3.4 columns, seven seats at 1512px clear overflow and preserve ratios", () => {
  const old = oldColumns();
  expect(old.minWidth).toBe(3906);
  const ws = restore(JSON.stringify({ ver: 1, layout: { arrange: "columns" } }), [{ tree: old.tree, manual: false }]).restored[0];
  const geometry = JSON.stringify(ws.tree);
  const h = harness(ws);
  h.applyGridSize(old);
  expect(h.root.firstElementChild.style.minWidth).toBe("3906px");
  expect(h.root.style.overflow).toBe("auto");
  expect(ws.manual).toBe(true);
  h.render(); // Actual render must pass null to applyGridSize for a locked tree.
  expect(h.root.firstElementChild.style).toEqual({ minWidth: "", minHeight: "" });
  expect(h.root.style.overflow).toBe("");
  expect(JSON.stringify(ws.tree)).toBe(geometry);
  // Independent geometry, including every 1px divider, checks the right edge.
  function rightEdge(node: LayoutNode, x: number, width: number): number {
    if (node.type === "pane") { expect(width).toBeGreaterThan(0); return x + width; }
    const a = (width - 1) * (node.ratio ?? .5);
    return Math.max(rightEdge(node.a, x, a), rightEdge(node.b, x + a + 1, width - a - 1));
  }
  expect(rightEdge(ws.tree!, 0, 1512)).toBeCloseTo(1512, 9);
  console.log("Theo condition PASS: minWidth=''; minHeight=''; overflow=''; rightEdge=1512; ratios unchanged");
});

test("explicit equal unlocks, keeps topology and stack ratios, clears sizing, skips daemon", async () => {
  const ws = { tree: mixed(), manual: true };
  const h = harness(ws);
  await h.actionEqualize("equal");
  expect(ws.manual).toBe(false);
  expect(ws.tree!.type).toBe("split");
  expect((ws.tree as any).a.ratio).toBe(.75);
  expect(treeSids(ws.tree)).toEqual([1, 2, 3]);
  expect((ws.tree as any).ratio).toBe(.5);
  expect(h.calls()).toBe(0);
  expect(h.root.firstElementChild.style.minWidth).toBe("");
  expect(h.root.style.overflow).toBe("");
});

test("per-workspace grid/equal choices stay isolated on resize and new-seat insertion", async () => {
  const a = { tree: oldColumns().tree, manual: true };
  const b = { tree: oldColumns().tree, manual: false };
  const h = harness(a, "grid");
  await h.actionEqualize("grid");
  const gridBefore = JSON.stringify(a.tree);
  h.switchTo(b);
  await h.actionEqualize("equal");
  h.reflow();
  expect(h.modes.get(a)).toBe("grid");
  expect(h.modes.get(b)).toBe("equal");
  expect(JSON.stringify(a.tree)).toBe(gridBefore);
  expect(h.autoGrid(b).minWidth).toBe(0);
  h.switchTo(a);
  await h.actionNew();
  const expected = layouts.gridLayout(treeSids(a.tree).map(sid => ({ sid, role: null, head: false })),
    { w: 1512, h: 900 }, layouts.gridSpecFor(13), "grid");
  expect(a.tree).toEqual(expected.tree);
  expect(h.autoGrid({ tree: oldColumns().tree }).minWidth).toBe(0);
});

test("manual drag/split locks survive new-seat insertion without reweighting existing ratios", async () => {
  const ws = { tree: mixed(), manual: true };
  const before = JSON.stringify(ws.tree);
  const h = harness(ws);
  await h.actionNew();
  expect(ws.manual).toBe(true);
  expect(JSON.stringify((ws.tree as any).a)).toBe(before);
  expect(treeSids(ws.tree)).toEqual([1, 2, 3, 8]);
});

test("grid retains live-pane filtering and cached-role offline fallback", async () => {
  const ws = { tree: oldColumns().tree, manual: true };
  const h = harness(ws);
  h.panes.delete(7);
  await h.actionEqualize("grid");
  expect(treeSids(ws.tree)).toEqual([1, 2, 3, 4, 5, 6]);
  const offline = new Function("current", "arrangeModes", "collectSids", "panes", "paneKey", "invoke", "autoGrid", "render",
    block("async function actionEqualize(", "\n// ---------- workspace tabs") + 'return actionEqualize("grid");');
  const cached = new Map([[1, { role: "master" }], [2, { role: "cso" }],
    [6, { role: "reviewer-gemini" }], [7, { role: "reviewer-codex" }]]);
  const saved = { tree: oldColumns().tree, manual: true };
  const fallback = harness(saved, "grid", cached);
  await offline(() => saved, fallback.modes, treeSids, fallback.panes, (sid: number) => sid,
    async () => { throw new Error("offline"); }, fallback.autoGrid, fallback.render);
  const expected = layouts.gridLayout(treeSids(oldColumns().tree).map(sid => ({ sid, role: cached.get(sid)?.role ?? null, head: false })),
    { w: 1512, h: 900 }, layouts.gridSpecFor(13), "grid");
  expect(saved.tree).toEqual(expected.tree); // Includes master/cso 3:1 and reviewer 1:1 stacks.
  expect((saved.tree as any).a.ratio).toBe(.75);
  expect((saved.tree as any).b.b.ratio).toBe(.5);
});

test("arrange menu saves equal/grid modes and keeps standard and sidebar actions", async () => {
  const settings = readSettings(null, null, null);
  const calls: string[] = [];
  let listener: (event: any) => void = () => {};
  let items: { label: string; action: () => unknown }[] = [];
  new Function("document", "showCtxMenu", "settings", "saveSettings", "actionEqualize", "applyLayoutSettings",
    block('document.getElementById("btn-equalize")!', 'document.getElementById("btn-close")!'))(
    { getElementById: () => ({ addEventListener: (_: string, cb: typeof listener) => { listener = cb; } }) },
    (_x: number, _y: number, menu: typeof items) => { items = menu; }, settings,
    () => calls.push(`save:${settings.layout.arrange}`), (mode = "equal") => calls.push(`arrange:${mode}`), () => calls.push("sidebar"));
  listener({ currentTarget: { getBoundingClientRect: () => ({ left: 0, bottom: 10 }) } });
  expect(items.map(i => i.label)).toEqual(["너비균등배치", "격자", "역할별 표준 배치", "사이드바 접기"]);
  await items[1].action(); await items[0].action(); await items[2].action(); await items[3].action();
  expect(calls).toEqual(["save:grid", "arrange:grid", "save:equal", "arrange:equal", "arrange:equal", "save:equal", "sidebar"]);
  expect(settings.layout.sidebarCollapsed).toBe(true);
});


test("restored grid and equal tabs stay locked across restart until explicitly arranged", async () => {
  const a = { id: 1, name: "A", tree: oldColumns().tree, manual: true };
  const b = { id: 2, name: "B", tree: JSON.parse(JSON.stringify(oldColumns().tree),
    (key, value) => key === "sid" ? value + 7 : value) as LayoutNode, manual: true };
  const before = harness(a);
  await before.actionEqualize("grid");
  before.switchTo(b);
  await before.actionEqualize("equal");
  const savedTrees = [a, b].map(ws => JSON.stringify(ws.tree));
  expect(savedTrees[0]).not.toBe(savedTrees[1]);

  let serialized = "";
  const save = new Function("workspaces", "normalizeWorkspaces", "localStorage", "LAYOUT_KEY",
    `let groups = [], activeWs = 1, wsCounter = 2, groupCounter = 0, renameRevision;
     ${block("function normalizeGroups(", "\nfunction collectSids(")}
     return saveLayout;`)([a, b], normalizeWorkspaces,
    { setItem: (_key: string, value: string) => { serialized = value; } }, "cys-layout-v2");
  save(); // Actual saveLayout, including real workspace/group normalization and JSON encoding.
  const saved = JSON.parse(serialized);
  expect(saved.workspaces.map((ws: Workspace) => JSON.stringify(ws.tree))).toEqual(savedTrees);
  const restored = restore(JSON.stringify(readSettings(null, null, null)), saved.workspaces).restored;
  expect(restored.map(ws => ws.manual)).toEqual([true, true]);
  expect(restored[0]).not.toBe(a);
  expect(restored[1]).not.toBe(b);
  const fresh = harness(restored[0]);
  expect(restored.map(ws => fresh.modes.has(ws))).toEqual([false, false]);

  function widths(node: LayoutNode, width: number): number[] {
    if (node.type === "pane") { expect(width).toBeGreaterThan(0); return [width]; }
    if (node.dir === "col") return [...widths(node.a, width), ...widths(node.b, width)];
    const left = (width - 1) * (node.ratio ?? .5);
    return [...widths(node.a, left), ...widths(node.b, width - left - 1)];
  }
  for (const [index, ws] of restored.entries()) {
    fresh.switchTo(ws);
    fresh.root.offsetWidth = 1512;
    fresh.render();
    expect(JSON.stringify(ws.tree)).toBe(savedTrees[index]);
    fresh.root.offsetWidth = 1200;
    fresh.reflow(); // Actual ResizeObserver target, with a changed viewport width.
    expect(ws.manual).toBe(true);
    expect(JSON.stringify(ws.tree)).toBe(savedTrees[index]);
    expect(fresh.root.firstElementChild.style).toEqual({ minWidth: "", minHeight: "" });
    expect(fresh.root.style.overflow).toBe("");
    expect(widths(ws.tree!, 1200).every(width => width <= 1200)).toBe(true);
  }
  expect(fresh.autoGrid).not.toHaveBeenCalled();
  expect(fresh.equalWidthLayout).not.toHaveBeenCalled();
  expect(fresh.gridLayout).not.toHaveBeenCalled();

  fresh.switchTo(restored[0]);
  await fresh.actionEqualize("grid");
  const expectedGrid = layouts.gridLayout(treeSids(JSON.parse(savedTrees[0])).map(sid => ({ sid, role: null, head: false })),
    { w: 1200, h: 900 }, layouts.gridSpecFor(13), "grid");
  expect(restored[0].manual).toBe(false);
  expect(restored[0].tree).toEqual(expectedGrid.tree);
  expect(fresh.gridLayout).toHaveBeenCalled();
  expect(fresh.equalWidthLayout).not.toHaveBeenCalled();
  fresh.switchTo(restored[1]);
  await fresh.actionEqualize("equal");
  expect(restored[1].manual).toBe(false);
  expect(fresh.modes.get(restored[1])).toBe("equal");
  expect(fresh.equalWidthLayout).toHaveBeenCalled();
  const equalWidths = widths(restored[1].tree!, 1200);
  expect(equalWidths).toHaveLength(7);
  for (const width of equalWidths) expect(width).toBeCloseTo((1200 - 6) / 7, 9);
  console.log("restart PASS: 2 locked trees byte-equal after render/resize; autoGrid/equalWidthLayout/gridLayout calls=0; minWidth/minHeight/overflow cleared; explicit grid/equal verified");
});
