import { beforeEach, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { adoptHqOf, configureHierarchyNaming, paneRehomeTarget } from "./hierarchy";
import { parseNamingColors } from "./hqbadge";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const start = source.indexOf("let namingMtime:");
const end = source.indexOf("\nconst ghostStrikes", start);
const code = new Bun.Transpiler({ loader: "ts" }).transformSync(source.slice(start, end));
const naming = JSON.stringify({ roles: { worker: { name: "Mina", label: "연구본부·본부장" } } });

function setup() {
  const calls: string[] = [];
  let mtime: string | null = "1", raw: string | null = naming, failed = false;
  const invoke = async (cmd: string) => {
    calls.push(cmd);
    if (failed) throw new Error("unreadable");
    return cmd === "naming_config_mtime" ? mtime : raw;
  };
  const poll = new Function("invoke", "configureHierarchyNaming", "parseNamingColors", `
    let hierarchyEnabled = false, hqColors = {};
    ${code}
    return refreshNamingConfig;
  `)(invoke, configureHierarchyNaming, parseNamingColors) as () => Promise<boolean>;
  return { calls, poll, set: (stamp: string | null, text = naming) => { mtime = stamp; raw = text; },
    fail: () => { failed = true; } };
}

beforeEach(() => configureHierarchyNaming(null));

test("unchanged mtime never re-reads naming.json", async () => {
  const h = setup();
  await h.poll();
  await h.poll();
  await h.poll();
  expect(h.calls.filter((cmd) => cmd === "naming_config")).toHaveLength(1);
  expect(h.calls.filter((cmd) => cmd === "naming_config_mtime")).toHaveLength(3);
});

test("changed mtime reloads and makes existing panes eligible for regrouping", async () => {
  const h = setup();
  h.set(null);
  expect(await h.poll()).toBe(false);
  expect(adoptHqOf("Mina·작업·s21")).toBeNull();
  h.set("2");
  expect(await h.poll()).toBe(true);
  expect(adoptHqOf("Mina·작업·s21")).toBe("Mina");
  expect(paneRehomeTarget("기본", "Mina·작업·s21")).toBe("Mina");
  expect(await h.poll()).toBe(false);
});

test("missing, malformed and unreadable naming keep the last good grouping", async () => {
  const h = setup();
  await h.poll();
  h.set(null);
  expect(await h.poll()).toBe(false);
  h.set("2", '{"roles":');
  expect(await h.poll()).toBe(false);
  expect(await h.poll()).toBe(false);
  expect(h.calls.filter((cmd) => cmd === "naming_config")).toHaveLength(2);
  h.fail();
  expect(await h.poll()).toBe(false);
  expect(adoptHqOf("Mina·作業·s21")).toBe("Mina");
});

test("reload is inside existing serialized adoption poll before regrouping", () => {
  const poll = source.slice(source.indexOf("async function refreshPaneTitles()"), source.indexOf("setInterval(refreshPaneTitles, 3000)"));
  expect(poll.indexOf("refreshing = true")).toBeLessThan(poll.indexOf("await refreshNamingConfig()"));
  expect(poll.indexOf("await refreshNamingConfig()")).toBeLessThan(poll.indexOf("paneRehomeTarget("));
  expect(poll).toContain("let adopted = await refreshNamingConfig()");
  expect(source.match(/setInterval\(refreshPaneTitles, 3000\)/g)).toHaveLength(1);
});

test("existing adoption poll moves an already open pane after naming appears", async () => {
  const { runInNewContext } = await import("node:vm");
  const hierarchy = await import("./hierarchy");
  const { shouldAdoptSurface } = await import("./surfaceadopt");
  const helpers = source.slice(source.indexOf("function collectSids("), source.indexOf("// ---------- pane lifecycle"));
  const poll = source.slice(source.indexOf("async function refreshPaneTitles()"), source.indexOf("\nsetInterval(refreshPaneTitles"));
  const workspaces: any[] = [{ id: 1, name: "기본", tree: { type: "pane", sid: 1 } }];
  const pane = { usageEl: {}, roleEl: {}, titleEl: {} };
  let mtime: string | null = null, reads = 0, renders = 0;
  const context: any = {
    ...hierarchy, shouldAdoptSurface, parseNamingColors,
    started: true, refreshing: false, hierarchyEnabled: false, hqColors: {},
    workspaces, wsCounter: 2, autoRemoveExited: false, focusedSid: 1,
    surfaceTitles: new Map(), paneTitleState: new Map(), panes: new Map([["1", pane]]), ghostStrikes: new Map(),
    current: () => workspaces[0], paneKey: (sid: number) => String(sid),
    invoke: async (cmd: string) => {
      if (cmd === "naming_config_mtime") return mtime;
      if (cmd === "naming_config") { reads++; return naming; }
      if (cmd === "org_status") return { surfaces: [] };
      if (cmd === "list_surfaces") return { surfaces: [{ surface_id: 1, title: "Mina·연구본부·本部長", role: "worker", exited: false }] };
      throw new Error(cmd);
    },
    renderUsage() {}, setRoleDot() {}, displayPaneHeading() {},
    coalesceHqPanels: () => false, pruneEmptyHqPanels: () => false,
    renderHeader() {}, render: () => { renders++; }, setFocus() {}, updateFtRoot() {},
  };
  runInNewContext(new Bun.Transpiler({ loader: "ts" }).transformSync(`${helpers}\n${code}\n${poll}`), context);
  await context.refreshPaneTitles();
  expect(workspaces).toHaveLength(1);
  mtime = "1";
  await context.refreshPaneTitles();
  expect(workspaces).toHaveLength(2);
  expect(workspaces[0].tree).toBeNull();
  expect(context.collectSids(workspaces[1].tree)).toEqual([1]);
  expect(workspaces[1].name).toBe("Mina·연구본부");
  expect(context.panes.size).toBe(1);
  expect(renders).toBe(1);
  await context.refreshPaneTitles();
  expect(reads).toBe(1);
  expect(renders).toBe(1);
});

test("valid JSON with malformed roles keeps the last good grouping", async () => {
  const h = setup();
  await h.poll();
  for (const [i, raw] of ['null', '[]', '{}', '{"roles":null}', '{"roles":[]}', '{"roles":"bad"}'].entries()) {
    h.set(String(i + 2), raw);
    expect(await h.poll()).toBe(false);
    expect(adoptHqOf("Mina·작업·s21")).toBe("Mina");
  }
  h.set("empty-map", '{"roles":{}}');
  expect(await h.poll()).toBe(true);
  expect(adoptHqOf("Mina·작업·s21")).toBeNull();
});
