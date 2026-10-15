import { test, expect } from "bun:test";
import { inputSource } from "./inputsource";

function terminal() {
  let user: () => void = () => {};
  const term = { _core: { coreService: { onUserInput: (listener: () => void) => { user = listener; } } } };
  const source = inputSource(term);
  return { source, emit: (wasUserInput: boolean) => { if (wasUserInput) user(); return source.takeHuman(); } };
}

test("G1 automatic terminal replies do not mark human", () => {
  const { emit } = terminal();
  for (const data of ["\x1b[1;2R", "\x1b[?1;2R", "\x1b[0n", "\x1b[?1;2c", "\x1b[?6c",
    "\x1b[>0;276;0c", "\x1b[?1;2$y", "\x1b[8;24;80t", "\x1b[4;500;800t", "\x1b[6;10;20t",
    "\x1b[I", "\x1b[O", "\x1b]10;rgb:ffff/ffff/ffff\x1b\\", "\x1b]11;rgb:0000/0000/0000\x1b\\",
    "\x1b]12;rgb:0000/0000/0000\x1b\\", "\x1b]4;1;rgb:ffff/0000/0000\x1b\\", "\x1bP1$r0m\x1b\\"]) {
    expect(emit(false), data).toBe(false);
  }
});

test("G1 real xterm query replies do not mark human", async () => {
  const { Terminal } = await import("../node_modules/@xterm/xterm/lib/xterm.js");
  const term = new Terminal({ allowProposedApi: true });
  const source = inputSource(term);
  const replies: Array<[string, boolean]> = [];
  term.onData((data: string) => replies.push([data, source.takeHuman()]));
  await new Promise<void>((resolve) => term.write("\x1b[6n\x1b[c\x1b[>c\x1b[?6n\x1b[5n\x1b[?2004$p", resolve));
  expect(replies.map(([data]) => data)).toEqual([
    "\x1b[1;1R", "\x1b[?1;2c", "\x1b[>0;276;0c", "\x1b[?1;1R", "\x1b[0n", "\x1b[?2004;2$y",
  ]);
  expect(replies.every(([, human]) => !human)).toBe(true);
  term.dispose();
});

test("G2 real input and pasted reply bytes still mark human", () => {
  const { source, emit } = terminal();
  for (const data of ["x", "\x1b[1;2R", "paste", "\x1b[I", "\x1b[O",
    "\x1b]10;rgb:ffff/ffff/ffff\x1b\\", "\x1b[?5;1R", "\x1b[4;1;1t", "한", "\x1b[<0;1;1M"]) {
    expect(emit(true), data).toBe(true);
    expect(source.takeHuman(), "mark is consumed by one onData").toBe(false);
  }
});

test("G2 real xterm user input remains human", async () => {
  const { Terminal } = await import("../node_modules/@xterm/xterm/lib/xterm.js");
  const term = new Terminal({ allowProposedApi: true });
  const source = inputSource(term);
  const inputs: Array<[string, boolean]> = [];
  term.onData((data: string) => inputs.push([data, source.takeHuman()]));
  term.input("x", true);
  term.input("\x1b[1;2R", true);
  term.input("a", false);
  expect(inputs).toEqual([["x", true], ["\x1b[1;2R", true], ["a", false]]);
  term.dispose();
});

test("G3 missing internal API treats every onData as human and warns once", () => {
  const warnings: string[] = [];
  const source = inputSource({}, (message) => warnings.push(message));
  expect(source.takeHuman()).toBe(true);
  expect(source.takeHuman()).toBe(true);
  expect(warnings).toHaveLength(1);
});
