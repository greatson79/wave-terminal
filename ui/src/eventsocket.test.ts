import { expect, test } from "bun:test";
import { resolveEventSocket } from "./eventsocket";

test("default and department event slugs preserve socket identity", () => {
  const sockets = new Map<string, string | undefined>([["default", undefined], ["dept", "sock-a"]]);
  expect(resolveEventSocket("default", sockets)).toEqual({ known: true, socket: undefined });
  expect(resolveEventSocket("dept", sockets)).toEqual({ known: true, socket: "sock-a" });
  expect(resolveEventSocket("other", sockets).known).toBe(false);
});
