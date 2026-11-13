import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { sidebarTreeSpec, hqCountLabel } from "./sidebartree";
import { sortHqMembers } from "./hierarchy";
import { teamMemberRows } from "./teamtree";

class Element {
  className = "";
  textContent = "";
  dataset: Record<string, string> = {};
  style = {};
  children: Element[] = [];
  classList = { add: () => {} };
  append(...nodes: Element[]) { this.children.push(...nodes); }
  appendChild(node: Element) { this.append(node); }
  prepend(node: Element) { this.children.unshift(node); }
  insertBefore(node: Element, before: Element) { this.children.splice(this.children.indexOf(before), 0, node); }
  setAttribute() {}
  addEventListener() {}
}
const all = (el: Element): Element[] => [el, ...el.children.flatMap(all)];
const text = (el: Element): string => el.textContent + el.children.map(text).join("");

// Execute the real buildTab body without starting Tauri, xterm, a browser or a daemon.
function sidebar(hierarchyEnabled: boolean, collapsed = false) {
  const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
  const start = source.indexOf("function buildTab(");
  const end = source.indexOf("\n// 06: 그룹 섹션", start);
  const ws = { id: 1, name: "lesson", tree: [4, 5], socket: "lesson-socket", collapsed };
  const key = (sid: number, socket?: string) => `${socket ?? ""}#${sid}`;
  const state = new Map([[key(4, ws.socket), { reported: "editing" }], [key(5, ws.socket), { reported: null }]]);
  const context = {
    WS_COLORS: ["blue"], collectSids: (tree: number[]) => tree, hierarchyEnabled,
    sidebarTreeSpec, createHqTabShell: () => null, hqColors: {}, document: { createElement: () => new Element() },
    workspaces: [ws], activeWs: 0, deptPlaceholderLabel: () => "lesson", tabLabel: () => "lesson", groups: [],
    pendingApprovals: 0, panes: new Map(), paneKey: key, paneTitleState: state,
    // Deliberately opposite to reported: activity must use the former heading's task source.
    nodeSig: new Map([[`${ws.socket}#4`, { role: "worker", state: "idle", ctx_pct: 80, idle_secs: 70 }],
      [`${ws.socket}#5`, { role: "worker", state: "working", ctx_pct: 20, idle_secs: 0 }]]),
    surfaceTitles: new Map([[key(4, ws.socket), `worker-claude · ${"long-name".repeat(30)}`], [key(5, ws.socket), "worker-codex · lesson"]]),
    sortHqMembers, teamMemberRows, hqCountLabel, focusedSid: null,
  };
  const render = new Function(...Object.keys(context),
    new Bun.Transpiler({ loader: "ts" }).transformSync(source.slice(start, end)) + "return buildTab;")(...Object.values(context));
  return { tab: render(ws) as Element, state, render, ws, key };
}

for (const hierarchy of [true, false]) {
  test(`sidebar gives each seat its own task status, hierarchy=${hierarchy}`, () => {
    const fixture = sidebar(hierarchy);
    let rows = all(fixture.tab).filter((el) => el.className.startsWith("ws-member ") || el.className === "ws-member");
    expect(rows).toHaveLength(2);
    expect(text(rows[0])).toStartWith("작업 중 · ");
    expect(text(rows[1])).toContain("대기 중");
    expect(all(rows[0]).some((el) => el.className === "ws-dot working")).toBe(true);
    expect(all(rows[1]).some((el) => el.className === "ws-dot idle")).toBe(true);
    expect(text(fixture.tab)).toContain("CTX 80%");
    fixture.state.set(fixture.key(4, fixture.ws.socket), { reported: null });
    rows = all(fixture.render(fixture.ws)).filter((el) => el.className === "ws-member");
    expect(text(rows[0])).toContain("대기 중");
  });
  test(`collapsed sidebar preserves member hiding, hierarchy=${hierarchy}`, () => {
    expect(all(sidebar(hierarchy, true).tab).filter((el) => el.className === "ws-member")).toHaveLength(0);
  });
}
