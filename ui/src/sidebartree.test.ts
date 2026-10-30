import { expect, test } from "bun:test";
import { configureHierarchyNaming } from "./hierarchy";
import { sidebarTreeSpec, hasOrphanTopLevelWorker } from "./sidebartree";

test("known panel has one header with a nested member, and collapse hides members", () => {
  configureHierarchyNaming(JSON.stringify({ roles: { master: { name: "Alpha", label: "운영본부·책임자" } } }));
  const open = { name: "Alpha", sids: [4] };
  expect(sidebarTreeSpec(open)).toMatchObject({ headerLabel: "Alpha·운영본부", count: 1, chevron: "▾", memberSids: [4] });
  expect(hasOrphanTopLevelWorker(open)).toBe(false);
  expect(sidebarTreeSpec({ ...open, collapsed: true }).memberSids).toEqual([]);
  configureHierarchyNaming(null);
});
