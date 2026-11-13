import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { paneHeading, splitPaneHeading } from "./paneheading";

test.each([
  ["coo", "싱크·경영본부·벤·COO·s2061", 2061, "벤 · s2061"],
  ["worker-mgmt-claude-1", "싱크·경영본부·세라·비서1·s2083", 2083, "세라 · s2083"],
  ["worker-edu-codex-3", "스트림·Edu본부·SV13·워커·s2114", 2114, "워커 SV13 · s2114"],
  ["worker-dev-claude-1", "펄스·개발본부·본부장·s2066", 2066, "본부장 펄스 · s2066"],
])("structured seat %s has a simple named heading", (role, title, sid, expected) => {
  expect(paneHeading({ role, title, sid, model: "claude-opus-5-5", ctx: 45, tail: "editing", label: "task" }))
    .toBe(expected);
});

test("learner defaults and malformed identities preserve the role and label without telemetry", () => {
  // workflow_title in src/bin/cys.rs: {role}-{agent} · {folder}, or {role}-{agent}.
  for (const title of [null, "", "worker", "worker-claude · lesson", "worker-claude", "s4",
    "개발본부·s4·워커", "팀·개발본부··워커·s4",
    "팀·개발본부·교육본부·워커·s4"]) {
    expect(paneHeading({ sid: 4, role: "worker", title, label: "lesson", model: "gpt-5.6", ctx: 45, tail: "editing" }))
      .toBe("워커(lesson)·s4");
  }
});

test("duty determines secretary or COO display without staff lists", () => {
  for (const duty of ["COO", "비서2", "비서17"]) {
    expect(paneHeading({ sid: 5, role: "worker-2", title: `Team·개발본부·NewName·${duty}·s5` }))
      .toBe("NewName · s5");
  }
  expect(paneHeading({ sid: 5, role: "coo", title: "Team·개발본부·NewName·워커·s5" }))
    .toBe("워커 NewName · s5");
});

test("master CSO and reviewers keep their role identity even with a worker duty title", () => {
  for (const [role, expected] of [["master", "마스터"], ["cso", "CSO"], ["reviewer-codex", "리뷰어"]]) {
    expect(paneHeading({ sid: 5, role, title: "Team·개발본부·NewName·워커·s5", model: "model", ctx: 45, tail: "editing" }))
      .toBe(`${expected}·s5`);
  }
});

test("actual heading renderer omits model context and activity for every seat", () => {
  const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
  const start = source.indexOf("function displayPaneHeading(");
  const end = source.indexOf("\ninterface PaneRuntime", start);
  const render = new Function("paneHeading", "splitPaneHeading", "document", "sevClass",
    new Bun.Transpiler({ loader: "ts" }).transformSync(source.slice(start, end)) + "return displayPaneHeading;")(
    paneHeading, splitPaneHeading, { createElement: () => ({ textContent: "" }) }, () => "warning");
  for (const role of ["master", "cso", "reviewer-codex", "worker", "coo"]) {
    let text = "";
    const el = { isContentEditable: false, replaceChildren: (...nodes: any[]) => {
      text = nodes.map((node) => typeof node === "string" ? node : node.textContent).join("");
    } };
    render(el, 5, { role, reported: "editing", usage: { ctx_pct: 45, source: "statusline" }, model: "claude-opus-5-5" });
    expect(text).toBe(paneHeading({ sid: 5, role }));
    expect(text).not.toMatch(/CTX|Opus|작업 중|대기 중/);
  }
});
