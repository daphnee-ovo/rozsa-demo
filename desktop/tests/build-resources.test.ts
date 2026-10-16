import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { access, cp, mkdir, mkdtemp, readFile, writeFile, rename, symlink } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { checkResourceLoading } from "../build-smoke/resource-loading.ts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const base = join(root, "tmp/migration-forge-resources"); await mkdir(base, { recursive: true });
const output = await mkdtemp(join(base, "run-"));
const desktop = join(root, "desktop"), compiler = join(desktop, "node_modules/typescript/bin/tsc");
const packageManifest = JSON.parse(await readFile(join(desktop, "package.json"), "utf8"));
const expectedCompilerVersion = packageManifest.devDependencies.typescript;
const run = (args: string[]) => spawnSync(process.execPath, args, { cwd: desktop, encoding: "utf8" });
const report = await checkResourceLoading(join(desktop, ".vite"));
for (const name of ["main", "preload", "renderer"]) {
  const probe = join(output, name); await mkdir(probe);
  const path = join(probe, "tsconfig.json"), source = join(probe, "probe.ts");
  await writeFile(path, JSON.stringify({ extends: join(desktop, `tsconfig.${name}.json`), compilerOptions: { rootDir: probe, typeRoots: [join(desktop, "node_modules/@types")] }, files: ["probe.ts"], include: ["probe.ts"] }));
  await writeFile(source, name === "renderer" ? "document.title = 'probe';" : "process.cwd();");
  assert.equal(run([compiler, "-p", path]).status, 0);
  await writeFile(source, name === "renderer" ? "process.cwd();" : "document.title = 'probe';");
  const rejected = run([compiler, "-p", path]); assert.notEqual(rejected.status, 0); assert.match(rejected.stdout, /Cannot find name '(process|document)'/);
}
// Isolated bad input verifies the TS7 gate without editing shared source files.
const invalid = join(output, "invalid/desktop"); await mkdir(invalid, { recursive: true });
await cp(join(root, "contracts"), join(invalid, "../contracts"), { recursive: true });
for (const path of ["package.json", "forge-env.d.ts", "tsconfig.shared.json", "tsconfig.main.json", "tsconfig.preload.json", "tsconfig.renderer.json", "scripts", "main", "preload", "renderer"]) await cp(join(desktop, path), join(invalid, path), { recursive: true });
await symlink(join(desktop, "node_modules"), join(invalid, "node_modules"), process.platform === "win32" ? "junction" : "dir");
await writeFile(join(invalid, "main/error.ts"), "export const invalid: string = 123;");
const check = () => run([join(invalid, "scripts/typecheck.mjs")]);
assert.notEqual(check().status, 0); assert.match(check().stdout, /TS2322/);
await assert.rejects(access(join(invalid, ".vite")), { code: "ENOENT" });
await rename(join(invalid, "node_modules"), join(invalid, "modules.present"));
assert.notEqual(check().status, 0); assert.match(check().stderr, /ENOENT.*typescript/);
await mkdir(join(invalid, "node_modules/typescript"), { recursive: true });
const wrongCompilerVersion = "5.9.3";
await writeFile(join(invalid, "node_modules/typescript/package.json"), JSON.stringify({ version: wrongCompilerVersion }));
const wrongCompiler = check();
assert.notEqual(wrongCompiler.status, 0);
assert.ok(wrongCompiler.stderr.includes(`TS7 ${expectedCompilerVersion} required; installed ${wrongCompilerVersion}`));
const config = (await import("../forge.config.mjs")).default;
assert.throws(() => config.hooks.prePackage(config, process.platform === "win32" ? "linux" : "win32", process.arch), /requires host/);
// Cross-platform path contract: packager extraResource sits outside app.asar.
assert.ok(config.packagerConfig.asar); assert.ok(config.packagerConfig.extraResource[0].endsWith(process.platform === "win32" ? "rozsa-backend.exe" : "rozsa-backend"));
assert.equal(packageManifest.main, ".vite/build/main.cjs");
await writeFile(join(output, "result.json"), JSON.stringify({ platform: process.platform, report, noEmit: true, strictBoundaries: true, typeErrorGate: true, missingCompilerGate: true, wrongCompilerGate: true, crossPackageRejected: true }, null, 2));
console.log(`Forge resources PASS: ${output}`);
