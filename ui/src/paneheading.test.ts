import { expect, test } from "bun:test";
import { bindPaneRename, paneHeading, splitPaneHeading } from "./paneheading";

test("role, model, surface ID, context, and reported status compose a fixed heading", () => {
  expect(paneHeading({ sid: 8, role: "master", model: "claude-opus-5-5", ctx: 42, tail: "홈페이지 글 고치는 중" }))
    .toBe("마스터 Opus 5.5·s8 · CTX 42% · 작업 중");
  expect(paneHeading({ sid: 11, role: "worker", model: "claude-sonnet-5-5", ctx: 18 }))
    .toBe("워커 Sonnet 5.5·s11 · CTX 18% · 대기 중");
  expect(paneHeading({ sid: 11, role: "worker-2", model: "gpt-5.6-luna", ctx: 18, approximate: true }))
    .toBe("워커 gpt-5.6-luna·s11 · CTX ~18% · 대기 중");
});

test("missing values do not invent a model or zero context", () => {
  expect(paneHeading({ sid: 11, role: "worker", ctx: 18, tail: "작업" }))
    .toBe("워커·s11 · CTX 18% · 작업 중");
  expect(paneHeading({ sid: 8, model: "claude-opus-5-5" }))
    .toBe("Opus 5.5·s8 · CTX — · 대기 중");
  expect(paneHeading({ sid: 3, role: "reviewer-1", model: "claude-unknown" }))
    .toBe("리뷰어 unknown·s3 · CTX — · 대기 중");
});

test("user label appears beside the role without changing model and status", () => {
  expect(paneHeading({ sid: 8, role: "worker", label: "숙제", model: "claude-sonnet-5-5", ctx: 42 }))
    .toBe("워커(숙제) Sonnet 5.5·s8 · CTX 42% · 대기 중");
});

test("user label without a role leads the heading", () => {
  expect(paneHeading({ sid: 8, label: "숙제", model: "claude-sonnet-5-5" }))
    .toBe("숙제 · s8 · CTX — · 대기 중");
});

test("context coloring splits at the final context marker", () => {
  const heading = paneHeading({ sid: 8, role: "worker", label: "과제 · CTX 정리", ctx: 42 });
  expect(splitPaneHeading(heading)).toEqual(["워커(과제 · CTX 정리)·s8", "42% · 대기 중"]);
});

test("IME Enter does not save a pane name; one later Enter does", () => {
  const listeners = new Map<string, EventListener>();
  const titleEl = {
    contentEditable: "true",
    addEventListener: (type: string, handler: EventListener) => listeners.set(type, handler),
    removeEventListener: (type: string) => listeners.delete(type),
    blur: () => listeners.get("blur")?.({} as Event),
  } as unknown as HTMLElement;
  const calls: string[] = [];
  bindPaneRename(titleEl, (cancelled) => { if (!cancelled) calls.push("set_surface_label"); });
  const keydown = (key: string, isComposing: boolean, keyCode: number) =>
    listeners.get("keydown")?.({ key, isComposing, keyCode, stopPropagation() {}, preventDefault() {} } as KeyboardEvent);
  keydown("Enter", true, 13);
  keydown("Enter", false, 229);
  expect(calls).toEqual([]);
  expect(titleEl.contentEditable).toBe("true");
  keydown("Enter", false, 13);
  expect(calls).toEqual(["set_surface_label"]);
  expect(titleEl.contentEditable).toBe("false");
});

test("department heads use their title name without changing the routing role or status", () => {
  const input = { sid: 1921, role: "worker-edu-claude-1", title: "스프링·교육본부·본부장·s1921",
    label: "수업 준비", model: "claude-opus-5-5", ctx: 23 };
  expect(paneHeading(input)).toBe("본부장 스프링 Opus 5.5 · s1921 · CTX 23% · 대기 중");
  expect(input.role).toBe("worker-edu-claude-1");
  expect(paneHeading({ ...input, title: "펄스·개발본부·본부장·s1921", tail: "작업", approximate: true }))
    .toBe("본부장 펄스 Opus 5.5 · s1921 · CTX ~23% · 작업 중");
});

