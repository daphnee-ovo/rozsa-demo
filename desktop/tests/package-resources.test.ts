import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { cp, mkdtemp, rename, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { listPackage } from "@electron/asar";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const product = "Rózsa";
const packageDir = join(root, "desktop/out", `${product}-${process.platform}-${process.arch}`);
const appBundle = process.env.ROZSA_TEST_APP_BUNDLE
  ? resolve(process.env.ROZSA_TEST_APP_BUNDLE)
  : process.platform === "darwin" ? join(packageDir, `${product}.app`) : packageDir;
const resources = process.platform === "darwin" ? join(appBundle, "Contents/Resources") : join(appBundle, "resources");
const executable = process.platform === "darwin"
  ? join(appBundle, "Contents/MacOS/rozsa")
  : join(appBundle, process.platform === "win32" ? "rozsa.exe" : "rozsa");
const backendName = process.platform === "win32" ? "rozsa-backend.exe" : "rozsa-backend";

assert.notEqual(process.platform, "linux", "T014 installer acceptance runs makers on their host; Linux acceptance belongs to T057");
const archive = join(resources, "app.asar");
const backend = join(resources, backendName);
const files = listPackage(archive, {});
assert.ok(files.includes("/.vite/build/main.cjs"), "packaged Electron main entry exists");
assert.ok(files.includes("/.vite/build/preload.cjs"), "packaged sandbox preload exists");
assert.ok(files.some(path => path.startsWith("/.vite/renderer/") && path.endsWith(".html")), "packaged UI HTML exists");
assert.ok(files.some(path => path.startsWith("/.vite/renderer/") && path.endsWith(".css")), "packaged UI stylesheet exists");
assert.ok((await stat(backend)).isFile(), "Go backend is installed as an external resource");
if (process.platform !== "win32") assert.ok((await stat(backend)).mode & 0o111, "installed Go backend is executable");

// A damaged copy exercises the actual packaged entry's startup diagnostic without
// changing the maker output or the user's installed application.
const scratch = await mkdtemp(join(tmpdir(), "rozsa-package-"));
const damagedApp = join(scratch, `${product}.app`);
await cp(appBundle, damagedApp, { recursive: true });
const damagedBackend = process.platform === "darwin"
  ? join(damagedApp, "Contents/Resources", backendName)
  : join(damagedApp, "resources", backendName);
await rename(damagedBackend, `${damagedBackend}.missing`);
const damagedExecutable = process.platform === "darwin"
  ? join(damagedApp, "Contents/MacOS/rozsa")
  : join(damagedApp, process.platform === "win32" ? "rozsa.exe" : "rozsa");
const child = spawn(damagedExecutable, [`--user-data-dir=${join(scratch, "profile")}`], {
  cwd: scratch,
  stdio: ["ignore", "pipe", "pipe"],
});
let logs = "";
child.stdout.on("data", (value: Buffer) => { logs += value.toString(); });
child.stderr.on("data", (value: Buffer) => { logs += value.toString(); });
let diagnosticSeen = false;
await new Promise<void>((resolve, reject) => {
  const timeout = setTimeout(() => {
    child.kill("SIGTERM");
    reject(new Error(`Packaged startup diagnostic timed out:\n${logs}`));
  }, 15000);
  const check = () => {
    if (!diagnosticSeen && /Rózsa could not start:/.test(logs)) { diagnosticSeen = true; clearTimeout(timeout); resolve(); }
  };
  child.stdout.on("data", check);
  child.stderr.on("data", check);
  child.once("error", error => { clearTimeout(timeout); reject(error); });
  child.once("exit", code => {
    if (!diagnosticSeen) { clearTimeout(timeout); reject(new Error(`Packaged app exited before startup diagnostic (${code}):\n${logs}`)); }
  });
  check();
});
assert.match(logs, /Rózsa could not start:/);
assert.match(logs, /ENOENT/);
if (child.exitCode === null && child.signalCode === null) {
  child.kill("SIGTERM");
  await new Promise<void>(resolve => child.once("exit", () => resolve()));
}

console.log(`Forge packaged resources PASS: ${appBundle}`);
