import { expect, test } from "bun:test";
import { shouldAdoptSurface } from "./surfaceadopt";

test("named live surface is adoptable without a role", () => {
  expect(shouldAdoptSurface({ exited: false, role: null, title: "work" })).toBe(true);
  expect(shouldAdoptSurface({ exited: true, role: "worker", title: "work" })).toBe(false);
});
