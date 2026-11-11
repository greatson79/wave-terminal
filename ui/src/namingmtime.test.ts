import { expect, test } from "bun:test";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";

const source = readFileSync(new URL("../../src-tauri/src/main.rs", import.meta.url), "utf8");

test("mtime bridge has no path argument and resolves the existing fixed naming path", () => {
  expect(source.includes('fn naming_config_mtime() -> Option<String>')).toBe(true);
  const start = source.indexOf("fn naming_config_mtime()");
  const command = source.slice(start, source.indexOf("\n}", start) + 2);
  expect(command).toContain('cys::home_dir().join(".cys/naming.json")');
  expect(command).not.toContain("read_to_string");
  expect(source).toContain("            naming_config_mtime,");
});

test("Rust mtime command: absent file, fixed path and unchanged metadata", () => {
  const start = source.indexOf("fn naming_config_mtime()");
  expect(start).toBeGreaterThan(-1);
  const command = source.slice(start, source.indexOf("\n}", start) + 2);
  const root = mkdtempSync(join(process.platform === "darwin" ? "/private/tmp" : tmpdir(), "wave-test-naming-"));
  writeFileSync(join(root, "mtime.rs"), `
    mod cys { pub fn home_dir() -> std::path::PathBuf { std::env::var("WAVE_TEST_ROOT").unwrap().into() } }
    ${command}
    #[test] fn absent_file_is_none() { assert_eq!(naming_config_mtime(), None); }
    #[test] fn fixed_path_and_unchanged_mtime() {
      let home = cys::home_dir();
      std::fs::create_dir_all(home.join(".cys")).unwrap();
      std::fs::write(home.join("elsewhere.json"), "ignored").unwrap();
      assert_eq!(naming_config_mtime(), None);
      std::fs::write(home.join(".cys/naming.json"), "{}").unwrap();
      let first = naming_config_mtime();
      assert!(first.is_some());
      assert_eq!(naming_config_mtime(), first);
    }
  `);
  const env = { ...process.env, WAVE_TEST_ROOT: root };
  const compile = spawnSync("rustc", ["--test", join(root, "mtime.rs"), "-o", join(root, process.platform === "win32" ? "mtime-tests.exe" : "mtime-tests")], { env, encoding: "utf8" });
  expect(compile.status, compile.stderr).toBe(0);
  const run = spawnSync(join(root, process.platform === "win32" ? "mtime-tests.exe" : "mtime-tests"), ["--test-threads=1"], { env, encoding: "utf8" });
  expect(run.status, run.stdout + run.stderr).toBe(0);
  expect(run.stdout).toContain("2 passed; 0 failed");
});
