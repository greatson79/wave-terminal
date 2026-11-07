import { expect, test } from "bun:test";
import { coalesceHqWorkspaces, normalizeWorkspaces, pruneEmptyHqWorkspaces, rehomeRestoredSeats, remapPrimarySurfaceIds, prepareRenameLayout, applyRenameLayoutSafely, shouldKeepEmptyWorkspaceOnRestore } from "./wsrestore";
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


test("rename remaps primary leaves while preserving panels, groups and split layout", () => {
  const tree: LayoutNode = { type: "split", dir: "col", ratio: 0.35,
    a: { type: "pane", sid: 1 },
    b: { type: "split", dir: "row", ratio: 0.7,
      a: { type: "pane", sid: 2 }, b: { type: "pane", sid: 9 } } };
  const saved = [
    { id: 4, name: "operations", tree, groupId: 3, collapsed: true, manual: true },
    { id: 5, name: "notes", tree: null, groupId: 3 },
    { id: 6, name: "department", socket: "dept", tree: { type: "pane" as const, sid: 1 } },
  ];
  const restored = remapPrimarySurfaceIds(saved, { "1": 11, "2": 12 });
  expect(restored[0]).toEqual({ ...saved[0], tree: { ...tree,
    a: { type: "pane", sid: 11 }, b: { ...tree.b, a: { type: "pane", sid: 12 } } } });
  expect(restored[1]).toBe(saved[1]);
  expect(restored[2]).toBe(saved[2]);
  expect(treeSids(saved[0].tree)).toEqual([1, 2, 9]);
  expect(treeSids(restored[0].tree)).toEqual([11, 12, 9]);
});

test("absent rename map leaves saved layout unchanged", () => {
  const saved = [{ id: 1, name: "panel", tree: { type: "pane" as const, sid: 1 } }];
  expect(remapPrimarySurfaceIds(saved)).toBe(saved);
  expect(remapPrimarySurfaceIds(saved, {})).toBe(saved);
});

test("rename IDs swap simultaneously and remain scoped to the primary socket", () => {
  const saved = [
    { id: 1, name: "one", tree: { type: "pane" as const, sid: 1 } },
    { id: 2, name: "two", tree: { type: "pane" as const, sid: 2 } },
    { id: 3, name: "dept", socket: "dept", tree: { type: "pane" as const, sid: 2 } },
  ];
  const restored = remapPrimarySurfaceIds(saved, { "1": 2, "2": 1 });
  expect(restored.map((ws) => treeSids(ws.tree))).toEqual([[2], [1], [2]]);
  expect(restored[2]).toBe(saved[2]);
});

test("rename collisions or invalid IDs reject atomically without changing saved trees", () => {
  const saved = [
    { id: 1, name: "one", tree: { type: "pane" as const, sid: 1 } },
    { id: 2, name: "two", tree: { type: "pane" as const, sid: 2 } },
  ];
  for (const map of [{ "1": 2 }, { "1": 3, "2": 3 }, { "1": -1 }, { "01": 3 }, { "1": NaN }]) {
    expect(() => remapPrimarySurfaceIds(saved, map)).toThrow();
    expect(saved.map((ws) => treeSids(ws.tree))).toEqual([[1], [2]]);
  }
});


test("replayable swap survives failure after persistence without remapping twice", () => {
  const saved = {
    workspaces: [
      { id: 1, name: "one", tree: { type: "pane" as const, sid: 1 } },
      { id: 2, name: "two", tree: { type: "pane" as const, sid: 2 } },
    ],
    groups: [{ id: 7, name: "saved group" }], active: 1, counter: 9, custom: { preserved: true },
  };
  const handover = { revision: "rename-1", map: { "1": 2, "2": 1 } };
  const converted = prepareRenameLayout(saved, handover);
  const persisted = JSON.stringify(converted); // Crash before ack: backend still replays this revision.
  const restarted = JSON.parse(persisted) as typeof converted;
  expect(prepareRenameLayout(restarted, handover)).toBe(restarted);
  expect(restarted.workspaces.map((ws) => treeSids(ws.tree))).toEqual([[2], [1]]);
  expect(restarted.renameRevision).toBe("rename-1");
  expect(restarted.groups).toEqual(saved.groups);
  expect(restarted.active).toBe(1);
  expect(restarted.custom).toEqual({ preserved: true });
  // Normal subsequent saves carry the revision even after backend acknowledgement.
  const savedAgain = { ...restarted, active: 0 };
  expect(prepareRenameLayout(savedAgain, handover)).toBe(savedAgain);
});

test("failed layout persistence leaves old storage replayable after remapping in memory", () => {
  const saved = { workspaces: [{ id: 1, name: "panel", tree: { type: "pane" as const, sid: 1 } }] };
  const oldStorage = JSON.stringify(saved);
  const handover = { revision: "rename-2", map: { "1": 11 } };
  const inMemory = prepareRenameLayout(saved, handover);
  const write = (_value: string) => { throw new Error("storage full"); };
  expect(() => write(JSON.stringify(inMemory))).toThrow("storage full");
  expect(JSON.stringify(saved)).toBe(oldStorage);
  const restarted = prepareRenameLayout(JSON.parse(oldStorage) as typeof saved, handover);
  expect(treeSids(restarted.workspaces[0].tree)).toEqual([11]);
  expect(restarted.renameRevision).toBe("rename-2");
});

test("no pending revision preserves the saved document", () => {
  const saved = { workspaces: [], renameRevision: "earlier" };
  expect(prepareRenameLayout(saved, null)).toBe(saved);
  expect(prepareRenameLayout(saved, { revision: "", map: {} })).toBe(saved);
  expect(() => prepareRenameLayout(saved, { revision: "", map: { "1": 2 } })).toThrow();
});

