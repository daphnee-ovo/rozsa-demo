import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { backendName, backendPath, projectRoot } from "../scripts/backend.mjs";

const backend = resolve(projectRoot, "backend");
const module = await readFile(resolve(backend, "go.mod"), "utf8");
assert.match(module, /^go 1\.27\.2$/m);
const go = (...args: string[]) => {
  const result = spawnSync("go", ["-C", backend, ...args], { encoding: "utf8" });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, result.stdout + result.stderr);
  return result.stdout.trim();
};
const version = go("env", "GOVERSION");
// The Go directive is a minimum, not an exact pin; Go itself rejects older tools.
assert.match(version, /^go\d+\.\d+\.\d+$/);
assert.doesNotMatch(go("env", "GOEXPERIMENT"), /(?:^|,)nojsonv2(?:,|$)/);
const resources = process.platform === "darwin" ? "Rozsa Migration.app/Contents/Resources" : "resources";
const packaged = resolve(projectRoot, "desktop/out", `Rozsa Migration-${process.platform}-${process.arch}`, resources, backendName);
for (const binary of [backendPath, packaged]) {
  const metadata = go("version", "-m", binary);
  assert.ok(metadata.split("\n")[0].endsWith(`: ${version}`), metadata);
  assert.doesNotMatch(metadata, /GOEXPERIMENT=.*nojsonv2/);
  console.log(metadata);
}
console.log(`Go toolchain PASS: module >=1.27.2; selected ${version}; development/package metadata agree`);
