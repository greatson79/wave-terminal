import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const start = source.indexOf("function invalidateHelpPreview() {");
const end = source.indexOf("\n}", start) + 2;
const invalidate = new Function("helpMessage", "helpKind", "helpDepartmentHint", "helpSend", "document", `
  let helpEditRevision = 0, previewedHelp = null, helpSent = false;
  const helpPreviewDetails = {}, helpResult = {};
  ${new Bun.Transpiler({ loader: "ts" }).transformSync(source.slice(start, end))}
  invalidateHelpPreview();
`);

for (const kind of ["help", "feedback"]) {
  test(`${kind}: department creation hint does not block sending`, () => {
    for (const value of ["부서를 만들어 주세요", "조직 만들기", "비서 만들어줘"]) {
      const hint = { hidden: true };
      const send = { disabled: true };
      invalidate({ value }, { value: kind }, hint, send, { getElementById: () => ({}) });
      expect(hint.hidden).toBe(false);
      expect(send.disabled).toBe(false);
    }
  });
}

test("unrelated input hides the hint and existing length/empty validation stays", () => {
  for (const value of ["부서가 안 보여요", "파일 만들어줘", "", "가".repeat(501)]) {
    const hint = { hidden: false };
    const send = { disabled: false };
    invalidate({ value }, {}, hint, send, { getElementById: () => ({}) });
    expect(hint.hidden).toBe(true);
    expect(send.disabled).toBe(!value.trim() || value.length > 500);
  }
});

test("visible hint keeps the original message in the normal send flow", async () => {
  const { safeHelpText } = await import("./help");
  const { sendPreparedHelp } = await import("./helpflow");
  const elements = new Map<string, any>();
  const element = () => ({ value: "", textContent: "", hidden: false, disabled: false,
    handlers: {} as Record<string, () => Promise<void> | void>,
    addEventListener(event: string, handler: () => Promise<void> | void) { this.handlers[event] = handler; },
    setAttribute() {}, after() {} });
  const hint = element();
  const document = { getElementById: (id: string) => {
    if (!elements.has(id)) elements.set(id, element());
    return elements.get(id);
  }, createElement: () => hint };
  let sent: any;
  const invoke = async (cmd: string, args: any) => {
    if (cmd === "help_username") return "";
    if (cmd === "prepare_help") return args;
    if (cmd === "send_help") { sent = args.payload; return { id: "R-hint" }; }
    throw new Error(cmd);
  };
  const begin = source.indexOf('const helpBox =');
  const end = source.indexOf('// ---------- layout model', begin);
  new Function("document", "invoke", "safeHelpText", "sendPreparedHelp",
    new Bun.Transpiler({ loader: "ts" }).transformSync(source.slice(begin, end)))(document, invoke, safeHelpText, sendPreparedHelp);
  const message = document.getElementById("help-message");
  message.value = "부서를 만들어 주세요";
  document.getElementById("help-kind").value = "help";
  await message.handlers.input();
  expect(hint.hidden).toBe(false);
  expect(hint.textContent).toBe("부서 만들기는 마스터 채팅창에 입력하세요");
  await document.getElementById("help-send").handlers.click();
  expect(sent).toEqual({ kind: "help", message: "부서를 만들어 주세요" });
  expect(document.getElementById("help-result").textContent).toBe("접수됨 · 접수 번호 R-hint");
});