const fallbackSaved = () => ({
  workspaces: [
    { id: 1, name: "ops", tree: { type: "pane" as const, sid: 1 }, groupId: 7 },
    { id: 2, name: "dept", socket: "dept", tree: { type: "pane" as const, sid: 1 } },
  ],
  groups: [{ id: 7, name: "team" }], active: 1, custom: { kept: true },
});

test("corrupt or colliding handover keeps the saved layout, drops the ack and explains in Korean", () => {
  const saved = fallbackSaved();
  saved.workspaces[0].tree = { type: "split", dir: "row", a: { type: "pane", sid: 1 }, b: { type: "pane", sid: 5 } } as never;
  const before = JSON.stringify(saved);
  for (const bad of [{ revision: "r1", map: [] }, { revision: 3, map: {} }, { revision: "r1", map: { "1": 5 } },
    { revision: "", map: { "1": 2 } }, { revision: "r1", map: { "x": 2 } }]) {
    const result = applyRenameLayoutSafely(saved, bad as never);
    expect(result.layout).toBe(saved);
    expect(result.handover).toBeNull();
    expect(result.notice).toContain("저장된 레이아웃을 유지");
    expect(result.backup).toBe(before);
    expect(JSON.stringify(saved)).toBe(before);
  }
});

test("valid handover backs up the exact old layout and keeps groups, departments and extras", () => {
  const saved = fallbackSaved();
  const before = JSON.stringify(saved);
  const handover = { revision: "r2", map: { "1": 11 } };
  const result = applyRenameLayoutSafely(saved, handover);
  expect(result.notice).toBeUndefined();
  expect(result.handover).toBe(handover);
  expect(result.backup).toBe(before);
  expect(treeSids(result.layout.workspaces[0].tree)).toEqual([11]);
  expect(result.layout.workspaces[1]).toBe(saved.workspaces[1]); // department socket untouched
  expect(result.layout.groups).toBe(saved.groups);
  expect(result.layout.custom).toEqual({ kept: true });
});

test("no handover or an already applied revision changes nothing and writes no backup", () => {
  const saved = fallbackSaved();
  expect(applyRenameLayoutSafely(saved, null)).toEqual({ layout: saved, handover: null });
  const applied = { ...saved, renameRevision: "r3" };
  const result = applyRenameLayoutSafely(applied, { revision: "r3", map: { "1": 11 } });
  expect(result.layout).toBe(applied);
  expect(result.backup).toBeUndefined();
  expect(result.notice).toBeUndefined();
});

// Carry 1 R2 (Zen carry1-r1 blocking): a fallback handover has no old→new map, so every saved primary SID belongs
// to the old daemon generation. None may attach to a fresh seat that only shares the integer.
test("fallback handover: old primary SID 3 never attaches to new seat 3 from another origin", () => {
  const saved = {
    workspaces: [
      { id: 1, name: "ops", groupId: 7, tree: { type: "split" as const, dir: "row" as const,
        a: { type: "pane" as const, sid: 3 }, b: { type: "pane" as const, sid: 4 } } },
      { id: 2, name: "dept", socket: "dept", tree: { type: "pane" as const, sid: 3 } },
    ],
    groups: [{ id: 7, name: "team" }], active: 1, custom: { kept: true },
  };
  const before = JSON.stringify(saved);
  const handover = { revision: "fb-1", map: {}, fallback: true };
  const result = applyRenameLayoutSafely(saved, handover);
  expect(result.handover).toBe(handover); // still acknowledged, so the backend can run the existing restore
  expect(result.backup).toBe(before);     // the exact old layout stays recoverable
  const primary = result.layout.workspaces[0];
  expect(primary.tree).toBeNull();        // detached: no old integer left to match
  expect(primary.groupId).toBe(7);        // panel and group stay
  expect(result.layout.workspaces[1]).toBe(saved.workspaces[1]); // department socket is a different origin, untouched
  expect(result.layout.groups).toBe(saved.groups);
  expect(result.layout.custom).toEqual({ kept: true });
  expect(result.layout.renameRevision).toBe("fb-1");
  // The fresh daemon's seat 3 (a different origin) is not owned by any primary pane after the fallback.
  const owners = result.layout.workspaces.filter((ws) => ws.socket === undefined && treeSids(ws.tree).includes(3));
  expect(owners).toEqual([]);
  // Replay of the same revision after a crash changes nothing more.
  expect(prepareRenameLayout(result.layout, handover)).toBe(result.layout);
});

test("fallback handover with a map or a non-boolean flag is refused and keeps the saved layout", () => {
  const saved = { workspaces: [{ id: 1, name: "ops", tree: { type: "pane" as const, sid: 3 } }] };
  for (const bad of [{ revision: "fb-2", map: { "3": 9 }, fallback: true }, { revision: "fb-3", map: {}, fallback: "yes" }]) {
    const result = applyRenameLayoutSafely(saved, bad as never);
    expect(result.layout).toBe(saved);
    expect(result.handover).toBeNull();
    expect(result.notice).toContain("저장된 레이아웃을 유지");
  }
});

test("a normal handover without the fallback flag keeps today's remap", () => {
  const saved = { workspaces: [{ id: 1, name: "ops", tree: { type: "pane" as const, sid: 3 } }] };
  const result = applyRenameLayoutSafely(saved, { revision: "n-1", map: { "3": 30 }, fallback: false });
  expect(treeSids(result.layout.workspaces[0].tree)).toEqual([30]);
});
