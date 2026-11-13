import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
function extract(signature: string) {
  const start = source.indexOf(signature);
  return start < 0 ? "" : source.slice(start, source.indexOf("\n}\n", start) + 2);
}
const code = new Bun.Transpiler({ loader: "ts" }).transformSync([
  extract("async function promptBinaryPatch() {"),
  extract("function binaryUpdateModal("),
  extract("function confirmModal("),
].join("\n"));

type Seat = { surface_id: number; exited: boolean; agent?: string | null };
const fleet = (...surfaces: Seat[][]) => ({ departments: surfaces.map((seats, i) => ({ name: `dept-${i}`, surfaces: seats })) });
const live = { surface_id: 1, exited: false, agent: "codex" };
const shell = { surface_id: 2, exited: false, agent: null };
const flush = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };

// The actual prompt and modal run against a small DOM; only Tauri IPC is substituted.
async function runPrompt(reply: unknown) {
  const dialogs: any[] = [], updates: unknown[] = [], errors: unknown[] = [], commands: string[] = [];
  let focused: any;
  const document = {
    createElement() {
      const listeners: Record<string, Function> = {};
      const nodes: Record<string, any> = {};
      for (const key of ["h3", "p", ".modal-yes", ".modal-no", ".modal"]) {
        nodes[key] = { textContent: "", hidden: false, attrs: {} as Record<string, string>,
          addEventListener(event: string, fn: Function) { this[event] = fn; },
          setAttribute(name: string, value: string) { this.attrs[name] = value; },
          focus() { focused = this; }, click() {} };
      }
      return { className: "", innerHTML: "", nodes, removed: false,
        querySelector: (key: string) => nodes[key],
        addEventListener(event: string, fn: Function) { listeners[event] = fn; },
        dispatch(event: string, e: any) { listeners[event]?.(e); },
        remove() { this.removed = true; } };
    },
    body: { appendChild(overlay: any) { dialogs.push(overlay); } },
  };
  const prompt = new Function("document", "updateAvailable", "invoke", "checkForUpdate", "dismissToast", "toast",
    `${code}; return promptBinaryPatch();`);
  const pending = prompt(document, { version: "0.3.5" }, async (command: string, args: unknown) => {
    commands.push(command);
    if (command === "org_fleet" || command === "org_fleet_for_update") {
      if (reply instanceof Error) throw reply;
      return reply;
    }
    if (command === "install_update") { updates.push(args); return; }
    throw new Error(`unexpected command: ${command}`);
  }, async () => {}, () => {}, (...args: unknown[]) => errors.push(args));
  await flush();
  return { dialogs, updates, errors, commands, pending, focused: () => focused,
    async click(key: string) { dialogs.at(-1)?.nodes[key].click(); await flush(); } };
}

for (const [name, reply, count] of [
  ["live agent", fleet([live]), 1],
  ["plain shell", fleet([shell]), 1],
  ["all departments", fleet([live], [shell, { ...live, exited: true }]), 2],
] as const) {
  test(`${name}: count shown, Later focused, first step cannot install`, async () => {
    const r = await runPrompt(reply);
    expect(r.dialogs).toHaveLength(1);
    expect(r.dialogs[0].nodes.p.textContent).toContain(`열린 좌석: ${count}개`);
    expect(r.focused()).toBe(r.dialogs[0].nodes[".modal-no"]);
    expect(r.focused().textContent).toBe("나중에");
    await r.click(".modal-yes");
    expect(r.dialogs).toHaveLength(1);
    expect(r.updates).toEqual([]);
    expect(r.dialogs[0].nodes[".modal-yes"].textContent).toBe(`좌석 ${count}개를 닫고 지금 업데이트`);
    expect(r.focused()).toBe(r.dialogs[0].nodes[".modal-no"]);
    await r.click(".modal-yes"); await r.pending;
    expect(r.updates).toEqual([{ force: true }]);
    expect(r.commands[0]).toBe("org_fleet_for_update");
  });
}

