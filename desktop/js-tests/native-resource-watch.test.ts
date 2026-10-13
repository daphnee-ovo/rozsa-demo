// Real candidate entry/preload/Go, with only validation resources/profile under tmp/.
import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdir, mkdtemp, readFile, writeFile, copyFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const base = join(root, "tmp/migration-native-resource-watch"); await mkdir(base, { recursive: true });
const output = await mkdtemp(join(base, "run-"));
const sourceRoot = join(output, "sources"), candidateRoot = join(output, "candidate"), desktop = join(candidateRoot, "desktop"), outDir = join(desktop, "dist");
const sources = ["desktop/renderer/index.html", "crates/rozsa-gui/frontend/ui/styles/foundations/tokens.css", "crates/rozsa-gui/frontend/ui/styles/features/sidebar.css"];
for (const source of sources) { const path = join(sourceRoot, source); await mkdir(dirname(path), { recursive: true }); await copyFile(join(root, source), path); }
await mkdir(desktop, { recursive: true });
await writeFile(join(desktop, "package.json"), JSON.stringify({ name: "rozsa-t009-validation", main: "dist/node/main/entry.js", type: "module" }));
const backendName = process.platform === "win32" ? "rozsa-backend.exe" : "rozsa-backend";
await mkdir(join(candidateRoot, "dist/migration"), { recursive: true });
await copyFile(join(root, "dist/migration", backendName), join(candidateRoot, "dist/migration", backendName));
const sessions = join(output, "sessions"); await mkdir(sessions);
await writeFile(join(sessions, "watch.jsonl"), JSON.stringify({ type: "session", version: 3, id: "watch", timestamp: "2026-10-13T00:00:00Z", cwd: "/synthetic" }) + "\n");
let watchLogs = "", electronLogs = "";
const watcher = spawn(process.execPath, [join(root, "desktop/scripts/build.mjs"), "--watch", "--resource-root", sourceRoot, "--out-dir", outDir], { stdio: ["ignore", "pipe", "pipe"] });
watcher.stdout.on("data", b => watchLogs += b.toString()); watcher.stderr.on("data", b => watchLogs += b.toString());
const exitOf = (child: ChildProcess) => new Promise<number | null>((accept, reject) => { child.once("exit", accept); child.once("error", reject); });
const watchExit = exitOf(watcher);
async function poll<T>(get: () => Promise<T | undefined>, label: string): Promise<T> {
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    const value = await get(); if (value !== undefined) return value;
    if (watcher.exitCode !== null || watcher.signalCode !== null) throw new Error(`Watcher exited: ${watchLogs}`);
    await new Promise(accept => setTimeout(accept, 100));
  }
  throw new Error(`Timed out: ${label}\n${watchLogs}\n${electronLogs}`);
}
type CDPResult = Record<string, any>;
let electron: ChildProcess | undefined, electronExit: Promise<number | null> | undefined, socket: WebSocket | undefined, normal = false;
const pending = new Map<number, { accept(value: CDPResult): void; reject(error: Error): void; timer: NodeJS.Timeout }>();
async function terminate(child: ChildProcess, exit: Promise<number | null>): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.kill("SIGTERM");
  await Promise.race([exit, new Promise(accept => setTimeout(accept, 3000))]);
  if (child.exitCode === null && child.signalCode === null) { child.kill("SIGKILL"); await exit; }
}
try {
  await poll(async () => watchLogs.includes("Watch ready") ? true : undefined, "initial generated build");
  const binary = join(root, "desktop/node_modules/electron/dist", (await readFile(join(root, "desktop/node_modules/electron/path.txt"), "utf8")).trim());
  electron = spawn(binary, [desktop, "--remote-debugging-port=0", `--user-data-dir=${join(output, "profile")}`, "--session-dir", sessions], { cwd: root, env: { ...process.env, TMPDIR: join(root, "tmp/migration/install-tmp") }, stdio: ["ignore", "pipe", "pipe"] });
  electronExit = exitOf(electron);
  electron.stdout!.on("data", b => electronLogs += b.toString()); electron.stderr!.on("data", b => electronLogs += b.toString());
  const endpoint = await poll(async () => {
    if (electron!.exitCode !== null || electron!.signalCode !== null) throw new Error(`Electron exited: ${electronLogs}`);
    return /DevTools listening on (ws:\/\/\S+)/.exec(electronLogs)?.[1];
  }, "Electron DevTools");
  socket = new WebSocket(endpoint);
  await new Promise<void>((accept, reject) => { socket!.addEventListener("open", () => accept(), { once: true }); socket!.addEventListener("error", reject, { once: true }); });
  let next = 0;
  socket.addEventListener("message", event => {
    const result = JSON.parse(String(event.data)), item = pending.get(result.id); if (!item) return;
    pending.delete(result.id); clearTimeout(item.timer);
    if (result.error) item.reject(new Error(JSON.stringify(result.error))); else item.accept(result.result);
  });
  const send = (method: string, params: object = {}, sessionId?: string): Promise<CDPResult> => new Promise((accept, reject) => {
    const id = ++next, timer = setTimeout(() => { pending.delete(id); reject(new Error(`DevTools timeout: ${method}`)); }, 5000);
    pending.set(id, { accept, reject, timer }); socket!.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
  const target = await poll(async () => (await send("Target.getTargets")).targetInfos.find((t: { type: string; url: string }) => t.type === "page" && t.url.endsWith("/renderer/index.html")), "candidate page");
  const attached = await send("Target.attachToTarget", { targetId: target.targetId, flatten: true });
  const evaluate = async (expression: string) => {
    const result = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, attached.sessionId);
    if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails)); return result.result.value;
  };
  const stateExpression = `({heading:document.querySelector('h1')?.textContent,background:document.body ? getComputedStyle(document.body).backgroundColor : undefined,sessions:document.querySelector('#sessions')?.children.length,node:typeof window.require})`;
  await poll(async () => await evaluate("document.querySelector('#sessions')?.children.length===1") ? true : undefined, "Go session list");
  const before = await evaluate(stateExpression); assert.equal(before.heading, "Sessions"); assert.equal(before.node, "undefined");
  const screenshot = async (name: string) => { const result = await send("Page.captureScreenshot", {}, attached.sessionId); await writeFile(join(output, name), Buffer.from(result.data, "base64")); };
  await screenshot("before.png");
  const html = join(sourceRoot, sources[0]), css = join(sourceRoot, sources[1]);
  await writeFile(html, (await readFile(html, "utf8")).replace("<h1>Sessions</h1>", "<h1>Resource watch verified</h1>"));
  await writeFile(css, (await readFile(css, "utf8")) + "\n:root { --bg: rgb(218, 234, 247) !important; }\n");
  await poll(async () => (await readFile(join(outDir, "browser/renderer/index.html"), "utf8")).includes("Resource watch verified") && (await readFile(join(outDir, "browser/ui/tokens.css"), "utf8")).includes("rgb(218, 234, 247)") ? true : undefined, "watch emits edited isolated inputs");
  await send("Page.reload", { ignoreCache: true }, attached.sessionId);
  const after = await poll(async () => { const state = await evaluate(stateExpression); return state.heading === "Resource watch verified" && state.background === "rgb(218, 234, 247)" && state.sessions === 1 ? state : undefined; }, "actual reloaded heading and computed CSS");
  assert.equal(after.node, "undefined"); await screenshot("after.png");
  assert.match(electronLogs, /Session window shown/);
  socket.send(JSON.stringify({ id: ++next, method: "Runtime.evaluate", params: { expression: "window.close()" }, sessionId: attached.sessionId }));
  const exitCode = await Promise.race([electronExit, new Promise<never>((_, reject) => { const timer = setTimeout(() => reject(new Error("Window close timeout")), 10000); timer.unref(); })]);
  assert.equal(exitCode, 0);
  const backendPID = Number(/Backend started: (\d+)/.exec(electronLogs)?.[1]); assert.ok(backendPID > 0);
  assert.match(electronLogs, new RegExp(`Backend stopped: ${backendPID} \\(0\\)`)); assert.throws(() => process.kill(backendPID, 0), /ESRCH/);
  watcher.kill("SIGTERM"); assert.equal(await watchExit, 0);
  normal = true;
  await writeFile(join(output, "result.json"), JSON.stringify({ platform: process.platform, sourceRoot, outDir, before, after, actualReload: true, windowShown: true, electronExitCode: exitCode, backendPID, backendAbsent: true, watchStopped: true }, null, 2));
  console.log(`Native resource watch PASS: ${output}`);
} finally {
  socket?.close(); for (const item of pending.values()) clearTimeout(item.timer);
  if (electron && electronExit) await terminate(electron, electronExit);
  await terminate(watcher, watchExit);
  if (!normal) {
    const backendPID = Number(/Backend started: (\d+)/.exec(electronLogs)?.[1]);
    if (backendPID > 0) { try { process.kill(backendPID, "SIGTERM"); } catch (error) { if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error; } }
  }
  await writeFile(join(output, "watch.log"), watchLogs); await writeFile(join(output, "electron.log"), electronLogs);
}
