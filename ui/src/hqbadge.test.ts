import { expect, test } from "bun:test";
import { configureHierarchyNaming } from "./hierarchy";
import { hqPresentation, parseNamingColors } from "./hqbadge";

test("badge color comes only from validated local settings", () => {
  configureHierarchyNaming(JSON.stringify({ roles: { master: { name: "Alpha", label: "운영본부·책임자" } } }));
  const { colors, invalid } = parseNamingColors(JSON.stringify({ colors: { 운영본부: "#123456", Bad: "url(x)" } }));
  expect(invalid).toEqual(["Bad"]);
  expect(hqPresentation("Alpha", colors, [], "#ffffff")).toEqual({ hq: "운영본부", color: "#123456", initial: "운" });
  configureHierarchyNaming(null);
});
