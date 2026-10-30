import { expect, test } from "bun:test";
import { configureHierarchyNaming } from "./hierarchy";
import { teamMemberRows } from "./teamtree";

test("member label comes from title, with no model or address", () => {
  configureHierarchyNaming(JSON.stringify({ roles: { master: { name: "Alpha", label: "운영본부·책임자" } } }));
  expect(teamMemberRows([{ sid: 4, title: "Alpha·운영본부·민수 작성자·s4·Opus5" }])).toEqual([{ sid: 4, label: "민수, 작성자" }]);
  configureHierarchyNaming(null);
});
