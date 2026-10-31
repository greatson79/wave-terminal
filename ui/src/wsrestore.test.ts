import { expect, test } from "bun:test";
import { coalesceHqWorkspaces, normalizeWorkspaces, pruneEmptyHqWorkspaces, rehomeRestoredSeats, shouldKeepEmptyWorkspaceOnRestore } from "./wsrestore";
import { treeSids, type LayoutNode } from "./treededupe";
import { configureHierarchyNaming, paneRehomeTarget, sameHqPanelName } from "./hierarchy";

test("live or temporarily unavailable daemon keeps an empty panel", () => {
  expect(shouldKeepEmptyWorkspaceOnRestore(true)).toBe(true);
  expect(shouldKeepEmptyWorkspaceOnRestore(false)).toBe(true);
  expect(shouldKeepEmptyWorkspaceOnRestore(undefined)).toBe(false);
});

test("wave-spawn, new-surface and restart leave one headquarters panel with every seat", () => {
  configureHierarchyNaming(JSON.stringify({ roles: { master: { name: "Alpha", label: "운영본부·책임자" } } }));
  const workspaces = [
    { id: 1, name: "Alpha·운영본부", tree: { type: "pane" as const, sid: 1 } as LayoutNode | null },
    { id: 2, name: "Alpha", tree: { type: "pane" as const, sid: 2 } as LayoutNode | null },
    { id: 3, name: "Alpha·운영본부", tree: null as LayoutNode | null },
  ];
  rehomeRestoredSeats(workspaces, [
    { surface_id: 1, title: "Alpha·운영본부·본부장" },
    { surface_id: 2, title: "Alpha·작업·s2" },
  ], paneRehomeTarget, (name) => ({ id: 4, name, tree: null }), sameHqPanelName);
  const removed = coalesceHqWorkspaces(workspaces, sameHqPanelName);
  expect(removed.get(2)).toBe(1);
  expect(removed.get(3)).toBe(1);
  expect(workspaces).toHaveLength(1);
  expect(treeSids(workspaces[0].tree)).toEqual([1, 2]);
  expect(coalesceHqWorkspaces(workspaces, sameHqPanelName).size).toBe(0);
  configureHierarchyNaming(null);
});

test("coalescing keeps the occupied panel's user state and never crosses sockets", () => {
  configureHierarchyNaming(JSON.stringify({ roles: { master: { name: "Alpha", label: "운영본부·책임자" } } }));
  const workspaces = [
    { id: 1, name: "Alpha·운영본부", socket: "dept", tree: null as LayoutNode | null, collapsed: true, groupId: undefined as number | undefined },
    { id: 2, name: "Alpha", socket: "dept", tree: { type: "pane" as const, sid: 2 } as LayoutNode | null, collapsed: false, groupId: 9 as number | undefined },
    { id: 3, name: "Alpha", socket: "other", tree: null as LayoutNode | null, collapsed: false, groupId: undefined as number | undefined },
  ];
  expect(coalesceHqWorkspaces(workspaces, sameHqPanelName).get(1)).toBe(2);
  expect(workspaces).toHaveLength(2);
  expect(workspaces[0].groupId).toBe(9);
  expect(workspaces[0].collapsed).toBe(false);
  expect(workspaces[1].socket).toBe("other");
  configureHierarchyNaming(null);
});

test("confirmed default socket prunes a standalone empty HQ ghost but retains user and department tabs", () => {
  configureHierarchyNaming(JSON.stringify({ roles: { master: { name: "Alpha", label: "운영본부·책임자" } } }));
  const workspaces = [
    { id: 1, name: "Alpha·운영본부", tree: null as LayoutNode | null },
    { id: 2, name: "notes", tree: null as LayoutNode | null },
    { id: 3, name: "Alpha", socket: "dept", tree: null as LayoutNode | null },
  ];
  expect(pruneEmptyHqWorkspaces(workspaces, (name) => sameHqPanelName(name, name))).toEqual([1]);
  expect(workspaces.map((ws) => ws.id)).toEqual([2, 3]);
  configureHierarchyNaming(null);
});

