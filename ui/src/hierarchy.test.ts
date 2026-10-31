import { expect, test } from "bun:test";
import { adoptHqOf, autoAssignGroups, configureHierarchyNaming, hqDisplayName, isHqBrand, paneRehomeTarget, pickAdoptWs, sameHqPanelName, sortHqWorkspaces } from "./hierarchy";

const naming = JSON.stringify({ roles: {
  master: { name: "Alpha", label: "운영본부·책임자" },
  worker: { name: "Beta", label: "연구본부·실무자" },
} });

test("local naming roles determine panels, rehome, group and order", () => {
  expect(configureHierarchyNaming(naming)).toBe(true);
  expect(isHqBrand("Alpha")).toBe(true);
  expect(isHqBrand("Unknown")).toBe(false);
  expect(hqDisplayName("Alpha")).toBe("Alpha·운영본부");
  expect(adoptHqOf("Alpha·작업·s21")).toBe("Alpha");
  expect(paneRehomeTarget("Beta", "Alpha·작업·s21")).toBe("Alpha");
  const workspaces = [{ name: "Beta" }, { name: "Alpha" }];
  expect(pickAdoptWs(workspaces, workspaces[1])).toBe(workspaces[1]);
  expect(sortHqWorkspaces(workspaces).map((w) => w.name)).toEqual(["Alpha", "Beta"]);
  const members = [{ name: "운영본부·1팀" }];
  const assigned = autoAssignGroups(members, [], 1);
  expect(assigned.groups[0].name).toBe("운영본부");
  expect(members[0].groupId).toBe(1);
  expect(configureHierarchyNaming(null)).toBe(false);
  expect(isHqBrand("Alpha")).toBe(false);
});

test("addressed and addressless titles share one local headquarters panel", () => {
  configureHierarchyNaming(naming);
  expect(adoptHqOf("Alpha·작업·s21")).toBe("Alpha");
  expect(adoptHqOf("Alpha·운영본부·작업")).toBe("Alpha·운영본부");
  expect(sameHqPanelName("Alpha", "Alpha·운영본부")).toBe(true);
  expect(sameHqPanelName("Beta", "Alpha·운영본부")).toBe(false);
  expect(paneRehomeTarget("Alpha·운영본부", "Alpha·작업·s21")).toBeNull();
  configureHierarchyNaming(null);
});
