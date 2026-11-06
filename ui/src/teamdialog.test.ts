import { afterEach, expect, test } from "bun:test";
import { openTeamDialog } from "./teamdialog";

class El {
  tagName: string;
  children: (El | string)[] = [];
  listeners = new Map<string, ((e: any) => unknown)[]>();
  attrs: Record<string, string> = {};
  value = "";
  textContent = "";
  disabled = false;
  removed = false;
  open = false;
  type = "";
  className = "";
  constructor(tag: string) { this.tagName = tag; }
  append(...children: (El | string)[]) { this.children.push(...children); }
  setAttribute(k: string, v: string) { this.attrs[k] = v; }
  addEventListener(k: string, f: (e: any) => unknown) {
    this.listeners.set(k, [...this.listeners.get(k) ?? [], f]);
  }
  async fire(k: string, props = {}) {
    const e = { preventDefault() {}, stopPropagation() {}, ...props };
    for (const f of this.listeners.get(k) ?? []) await f(e);
    await Promise.resolve();
  }
  focus() {}
  select() {}
  showModal() { this.open = true; }
  close() { this.open = false; }
  remove() { this.removed = true; }
  all(): El[] { return [this, ...this.children.flatMap(c => typeof c === "string" ? [] : c.all())]; }
  button(text: string) { return this.all().find(e => e.tagName === "button" && e.textContent === text)!; }
}
const oldDocument = globalThis.document;
afterEach(() => { globalThis.document = oldDocument; });
function setup(overrides: Record<string, unknown> = {}) {
  const body = new El("body");
  globalThis.document = { body, activeElement: null, createElement: (tag: string) => new El(tag) } as unknown as Document;
  const saves: unknown[] = [];
  const dialog = openTeamDialog({ suggestion: "기획팀", chooseFolder: async () => "/fixture/한글 폴더", save: async v => { saves.push(v); }, ...overrides }) as unknown as El;
  const inputs = dialog.all().filter(e => e.tagName === "input");
  return { dialog, name: inputs[0], folder: inputs[1], saves };
}

test("create dialog labels fields, selects a folder, saves once and closes", async () => {
  const { dialog, name, folder, saves } = setup();
  expect(dialog.open).toBe(true);
  expect(dialog.all().filter(e => e.tagName === "label").map(e => e.children[0])).toEqual(["팀 이름", "작업 폴더"]);
  expect(name.value).toBe("기획팀");
  await dialog.button("폴더 선택").fire("click");
  expect(folder.value).toBe("/fixture/한글 폴더");
  await dialog.fire("keydown", { key: "Enter", keyCode: 13, isComposing: false });
  expect(saves).toEqual([{ name: "기획팀", folder: "/fixture/한글 폴더" }]);
  expect(dialog.removed).toBe(true);
});

test("composition state, isComposing and keyCode 229 block Enter and modified Enter", async () => {
  const { dialog, folder, saves } = setup();
  folder.value = "/fixture";
  await dialog.fire("compositionstart");
  await dialog.fire("keydown", { key: "Enter", keyCode: 13, ctrlKey: true });
  await dialog.fire("submit");
  await dialog.fire("compositionend");
  await dialog.fire("keydown", { key: "Enter", keyCode: 13, isComposing: true, metaKey: true });
  await dialog.fire("keydown", { key: "Enter", keyCode: 229, ctrlKey: true });
  expect(saves).toEqual([]);
  expect(dialog.removed).toBe(false);
  await dialog.fire("keydown", { key: "Enter", keyCode: 13 });
  expect(saves).toHaveLength(1);
});

test("Escape and cancel discard edits without saving or deleting", async () => {
  let deleted = 0;
  const { dialog, saves } = setup({ remove: async () => { deleted++; } });
  await dialog.fire("keydown", { key: "Escape" });
  expect(dialog.removed).toBe(true);
  expect(saves).toEqual([]);
  expect(deleted).toBe(0);
  const other = setup();
  await other.dialog.fire("cancel");
  expect(other.dialog.removed).toBe(true);
  expect(other.saves).toEqual([]);
});

test("validation and folder failure keep dialog open with simple Korean", async () => {
  const { dialog, name, folder, saves } = setup({ save: async () => { throw new Error("folder_missing"); } });
  await dialog.button("만들기").fire("click");
  expect(dialog.all().find(e => e.attrs.role === "alert")!.textContent).toBe("작업 폴더를 선택하세요.");
  folder.value = "/missing";
  name.value = "   ";
  await dialog.button("만들기").fire("click");
  expect(dialog.all().find(e => e.attrs.role === "alert")!.textContent).toContain("팀 이름");
  name.value = "검토팀";
  await dialog.button("만들기").fire("click");
  expect(dialog.all().find(e => e.attrs.role === "alert")!.textContent).toBe("작업 폴더를 찾을 수 없습니다. 폴더를 다시 선택하세요.");
  expect(dialog.removed).toBe(false);
  expect(saves).toEqual([]);
});