test("student department masters use the same explicit head title rule", () => {
  expect(paneHeading({ sid: 4, role: "master", title: "스프링·교육본부·본부장·s4", model: "claude-opus-5-5" }))
    .toBe("본부장 스프링 Opus 5.5 · s4 · CTX — · 대기 중");
});

test("an explicit named duty takes precedence over the department nickname", () => {
  expect(paneHeading({ sid: 4, role: "worker-edu-claude-1", title: "교육·교육본부·스프링 본부장·s4" }))
    .toBe("본부장 스프링 · s4 · CTX — · 대기 중");
  expect(paneHeading({ sid: 4, role: "worker", title: "  스프링  · 교육본부 · 본부장 · s4 [exited] " }))
    .toBe("본부장 스프링 · s4 · CTX — · 대기 중");
});

test("workers and other seat kinds keep their existing heading without an explicit head identity", () => {
  for (const role of ["master", "cso", "coo", "secretary", "worker-edu-claude-2", "reviewer-codex"]) {
    const input = { sid: 4, role, label: "숙제", model: "claude-opus-5-5", ctx: 23 };
    expect(paneHeading({ ...input, title: "스프링·교육본부·작성자·s4" })).toBe(paneHeading(input));
  }
  expect(paneHeading({ sid: 4, role: "worker-edu-claude-2", label: "숙제", title: "팀원·교육본부·작성자·s4" }))
    .toBe("워커(숙제)·s4 · CTX — · 대기 중");
});

test("nameless and odd titles fall back to today's heading, including labels", () => {
  for (const role of ["master", "worker-edu-claude-1"]) {
    for (const title of [null, "", "surface 4", "교육본부·본부장·s4", "·교육본부·본부장·s4",
      "s4·교육본부·본부장·s4", "교육본부·교육본부·본부장·s4", "본부장·교육본부·본부장·s4",
      "스프링·교육본부··본부장·s4", "스프링·교육본부·부본부장·s4", "스프링·교육본부·본부장 회의·s4",
      "본부장 작업 메모", "스프링·본부장·s4"]) {
      const input = { sid: 4, role, label: "숙제", ctx: 23 };
      expect(paneHeading({ ...input, title })).toBe(paneHeading(input));
    }
  }
});

test("surface refresh forwards the current title through the actual header renderer", async () => {
  const { readFileSync } = await import("node:fs");
  const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
  const stateStart = source.indexOf("const state = { reported: tasks.get(s.surface_id)");
  const stateEnd = source.indexOf("\n          paneTitleState.set", stateStart);
  const rendererStart = source.indexOf("function displayPaneHeading(");
  const rendererEnd = source.indexOf("\ninterface PaneRuntime", rendererStart);
  expect(stateStart).toBeGreaterThan(0);
  expect(rendererStart).toBeGreaterThan(0);
  const transpiler = new Bun.Transpiler({ loader: "ts" });
  const refreshState = new Function("s", "tasks", transpiler.transformSync(source.slice(stateStart, stateEnd)) + "return state;");
  const render = new Function("paneHeading", "splitPaneHeading", "document", "sevClass",
    transpiler.transformSync(source.slice(rendererStart, rendererEnd)) + "return displayPaneHeading;")(
    paneHeading, splitPaneHeading, { createElement: () => ({ textContent: "" }) }, () => "warning");
  let displayed = "";
  const el = { isContentEditable: false, replaceChildren: (...nodes: any[]) => {
    displayed = nodes.map((n) => typeof n === "string" ? n : n.textContent).join("");
  } };
  const surface = { surface_id: 1921, role: "worker-edu-claude-1", label: "수업", model: "claude-opus-5-5",
    title: "스프링·교육본부·본부장·s1921", exited: false, usage: { ctx_pct: 23, source: "statusline" } };
  render(el, 1921, refreshState(surface, new Map()));
  expect(displayed).toBe("본부장 스프링 Opus 5.5 · s1921 · CTX 23% · 대기 중");
  render(el, 1921, refreshState({ ...surface, title: "팀원·교육본부·작성자·s1921" }, new Map()));
  expect(displayed).toBe("워커(수업) Opus 5.5·s1921 · CTX 23% · 대기 중");
});
