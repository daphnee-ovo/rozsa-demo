import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, writeFile, rename, cp, symlink } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { checkResourceLoading } from "../build-smoke/resource-loading.ts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const base = join(root, "tmp/migration-build-resources"); await mkdir(base, { recursive: true });
const output = await mkdtemp(join(base, "run-"));
const compiler = join(root, "desktop/node_modules/typescript/bin/tsc");
const version = spawnSync(process.execPath, [compiler, "--version"], { encoding: "utf8" });
assert.equal(version.status, 0, version.stderr);
assert.equal(version.stdout.trim(), "Version 7.0.2");
const sources = ["desktop/renderer/index.html", "crates/rozsa-gui/frontend/ui/styles/foundations/tokens.css", "crates/rozsa-gui/frontend/ui/styles/features/sidebar.css"];
const targets = ["browser/renderer/index.html", "browser/ui/tokens.css", "browser/ui/sidebar.css"];
const resourceRoot = join(output, "sources"), outDir = join(output, "dist");
for (const source of sources) { const path = join(resourceRoot, source); await mkdir(dirname(path), { recursive: true }); await writeFile(path, await readFile(join(root, source))); }
const command = join(root, "desktop/scripts/build.mjs");
const built = spawnSync(process.execPath, [command, "--out-dir", outDir, "--resource-root", resourceRoot], { encoding: "utf8" });
assert.equal(built.status, 0, built.stdout + built.stderr);
const report = await checkResourceLoading(outDir);
const mainJS = await readFile(join(outDir, "node/main/entry.js"), "utf8");
assert.match(mainJS, /import .* from "electron"/);
assert.match(mainJS, /import\.meta\.dirname/);
assert.doesNotMatch(mainJS, /require\(|__dirname/);
for (const [i, source] of sources.entries()) assert.deepEqual(await readFile(join(outDir, targets[i])), await readFile(join(root, source)));

// TS7's effective configurations keep DOM out of main/preload and Node out of renderer.
for (const name of ["main", "preload", "renderer"]) {
  const config = spawnSync(process.execPath, [join(root, "desktop/node_modules/typescript/bin/tsc"), "-p", join(root, `desktop/tsconfig.${name}.json`), "--showConfig"], { encoding: "utf8" });
  assert.equal(config.status, 0, config.stderr);
  const options = JSON.parse(config.stdout).compilerOptions;
  assert.equal(options.strict, true); assert.equal(options.sourceMap, true); assert.equal(options.inlineSources, true); assert.equal(options.noEmitOnError, true);
  assert.deepEqual(options.types, name === "renderer" ? [] : ["node"]);
  assert.equal(options.moduleResolution, name === "main" ? "nodenext" : "bundler");
  assert.equal(options.module, name === "main" ? "nodenext" : "es2022");
  assert.equal(options.lib.includes("dom"), name === "renderer");
  const probe = join(output, `probe-${name}`); await mkdir(probe);
  const configPath = join(probe, "tsconfig.json"), sourcePath = join(probe, "probe.ts");
  await writeFile(configPath, JSON.stringify({ extends: join(root, `desktop/tsconfig.${name}.json`), compilerOptions: { rootDir: probe, noEmit: true, typeRoots: [join(root, "desktop/node_modules/@types")] }, include: ["probe.ts"], files: ["probe.ts"] }));
  const check = () => spawnSync(process.execPath, [join(root, "desktop/node_modules/typescript/bin/tsc"), "-p", configPath], { encoding: "utf8" });
  await writeFile(sourcePath, name === "renderer" ? "document.title = 'probe';\n" : "process.cwd();\n");
  const allowed = check(); assert.equal(allowed.status, 0, allowed.stdout + allowed.stderr);
  await writeFile(sourcePath, name === "renderer" ? "process.cwd();\n" : "document.title = 'probe';\n");
  const rejected = check(); assert.notEqual(rejected.status, 0); assert.match(rejected.stdout, /Cannot find name '(process|document)'/);
}
// Use an isolated copy to reject type errors and missing local compiler without
// editing shared sources or discovering a global/older compiler from PATH.
const invalidDesktop = join(output, "invalid/desktop"); await mkdir(invalidDesktop, { recursive: true });
for (const path of ["package.json", "tsconfig.shared.json", "tsconfig.main.json", "tsconfig.preload.json", "tsconfig.renderer.json", "scripts", "main", "preload", "renderer"]) {
  await cp(join(root, "desktop", path), join(invalidDesktop, path), { recursive: true });
}
await symlink(join(root, "desktop/node_modules"), join(invalidDesktop, "node_modules"), process.platform === "win32" ? "junction" : "dir");
await writeFile(join(invalidDesktop, "main/ts7-error.ts"), "export const invalid: string = 123;\n");
const invalidCommand = join(invalidDesktop, "scripts/build.mjs");
for (const flags of [["--typecheck"], ["--resource-root", resourceRoot]]) {
  const failed = spawnSync(process.execPath, [invalidCommand, ...flags], { encoding: "utf8" });
  assert.notEqual(failed.status, 0); assert.match(failed.stdout, /TS2322/);
  assert.doesNotMatch(failed.stdout, /ready|typecheck passed/);
}
await assert.rejects(readFile(join(invalidDesktop, "dist/node/main/entry.js")), { code: "ENOENT" });
await rename(join(invalidDesktop, "node_modules"), join(invalidDesktop, "node_modules.present"));
await mkdir(join(invalidDesktop, "node_modules"));
await symlink(join(root, "desktop/node_modules/esbuild"), join(invalidDesktop, "node_modules/esbuild"), process.platform === "win32" ? "junction" : "dir");
const missingCompiler = spawnSync(process.execPath, [invalidCommand, "--typecheck"], { encoding: "utf8" });
assert.notEqual(missingCompiler.status, 0); assert.match(missingCompiler.stderr, /ENOENT.*typescript/);
assert.doesNotMatch(missingCompiler.stdout, /typecheck passed/);
await mkdir(join(invalidDesktop, "node_modules/typescript"));
const compilerPackage = join(invalidDesktop, "node_modules/typescript/package.json");
await writeFile(compilerPackage, JSON.stringify({ version: "5.9.3" }));
const wrongCompiler = spawnSync(process.execPath, [invalidCommand, "--typecheck"], { encoding: "utf8" });
assert.notEqual(wrongCompiler.status, 0); assert.match(wrongCompiler.stderr, /TS7 7\.0\.2 required; installed 5\.9\.3/);
await writeFile(compilerPackage, JSON.stringify({ version: "7.0.2" }));
const missingBinary = spawnSync(process.execPath, [invalidCommand, "--typecheck"], { encoding: "utf8" });
assert.notEqual(missingBinary.status, 0); assert.match(missingBinary.stderr, /MODULE_NOT_FOUND/);
assert.doesNotMatch(missingBinary.stdout, /typecheck passed/);
let logs = "";
const watcher = spawn(process.execPath, [command, "--watch", "--out-dir", outDir, "--resource-root", resourceRoot], { stdio: ["ignore", "pipe", "pipe"] });
watcher.stdout.on("data", b => logs += b.toString()); watcher.stderr.on("data", b => logs += b.toString());
const exit = new Promise<number | null>((accept, reject) => { watcher.once("exit", accept); watcher.once("error", reject); });
async function waitFor(test: () => Promise<boolean>, label: string): Promise<void> {
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    if (await test()) return;
    if (watcher.exitCode !== null || watcher.signalCode !== null) throw new Error(`Watch exited: ${logs}`);
    await new Promise(accept => setTimeout(accept, 100));
  }
  throw new Error(`Timed out: ${label}\n${logs}`);
}
try {
  await waitFor(async () => logs.includes("Watch ready"), "initial watch build");
  for (const [i, source] of sources.entries()) {
    const path = join(resourceRoot, source), updated = (await readFile(path, "utf8")) + `\n${i === 0 ? "<!--" : "/*"} watch ${i} ${i === 0 ? "-->" : "*/"}\n`;
    // Atomic editor saves replace the original inode.
    await writeFile(path + ".save", updated); await rename(path + ".save", path);
    await waitFor(async () => (await readFile(join(outDir, targets[i]), "utf8")) === updated, source);
  }
  const required = join(resourceRoot, sources[2]); await rename(required, required + ".missing");
  await waitFor(async () => logs.includes("Watch build failed:") && logs.includes("ENOENT"), "missing input reports failure");
  const readiness = logs.split("Watch ready").length;
  await rename(required + ".missing", required);
  await waitFor(async () => logs.split("Watch ready").length > readiness, "recovery after restoring source");
  const missing = spawnSync(process.execPath, [join(root, "desktop/scripts/resources.mjs"), "--resource-root", join(output, "absent"), "--out-dir", join(output, "missing-dist")], { encoding: "utf8" });
  assert.notEqual(missing.status, 0); assert.match(missing.stderr, /ENOENT/);
} finally {
  watcher.kill("SIGTERM");
  const code = await Promise.race([exit, new Promise<never>((_, reject) => { const timer = setTimeout(() => reject(new Error("Watch shutdown timed out")), 5000); timer.unref(); })]);
  assert.equal(code, 0);
  await writeFile(join(output, "watch.log"), logs);
}
// A resource-only edit after a failed TS build must retry compilation before readiness.
const recoveryDir = join(output, "compile-recovery"); await mkdir(recoveryDir);
await writeFile(join(recoveryDir, "node"), "Deliberately block compiler output in this isolated fixture");
let recoveryLogs = "";
const recoveryWatcher = spawn(process.execPath, [command, "--watch", "--out-dir", recoveryDir, "--resource-root", resourceRoot], { stdio: ["ignore", "pipe", "pipe"] });
recoveryWatcher.stdout.on("data", b => recoveryLogs += b.toString()); recoveryWatcher.stderr.on("data", b => recoveryLogs += b.toString());
const recoveryExit = new Promise<number | null>((accept, reject) => { recoveryWatcher.once("exit", accept); recoveryWatcher.once("error", reject); });
const waitRecovery = async (test: () => boolean) => {
  const deadline = Date.now() + 20000;
  while (!test()) {
    if (Date.now() > deadline || recoveryWatcher.exitCode !== null) throw new Error(`Compiler recovery timed out:\n${recoveryLogs}`);
    await new Promise(accept => setTimeout(accept, 100));
  }
};
try {
  await waitRecovery(() => recoveryLogs.includes("Watch build failed:"));
  assert.ok(!recoveryLogs.includes("Watch ready"));
  await rename(join(recoveryDir, "node"), join(recoveryDir, "node.blocker"));
  const path = join(resourceRoot, sources[0]); await writeFile(path, (await readFile(path, "utf8")) + "\n<!-- compiler retry -->\n");
  await waitRecovery(() => recoveryLogs.includes("Watch ready"));
  await checkResourceLoading(recoveryDir);
} finally {
  recoveryWatcher.kill("SIGTERM");
  assert.equal(await recoveryExit, 0);
  await writeFile(join(output, "compiler-recovery.log"), recoveryLogs);
}
await writeFile(join(output, "result.json"), JSON.stringify({ platform: process.platform, compiler: version.stdout.trim(), mainESM: true, typedBuildFailure: true, missingCompilerFailure: true, report, resourcesVerbatim: true, environmentBoundaries: true, atomicSave: true, missingInput: true, recovery: true, compilerRecovery: true, watchStopped: true }, null, 2));
console.log(`Build resources PASS: ${output}`);
