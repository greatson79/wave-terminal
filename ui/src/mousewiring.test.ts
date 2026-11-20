import { describe, expect, it, spyOn } from "bun:test";
import { readFileSync } from "node:fs";
import { routeOnData } from "./mousefilter";
import { inputSource } from "./inputsource";

const { Terminal } = require("@xterm/xterm");
const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const start = main.indexOf("  term.onData((data) => {");
const end = main.indexOf("  // ★F:", start);
if (start < 0 || end < 0) throw new Error("production input wiring boundary missing");
// Evaluate the production subscriptions without booting Tauri or copying their routing logic.
const wire = new Function("term", "routeOnData", "navigator", "applyIme", "source", "insertLeak", "sendRaw", main.slice(start, end));

async function seat(encoding: "DEFAULT" | "SGR", alternate = false, windows = true) {
  const term = new Terminal({ cols: 120, rows: 35, scrollback: 5000 });
  const write = (data: string) => new Promise<void>((resolve) => term.write(data, resolve));
  await write(Array.from({ length: 300 }, (_, n) => `row${n}\r\n`).join(""));
  if (alternate) await write("\x1b[?1049h");
  await write(`\x1b[?1000h\x1b[?1006${encoding === "SGR" ? "h" : "l"}`);
  term.scrollToBottom();
  // An unopened terminal has no DOM viewport; observe the real API call, not rendered movement.
  const scroll = spyOn(term, "scrollLines");
  const sent: string[] = [];
  const sentBinary: boolean[] = [];
  const data: string[] = [];
  const binary: string[] = [];
  term.onData((report: string) => data.push(report));
  term.onBinary((report: string) => binary.push(report));
  wire(term, routeOnData, { userAgent: windows ? "Windows" : "Macintosh" },
    (event: { data: string }) => sent.push(event.data), inputSource(term), null,
    (report: string, _human = true, binary = false) => { sent.push(report); sentBinary.push(binary); });
  // The real xterm mouse service encodes and fires onData/onBinary; no DOM or native app is needed.
  const wheel = (col = 10, row = 10) => term._core.coreMouseService.triggerMouseEvent({
    col, row, x: 100, y: 100, button: 4, action: 0, ctrl: false, alt: false, shift: false,
  });
  return { term, wheel, sent, sentBinary, data, binary, scroll };
}

describe("production xterm mouse subscriptions", () => {
  it("Windows legacy normal-buffer wheel requests local history scroll", async () => {
    const s = await seat("DEFAULT");
    try {
      const before = s.term.buffer.active.viewportY;
      expect(s.wheel()).toBe(true);
      expect(s.binary).toHaveLength(1);
      expect(s.data).toHaveLength(0);
      expect(s.scroll).toHaveBeenCalledWith(-3);
      expect(s.sent).toEqual([]);
    } finally { s.scroll.mockRestore(); s.term.dispose(); }
  });

  for (const encoding of ["DEFAULT", "SGR"] as const) {
    it(`Windows ${encoding} alt-buffer wheel reaches the app, not local scrollback`, async () => {
      const s = await seat(encoding, true);
      try {
        expect(s.term.buffer.active.type).toBe("alternate");
        expect(s.wheel()).toBe(true);
        const reports = encoding === "DEFAULT" ? s.binary : s.data;
        expect(reports).toHaveLength(1);
        expect(s.sent).toEqual(reports);
        expect(s.scroll).not.toHaveBeenCalled();
        expect(s.term.buffer.active.viewportY).toBe(0);
      } finally { s.scroll.mockRestore(); s.term.dispose(); }
    });
  }

  it("Windows SGR normal-buffer wheel keeps local scrolling", async () => {
    const s = await seat("SGR");
    try {
      const before = s.term.buffer.active.viewportY;
      s.wheel();
      expect(s.scroll).toHaveBeenCalledWith(-3);
      expect(s.sent).toEqual([]);
    } finally { s.scroll.mockRestore(); s.term.dispose(); }
  });

  for (const alternate of [false, true]) {
    it(`macOS SGR forwarding remains unchanged (alternate=${alternate})`, async () => {
      const s = await seat("SGR", alternate, false);
      try {
        const before = s.term.buffer.active.viewportY;
        s.wheel();
        expect(s.sent).toEqual(s.data);
        expect(s.term.buffer.active.viewportY).toBe(before);
      } finally { s.scroll.mockRestore(); s.term.dispose(); }
    });
  }

  it("macOS legacy subscription behavior remains unchanged", async () => {
    const s = await seat("DEFAULT", false, false);
    try {
      const before = s.term.buffer.active.viewportY;
      s.wheel();
      expect(s.binary).toHaveLength(1);
      expect(s.sent).toEqual([]);
      expect(s.term.buffer.active.viewportY).toBe(before);
    } finally { s.scroll.mockRestore(); s.term.dispose(); }
  });
});

it("high-column legacy reports request exact binary transport", async () => {
  const s = await seat("DEFAULT", true);
  try {
    for (const col of [95, 100, 119]) {
      expect(s.wheel(col, 10)).toBe(true);
      expect(s.sent.at(-1)).toBe(s.binary.at(-1));
      expect(s.sentBinary.at(-1)).toBe(true);
      expect(Array.from(s.binary.at(-1)!, c => c.charCodeAt(0))).toEqual([27, 91, 77, 96, col + 33, 43]);
    }
  } finally { s.scroll.mockRestore(); s.term.dispose(); }
});

it("production sendRaw preserves binary flag through IPC", async () => {
  const from = main.indexOf("  let sendChain: Promise<unknown> = Promise.resolve();");
  const to = main.indexOf("  // ── 붙여넣기", from);
  expect(from).toBeGreaterThan(0);
  expect(to).toBeGreaterThan(from);
  const code = new Bun.Transpiler({ loader: "ts" }).transformSync(main.slice(from, to));
  const calls: any[] = [];
  const sendRaw = new Function("invoke", "socket", "sid", `let follow = false; ${code}; return sendRaw;`)(
    (command: string, args: any) => { calls.push([command, args]); return Promise.resolve(); }, "test-socket", 7);
  const data = "\x1b[M`\x80+";
  await sendRaw(data, true, true);
  expect(calls).toEqual([["send_input", { socket: "test-socket", surfaceId: 7, data, human: true, binary: true }]]);
});
