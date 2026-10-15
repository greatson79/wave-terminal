import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

test("hidden update badge overrides its display rule", () => {
  const css = readFileSync(new URL("./style.css", import.meta.url), "utf8");
  expect(css).toMatch(/\.badge\[hidden\]\s*\{\s*display:\s*none\s*;/);
});