test("restart retains every panel on one socket and repairs duplicate ids without duplicating seats", () => {
  const pane = (sid: number) => ({ type: "pane" as const, sid });
  const saved = [
    { id: 1, name: "Alpha", socket: "socket-a", tree: pane(11) },
    { id: 2, name: "Beta", socket: "socket-a", tree: pane(22) },
    { id: 2, name: "Alpha", socket: "socket-a", tree: pane(33) },
    { id: 4, name: "Alpha", socket: "socket-b", tree: pane(11) },
    { id: 5, name: "Beta", socket: "socket-a", tree: pane(22) },
  ];
  const restored = normalizeWorkspaces(saved);
  expect(restored.map((w) => [w.name, w.socket])).toEqual([
    ["Alpha", "socket-a"], ["Beta", "socket-a"], ["Alpha", "socket-a"],
    ["Alpha", "socket-b"], ["Beta", "socket-a"],
  ]);
  expect(new Set(restored.map((w) => w.id)).size).toBe(5);
  expect(treeSids(restored[0].tree)).toEqual([11]);
  expect(treeSids(restored[1].tree)).toEqual([22]);
  expect(treeSids(restored[2].tree)).toEqual([33]);
  expect(treeSids(restored[3].tree)).toEqual([11]);
  expect(treeSids(restored[4].tree)).toEqual([]);
});

test("first render splits a saved 13-seat four-row grid by local brands without losing seats", () => {
  const brands = ["Alpha", "Beta", "Gamma"];
  configureHierarchyNaming(JSON.stringify({ roles: Object.fromEntries(brands.map((name, i) =>
    [i ? `worker${i}` : "master", { name, label: `${name}본부·담당` }])) }));
  const pane = (sid: number) => ({ type: "pane" as const, sid });
  const row = (ids: number[]): LayoutNode => ids.slice(1).reduce<LayoutNode>((tree, sid) =>
    ({ type: "split" as const, dir: "row" as const, a: tree, b: pane(sid) }), pane(ids[0]));
  const rows = [[1, 2, 3, 4], [5, 6, 7], [8, 9, 10], [11, 12, 13]];
  const grid = rows.slice(1).reduce<LayoutNode>((tree, ids) =>
    ({ type: "split" as const, dir: "col" as const, a: tree, b: row(ids) }), row(rows[0]));
  const workspaces: { id: number; name: string; socket?: string; tree: LayoutNode | null }[] = [
    { id: 1, name: "non title", tree: grid },
    { id: 2, name: "Beta", tree: null },
    { id: 3, name: "other socket", socket: "dept", tree: pane(1) },
  ];
  const live = Array.from({ length: 13 }, (_, i) => ({ surface_id: i + 1,
    title: `${brands[i % 3]}·작업·s${i + 1}` }));
  let nextId = 4;
  rehomeRestoredSeats(workspaces, live, paneRehomeTarget,
    (name, socket) => ({ id: nextId++, name, socket, tree: null }));
  expect(workspaces.find((ws) => ws.id === 1)?.tree).toBeNull();
  expect(workspaces.find((ws) => ws.id === 2)?.tree && treeSids(workspaces[1].tree)).toEqual([2, 5, 8, 11]);
  expect(workspaces.find((ws) => ws.name === "Alpha")?.tree && treeSids(workspaces.find((ws) => ws.name === "Alpha")!.tree)).toEqual([1, 4, 7, 10, 13]);
  expect(workspaces.find((ws) => ws.name === "Gamma")?.tree && treeSids(workspaces.find((ws) => ws.name === "Gamma")!.tree)).toEqual([3, 6, 9, 12]);
  expect(workspaces.find((ws) => ws.socket === "dept")?.tree && treeSids(workspaces[2].tree)).toEqual([1]);
  expect(workspaces.filter((ws) => !ws.socket).flatMap((ws) => treeSids(ws.tree)).sort((a, b) => a - b))
    .toEqual(Array.from({ length: 13 }, (_, i) => i + 1));
  configureHierarchyNaming(null);
});

test("restore leaves seats without a live title in their saved panel", () => {
  const workspaces: { id: number; name: string; tree: LayoutNode | null }[] =
    [{ id: 1, name: "non title", tree: { type: "pane", sid: 9 } }];
  rehomeRestoredSeats(workspaces, [], () => "Alpha", () => ({ id: 2, name: "Alpha", tree: null }));
  expect(treeSids(workspaces[0].tree)).toEqual([9]);
  expect(workspaces).toHaveLength(1);
});

test("restore reuses an exact legacy target even when local HQ aliases do not match", () => {
  configureHierarchyNaming(null);
  const workspaces = [
    { id: 1, name: "misc", tree: { type: "pane" as const, sid: 9 } as LayoutNode | null },
    { id: 2, name: "legacy·운영본부", tree: null as LayoutNode | null },
  ];
  rehomeRestoredSeats(workspaces, [{ surface_id: 9, title: "legacy·운영본부·작업" }],
    () => "legacy·운영본부", () => { throw new Error("duplicate workspace"); }, sameHqPanelName);
  expect(workspaces).toHaveLength(2);
  expect(treeSids(workspaces[1].tree)).toEqual([9]);
});