test("edit and delete use callbacks without starting runtime or touching files", async () => {
  let deleted = 0;
  const { dialog, name, folder } = setup({ team: { id: "fixed", name: "제작팀", folder: "/fixture" }, remove: async () => { deleted++; } });
  expect(name.value).toBe("제작팀");
  expect(folder.value).toBe("/fixture");
  expect(dialog.all().some(e => e.textContent === "작업 폴더와 파일은 삭제되지 않습니다.")).toBe(true);
  await dialog.button("팀 삭제").fire("click");
  expect(deleted).toBe(1);
  expect(dialog.removed).toBe(true);
});

test("busy submit is deduplicated", async () => {
  let finish!: () => void;
  let calls = 0;
  const { dialog, folder } = setup({ save: async () => { calls++; await new Promise<void>(r => { finish = r; }); } });
  folder.value = "/fixture";
  const first = dialog.button("만들기").fire("click");
  await dialog.fire("keydown", { key: "Enter", keyCode: 13 });
  expect(calls).toBe(1);
  finish();
  await first;
  expect(dialog.removed).toBe(true);
});

test("Enter on a focused button keeps its native action; composing Enter cancels the default", async () => {
  const { dialog, folder, saves } = setup();
  folder.value = "/fixture";
  let prevented = false;
  await dialog.fire("keydown", { key: "Enter", keyCode: 13, target: dialog.button("폴더 선택"), preventDefault() { prevented = true; } });
  expect(saves).toEqual([]);
  expect(prevented).toBe(false);
  await dialog.fire("keydown", { key: "Enter", keyCode: 229, target: dialog.button("만들기"), preventDefault() { prevented = true; } });
  expect(prevented).toBe(true);
  expect(saves).toEqual([]);
});

test("structured invalid_folder rejection shows the required Korean error for saving and picking", async () => {
  const rejection = { code: "invalid_folder", message: "작업 폴더를 찾을 수 없습니다. 폴더를 다시 선택하세요." };
  const saving = setup({ save: async () => { throw rejection; } });
  saving.folder.value = "/missing";
  await saving.dialog.button("만들기").fire("click");
  expect(saving.dialog.all().find(e => e.attrs.role === "alert")!.textContent)
    .toBe("작업 폴더를 찾을 수 없습니다. 폴더를 다시 선택하세요.");
  expect(saving.dialog.removed).toBe(false);
  const picking = setup({ chooseFolder: async () => { throw rejection; } });
  await picking.dialog.button("폴더 선택").fire("click");
  expect(picking.dialog.all().find(e => e.attrs.role === "alert")!.textContent)
    .toBe("작업 폴더를 찾을 수 없습니다. 폴더를 다시 선택하세요.");
  expect(picking.dialog.removed).toBe(false);
});

test("every composition guard blocks plain, Command and Control Enter", async () => {
  const { dialog, folder, saves } = setup();
  folder.value = "/fixture";
  for (const modifier of [{}, { metaKey: true }, { ctrlKey: true }]) {
    for (const guard of ["state", "isComposing", "keyCode"] as const) {
      let prevented = false;
      if (guard === "state") await dialog.fire("compositionstart");
      await dialog.fire("keydown", { key: "Enter", keyCode: guard === "keyCode" ? 229 : 13,
        isComposing: guard === "isComposing", ...modifier, preventDefault() { prevented = true; } });
      if (guard === "state") await dialog.fire("compositionend");
      expect(prevented).toBe(true);
      expect(saves).toEqual([]);
    }
  }
});

test("structured backend messages are shown verbatim and validation branches on code", async () => {
  const message = "팀 정보를 저장하지 못했습니다. 폴더 권한을 확인하세요.";
  const store = setup({ save: async () => { throw { code: "store_io", message }; } });
  store.folder.value = "C:\\Users\\최형석\\내 팀 폴더";
  await store.dialog.button("만들기").fire("click");
  expect(store.dialog.all().find(e => e.attrs.role === "alert")!.textContent).toBe(message);
  expect(store.folder.attrs["aria-invalid"]).not.toBe("true");
  const invalid = setup({ save: async () => { throw { code: "invalid_name", message: "팀 이름을 다시 입력하세요." }; } });
  invalid.folder.value = "/fixture";
  await invalid.dialog.button("만들기").fire("click");
  expect(invalid.dialog.all().find(e => e.attrs.role === "alert")!.textContent).toBe("팀 이름을 다시 입력하세요.");
  expect(invalid.name.attrs["aria-invalid"]).toBe("true");
});
