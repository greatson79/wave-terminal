import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

test("a refused daemon migration shows the Korean toast and the UI is not left waiting for the daemon", () => {
  expect(source).toContain('"새 데몬으로 옮기지 못해 기존 데몬으로 계속 실행합니다. 컴퓨터를 다시 켜면 새 판이 적용됩니다."');
  const afterReady = source.slice(source.indexOf('const status = (await invoke("daemon_status"))'));
  const toastAt = afterReady.indexOf("MIGRATION_REFUSED_TOAST");
  expect(toastAt).toBeGreaterThan(0);
  expect(afterReady.indexOf('invoke("migration_notice")')).toBeLessThan(toastAt);
  expect(afterReady.slice(0, toastAt)).toContain("headerDaemonInfo.textContent += ` · ${migrationNotice}`"); // detail kept in #daemon-info
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
