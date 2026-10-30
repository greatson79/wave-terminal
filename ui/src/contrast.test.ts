import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { wcagContrast } from "./contrast";

test("fixed accent and error surfaces use readable text colors", () => {
  const css = readFileSync(new URL("./style.css", import.meta.url), "utf8");
  const token = (name: string) => css.match(new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{6})`))?.[1] ?? "";
  expect(wcagContrast(token("accent"), token("on-accent"))).toBeGreaterThanOrEqual(4.5);
  expect(wcagContrast(token("error"), token("on-error"))).toBeGreaterThanOrEqual(4.5);
  expect(css).toContain(".ws-tab.active .ws-name, .ws-tab.active .ws-sub,");
  expect(css).toContain("#ctx-menu .ctx-item:hover { background: var(--accent); color: var(--on-accent); }");
  expect(css).toContain(".modal-btns .modal-yes { background: var(--accent); border-color: var(--accent); color: var(--on-accent);");
});