for (const [name, reply] of [
  ["IPC failure", new Error("unavailable")],
  ["department error", { departments: [{ surfaces: [], error: "timeout" }] }],
  ["partial fleet with a live seat", { departments: [{ surfaces: [live] }, { surfaces: [], error: "timeout" }] }],
  ["missing departments", {}], ["missing surfaces", { departments: [{}] }],
  ["missing exited", { departments: [{ surfaces: [{ surface_id: 1 }] }] }],
  ["empty inventory", { departments: [] }],
] as const) {
  test(`${name}: unknown has the same usable two-step dialog`, async () => {
    const r = await runPrompt(reply);
    expect(r.dialogs).toHaveLength(1);
    expect(r.dialogs[0].nodes.p.textContent).toContain("열린 좌석: 모름");
    expect(r.errors).toEqual([]);
    await r.click(".modal-yes");
    expect(r.updates).toEqual([]);
    expect(r.dialogs[0].nodes[".modal-yes"].textContent).toBe("열린 좌석을 닫고 지금 업데이트");
    await r.click(".modal-yes"); await r.pending;
    expect(r.updates).toEqual([{ force: true }]);
  });
}

test("Later at either step leaves the fleet unchanged and never installs", async () => {
  for (const reply of [fleet([live]), new Error("offline")]) {
    for (const secondStep of [false, true]) {
      const before = JSON.stringify(reply);
      const r = await runPrompt(reply);
      if (secondStep) await r.click(".modal-yes");
      await r.click(".modal-no"); await r.pending;
      expect(r.updates).toEqual([]);
      expect(JSON.stringify(reply)).toBe(before);
    }
  }
});

test("Escape cancels without installation", async () => {
  const r = await runPrompt(fleet([live]));
  r.dialogs[0].dispatch("keydown", { key: "Escape", preventDefault() {}, stopPropagation() {} });
  await flush();
  expect(r.dialogs[0].removed).toBe(true);
  expect(r.updates).toEqual([]);
});

test("verified zero seats uses honest fresh-seat text and an explicit install click", async () => {
  for (const reply of [fleet([]), fleet([{ ...live, exited: true }])]) {
    const r = await runPrompt(reply);
    expect(r.dialogs).toHaveLength(1);
    const body = r.dialogs[0].nodes.p.textContent;
    expect(body).toContain("좌석은 새로 열립니다");
    expect(body).not.toContain("자동 복원");
    expect(body).toContain("https://waveainetworks.com/get");
    expect(r.updates).toEqual([]);
    await r.click(".modal-yes"); await r.pending;
    expect(r.updates).toEqual([{ force: true }]);
  }
});

test("shared modal renders Later only when requested and keeps the generic No label", async () => {
  const start = source.indexOf("function confirmModal(");
  const end = source.indexOf("\n}\n", start) + 2;
  const code = new Bun.Transpiler({ loader: "ts" }).transformSync(source.slice(start, end));
  for (const cancel of [undefined, "나중에"]) {
    const nodes = Object.fromEntries(["h3", "p", ".modal-yes", ".modal-no"].map((key) => [key, {
      textContent: "", click: () => {}, addEventListener(_event: string, action: () => void) { this.click = action; },
    }]));
    let removed = false;
    const overlay = { innerHTML: "", className: "", querySelector: (key: string) => nodes[key],
      addEventListener() {}, remove() { removed = true; } };
    const document = { createElement: () => overlay, body: { appendChild() {} } };
    const modal = new Function("document", `${code}; return confirmModal;`)(document);
    const result = modal("title", "body", "지금 업데이트", cancel);
    expect(nodes[".modal-no"].textContent).toBe(cancel ?? "아니오");
    expect(nodes[".modal-yes"].textContent).toBe("지금 업데이트");
    nodes[".modal-no"].click();
    expect(await result).toBe(false);
    expect(removed).toBe(true);
  }
});
