import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const start = source.indexOf("async function promptBinaryPatch() {");
const end = source.indexOf("\n}\n", start) + 2;
const prompt = new Function("updateAvailable", "confirmModal", `return (${source.slice(start, end)})();`);

test("shared macOS/Windows binary update confirmation shows Wave manual-install URL", async () => {
  let message = "";
  await prompt({ version: "0.3.2" }, async (_title: string, body: string) => {
    message = body;
    return false;
  });
  expect(message).toContain("https://waveainetworks.com/get");
  expect(message).not.toContain(["cys" + "insight", "com"].join("."));
});
