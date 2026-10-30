import { expect, test } from "bun:test";
import { normalizeWorkspaces, shouldKeepEmptyWorkspaceOnRestore } from "./wsrestore";
import { treeSids } from "./treededupe";

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
