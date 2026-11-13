import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

// Execute the actual startup notice block, including its production message selection.
const selection = source.slice(source.indexOf("const MIGRATION_REFUSED_TOAST"), source.indexOf("let rotatingDaemon"));
const noticeStart = source.indexOf('    const migrationNotice = await invoke("migration_notice")');
const noticeEnd = source.indexOf('  accountOwners =', noticeStart);
const displayCode = source.slice(source.lastIndexOf("  try {", noticeStart), noticeEnd);
const code = new Bun.Transpiler({ loader: "ts" }).transformSync(selection + displayCode);
const display = new Function("invoke", "status", "headerDaemonInfo", "toast", `return (async () => { ${code} })();`);
const guidance = "자동으로 새 데몬으로 옮기는 작업이 끝나지 않았습니다. 하던 작업은 계속하셔도 됩니다. 화면 위쪽에 보이는 자세한 오류를 「도움 요청」으로 보내 주세요.";

async function showNotice(daemonVersion: unknown, appVersion: unknown = "0.3.4", notice: string | null = "migration detail") {
  const messages: unknown[][] = [];
  const header = { textContent: "pid=7 sock=test" };
  const calls: string[] = [];
  await display(async (command: string) => {
    calls.push(command);
    if (command === "migration_notice") return notice;
    if (command === "app_version") {
      if (appVersion instanceof Error) throw appVersion;
      return appVersion;
    }
    throw new Error(`unexpected command: ${command}`);
  }, { version: daemonVersion }, header, (...args: unknown[]) => messages.push(args));
  return { messages, header, calls };
}

for (const [label, daemonVersion, appVersion, prefix] of [
  ["older daemon", "0.3.3", "0.3.4", "지금은 이전 판 데몬(v0.3.3)으로 실행 중입니다. "],
  ["same version", "0.3.4", "0.3.4", ""],
  ["unknown daemon version", undefined, "0.3.4", ""],
  ["newer daemon", "0.3.5", "0.3.4", ""],
  ["numeric version order", "0.3.9", "0.3.10", "지금은 이전 판 데몬(v0.3.9)으로 실행 중입니다. "],
  ["malformed daemon version", "unknown", "0.3.4", ""],
  ["unclassified prerelease", "0.3.4-rc.1", "0.3.4", ""],
  ["failed app version check", "0.3.3", new Error("unavailable"), ""],
] as const) {
  test(`migration notice: ${label}`, async () => {
    const result = await showNotice(daemonVersion, appVersion);
    expect(result.messages).toEqual([["health", "업데이트 안내", prefix + guidance]]);
    expect(result.header.textContent).toBe("pid=7 sock=test · migration detail");
  });
}

test("no migration notice means no toast or version lookup", async () => {
  const result = await showNotice("0.3.3", "0.3.4", null);
  expect(result.messages).toEqual([]);
  expect(result.header.textContent).toBe("pid=7 sock=test");
  expect(result.calls).toEqual(["migration_notice"]);
});

test("the backend continues startup after a migration error (no early return) and exposes the notice command", () => {
  const rust = readFileSync(new URL("../../src-tauri/src/main.rs", import.meta.url), "utf8");
  const at = rust.indexOf("let migration = rename_migration::run(settings_file).await;");
  expect(at).toBeGreaterThan(0);
  const block = rust.slice(at, at + 900);
  expect(block).toContain("rename_migration::migration_error(&migration)");
  expect(block).toContain("if !rename_migration::startup_continues(&migration) { return; }");
  expect(block).not.toMatch(/migration_error[\s\S]{0,400}\n\s*return;\s*\n\s*\}/);
  expect(rust).toContain("fn migration_notice()");
  expect(rust).toContain("            migration_notice,\n");
});
