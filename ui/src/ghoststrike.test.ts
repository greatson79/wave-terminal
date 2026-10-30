import { expect, test } from "bun:test";
import { nextGhostStrike } from "./ghoststrike";

test("two nonempty successful misses remove a ghost, empty or failed lists defer", () => {
  expect(nextGhostStrike(0, { listOk: true, listNonEmpty: true, presentInList: false })).toEqual({ strikes: 1, remove: false });
  expect(nextGhostStrike(1, { listOk: true, listNonEmpty: true, presentInList: false })).toEqual({ strikes: 2, remove: true });
  expect(nextGhostStrike(1, { listOk: true, listNonEmpty: false, presentInList: false })).toEqual({ strikes: 1, remove: false });
  expect(nextGhostStrike(1, { listOk: false, listNonEmpty: true, presentInList: false })).toEqual({ strikes: 1, remove: false });
});
