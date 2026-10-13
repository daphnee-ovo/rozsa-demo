import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const desktop = resolve(import.meta.dirname, "..");
const expected = JSON.parse(readFileSync(resolve(desktop, "package.json"))).devDependencies.typescript;
const installed = JSON.parse(readFileSync(resolve(desktop, "node_modules/typescript/package.json"))).version;
if (!expected.startsWith("7.") || installed !== expected) throw new Error(`TS7 ${expected} required; installed ${installed}. Run npm ci in desktop.`);
for (const name of ["main", "preload", "renderer"]) {
  const result = spawnSync(process.execPath, [resolve(desktop, "node_modules/typescript/bin/tsc"), "-p", `tsconfig.${name}.json`, "--noEmit"], { cwd: desktop, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${name} typecheck failed (${result.status ?? result.signal})`);
}
console.log(`TS7 ${installed} typecheck passed (no emit)`);
