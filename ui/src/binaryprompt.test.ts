import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const start = source.indexOf("async function promptBinaryPatch() {");
const end = source.indexOf("\n}\n", start) + 2;
// Bun's documented transpiler keeps this test on the actual UI path, including its TypeScript types.
const promptCode = new Bun.Transpiler({ loader: "ts" }).transformSync(source.slice(start, end));
const prompt = new Function("updateAvailable", "confirmModal", "invoke", "checkForUpdate", "dismissToast", "toast", `${promptCode}; return promptBinaryPatch();`);
const warning = "업데이트하면 열린 좌석이 닫힙니다. 작업 파일은 그대로 남고, 업데이트 뒤 마스터에게 다시 열어 달라고 하면 됩니다.";

type Seat = { surface_id: number; exited: boolean; agent?: string | null };
async function runPrompt(departments: { name: string; surfaces: Seat[] }[], proceed: boolean) {
  const dialogs: unknown[][] = [];
  const updates: unknown[] = [];
  const state = { departments, settings: { theme: "dark" } };
  const before = JSON.stringify(state);
  await prompt({ version: "0.3.4" }, async (...args: unknown[]) => { dialogs.push(args); return proceed; },
    async (command: string, args: unknown) => {
      if (command === "org_fleet") return { departments };
      if (command === "install_update") { updates.push(args); state.departments = []; return; }
      throw new Error(`unexpected command: ${command}`);
    }, async () => {}, () => {}, () => {});
  return { dialogs, updates, state, before };
}

test("live plain shell shows one exact Korean warning with Update/Later", async () => {
  const result = await runPrompt([{ name: "_hq", surfaces: [{ surface_id: 1, exited: false, agent: null }] }], false);
  expect(result.dialogs).toHaveLength(1);
  expect(result.dialogs[0].slice(1)).toEqual([warning, "지금 업데이트", "나중에"]);
});

test("live seat in a registered department also warns", async () => {
  const result = await runPrompt([{ name: "_hq", surfaces: [] }, { name: "dept", surfaces: [{ surface_id: 1, exited: false }] }], false);
  expect(result.dialogs).toHaveLength(1);
  expect(result.dialogs[0][1]).toBe(warning);
});

test("zero live seats keeps today's generic confirmation, including manual-install URL", async () => {
  for (const surfaces of [[], [{ surface_id: 1, exited: true }]]) {
    const result = await runPrompt([{ name: "_hq", surfaces }], false);
    expect(result.dialogs).toHaveLength(1);
    expect(result.dialogs[0]).toEqual([
      "새 본체 버전 0.3.4 — 패치 설치",
      "새 본체(앱) 0.3.4을 패치 방식으로 설치합니다: 저장(drain) 신호 후 다운로드·서명 검증·교체하고 앱을 " +
        "재시작합니다. 부서·노드는 재시작 후 자동 복원됩니다(대화 기억 포함). 마지막 미저장분은 손실될 수 " +
        "있습니다.\n\n지금 설치하시겠습니까? (수동 설치는 다운로드 페이지 https://waveainetworks.com/get)",
      "설치",
    ]);
    expect(result.dialogs[0][1]).not.toContain(warning);
    expect(result.dialogs[0][2]).toBe("설치");
    expect(result.updates).toEqual([]);
  }
});

test("Later leaves seats/settings unchanged and never starts an update", async () => {
  const result = await runPrompt([{ name: "_hq", surfaces: [{ surface_id: 1, exited: false }] }], false);
  expect(result.dialogs[0][3]).toBe("나중에");
  expect(result.updates).toEqual([]);
  expect(JSON.stringify(result.state)).toBe(result.before);
});

test("Update now uses today's exact install_update force=true call", async () => {
  const result = await runPrompt([{ name: "_hq", surfaces: [{ surface_id: 1, exited: false }] }], true);
  expect(result.dialogs[0].slice(1)).toEqual([warning, "지금 업데이트", "나중에"]);
  expect(result.updates).toEqual([{ force: true }]);
});

test("zero seats confirmed still uses today's exact update call", async () => {
  const result = await runPrompt([{ name: "_hq", surfaces: [] }], true);
  expect(result.dialogs[0][2]).toBe("설치");
  expect(result.updates).toEqual([{ force: true }]);
});

test("failed seat lookup cannot silently use the zero-seat confirmation or install", async () => {
  const dialogs: unknown[] = [];
  const updates: unknown[] = [];
  const errors: unknown[] = [];
  await prompt({ version: "0.3.4" }, async (...args: unknown[]) => { dialogs.push(args); return true; },
    async (command: string, args: unknown) => {
      if (command === "org_fleet") throw new Error("unavailable");
      updates.push(args);
    }, async () => {}, () => {}, (...args: unknown[]) => errors.push(args));
  expect(dialogs).toEqual([]);
  expect(updates).toEqual([]);
  expect(errors).toHaveLength(1);
});

test("unreadable fleet entry is not evidence of zero live seats", async () => {
  const dialogs: unknown[] = [];
  const updates: unknown[] = [];
  await prompt({ version: "0.3.4" }, async (...args: unknown[]) => { dialogs.push(args); return true; },
    async (command: string, args: unknown) => {
      if (command === "org_fleet") return { departments: [{ name: "_hq", surfaces: [], error: "timeout" }] };
      updates.push(args);
    }, async () => {}, () => {}, () => {});
  expect(dialogs).toEqual([]);
  expect(updates).toEqual([]);
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
