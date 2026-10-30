import { expect, test } from "bun:test";
import { normalizeWorkspaces, rehomeRestoredSeats, shouldKeepEmptyWorkspaceOnRestore } from "./wsrestore";
import { treeSids, type LayoutNode } from "./treededupe";
import { configureHierarchyNaming, paneRehomeTarget } from "./hierarchy";

test("live or temporarily unavailable daemon keeps an empty panel", () => {
  expect(shouldKeepEmptyWorkspaceOnRestore(true)).toBe(true);
  expect(shouldKeepEmptyWorkspaceOnRestore(false)).toBe(true);
  expect(shouldKeepEmptyWorkspaceOnRestore(undefined)).toBe(false);
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
