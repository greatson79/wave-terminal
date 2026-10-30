import { expect, test } from "bun:test";
import { configureHierarchyNaming } from "./hierarchy";
import { createHqTabShell, type MiniDoc, type MiniEl } from "./wstab";

test("tab shell paints the local badge and border", () => {
  configureHierarchyNaming(JSON.stringify({ roles: { master: { name: "Alpha", label: "운영본부·책임자" } } }));
  const make = (): MiniEl & { children: MiniEl[]; attrs: Record<string, string> } => ({
    className: "", title: "", textContent: "", style: {}, children: [], attrs: {},
    setAttribute(name, value) { this.attrs[name] = value; },
    appendChild(child) { this.children.push(child); },
  });
  const doc: MiniDoc = { createElement: () => make() };
  const { tab, titleRow } = createHqTabShell("Alpha", { 운영본부: "#123456" }, [], "", doc);
  expect(tab.style.borderLeftColor).toBe("#123456");
  expect((titleRow as ReturnType<typeof make>).children[0].textContent).toBe("운");
  configureHierarchyNaming(null);
});
