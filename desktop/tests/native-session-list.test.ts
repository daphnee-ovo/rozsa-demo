// Native Electron acceptance for the assembled legacy UI and real session catalog.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, mkdtemp, writeFile, readFile, stat, rename } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const base = join(root, "tmp/migration-native-session-list");
await mkdir(base, { recursive: true });
const output = await mkdtemp(join(base, "run-"));
const global = join(output, "global"), project = join(output, "project");
await mkdir(global); await mkdir(project);
function session(id: string, text: string, timestamp: string): string {
  return JSON.stringify({ type: "session", version: 3, id, timestamp: "2026-10-13T00:00:00Z", cwd: "/synthetic" }) + "\n"
    + JSON.stringify({ type: "message", id: "e", timestamp, message: { role: "user", content: text } }) + "\n";
}
const fixture = join(project, "shared.jsonl");
const fixtureText = session("shared", "Project session · 玫瑰🌹", "2026-10-13T01:00:00Z");
await writeFile(join(global, "shared.jsonl"), session("shared", "Overridden global session", "2026-10-13T03:00:00Z"));
await writeFile(join(global, "recent.jsonl"), session("recent", "Review session catalog", "2026-10-13T02:00:00Z"));
await writeFile(fixture, fixtureText);
await writeFile(join(project, "broken.jsonl"), "malformed header\n");
const before = await stat(fixture);
let logs = "";
const electronBinary = join(root, "desktop/node_modules/electron/dist", (await readFile(join(root, "desktop/node_modules/electron/path.txt"), "utf8")).trim());
const child = spawn(process.env.ROZSA_NATIVE_EXECUTABLE || electronBinary, [...(process.env.ROZSA_NATIVE_EXECUTABLE ? [] : [join(root, "desktop")]), "--remote-debugging-port=0", `--user-data-dir=${join(output, "profile")}`, "--session-dir", global, "--session-dir", project], { cwd: output, env: { ...process.env, TMPDIR: join(root, "tmp/migration/install-tmp") }, stdio: ["ignore", "pipe", "pipe"] });
const exited = new Promise<number | null>((resolve, reject) => { child.once("error", reject); child.once("exit", code => resolve(code)); });
child.stdout.on("data", (data: Buffer) => { logs += data.toString(); });
child.stderr.on("data", (data: Buffer) => { logs += data.toString(); });
async function poll<T>(get: () => Promise<T | undefined>, label: string): Promise<T> {
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    const value = await get();
    if (value !== undefined) return value;
    if (child.exitCode !== null || child.signalCode !== null) throw new Error(`Electron exited during ${label}: ${logs}`);
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error(`Timed out: ${label}\n${logs}`);
}
let socket: WebSocket | undefined;
let closedNormally = false;
try {
  const url = await poll(async () => /DevTools listening on (ws:\/\/\S+)/.exec(logs)?.[1], "DevTools startup");
  socket = new WebSocket(url);
  await new Promise<void>((resolve, reject) => { socket!.addEventListener("open", () => resolve(), { once: true }); socket!.addEventListener("error", reject, { once: true }); });
  let id = 0;
  const pending = new Map<number, { resolve(value: any): void; reject(error: Error): void; timer: NodeJS.Timeout }>();
  socket.addEventListener("message", event => {
    const message = JSON.parse(String(event.data));
    const item = pending.get(message.id);
    if (!item) return;
    pending.delete(message.id); clearTimeout(item.timer);
    if (message.error) item.reject(new Error(JSON.stringify(message.error))); else item.resolve(message.result);
  });
  const send = (method: string, params: object = {}, sessionId?: string): Promise<any> => new Promise((resolve, reject) => {
    const requestID = ++id;
    const timer = setTimeout(() => { pending.delete(requestID); reject(new Error(`DevTools timeout: ${method}`)); }, 5000);
    pending.set(requestID, { resolve, reject, timer });
    socket!.send(JSON.stringify({ id: requestID, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
  const target = await poll(async () => {
    const result = await send("Target.getTargets");
    return result.targetInfos.find((item: { type: string; url: string }) =>
      item.type === "page" && (item.url.endsWith("/index.html") || /^http:\/\/127\.0\.0\.1:\d+\/$/.test(item.url)));
  }, "renderer target");
  const attached = await send("Target.attachToTarget", { targetId: target.targetId, flatten: true });
  const evaluate = async (expression: string): Promise<any> => {
    const result = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, attached.sessionId);
    if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  const snapshot = await poll(async () => {
    const value = await evaluate(`({rows:[...document.querySelectorAll('#sessionList .session-item')].map(row=>row.querySelector('.session-name')?.textContent.trim()),sidebar:(()=>{const el=document.querySelector('#sidebar');if(!el)return {exists:false};const style=getComputedStyle(el),rect=el.getBoundingClientRect();return {exists:true,display:style.display,visibility:style.visibility,width:rect.width,x:rect.x}})(),sidebarToggle:(()=>{const el=document.querySelector('body > .sidebar-toggle-button');return {exists:!!el,visible:!!el&&getComputedStyle(el).display!=='none'}})(),viewport:innerWidth,breakpoints:document.documentElement?.dataset.uiMaxWidth,bodyClass:document.body?.className,main:!!document.querySelector('#mainContentScene'),conversation:!!document.querySelector('#chatMessages'),node:typeof window.require,bridge:Object.keys(window.rozsa||{}),windows:window.frames.length})`);
    return value.rows.length === 2 && value.conversation ? value : undefined;
  }, "full UI and catalog rendering");
  assert.deepEqual(snapshot.rows, ["Review session catalog", "Project session · 玫瑰🌹"]);
  assert.equal(snapshot.sidebar.exists, true); assert.equal(snapshot.sidebar.display, "flex"); assert.equal(snapshot.sidebar.visibility, "visible", JSON.stringify(snapshot)); assert.ok(snapshot.sidebar.width > 0);
  assert.deepEqual(snapshot.sidebarToggle, { exists: true, visible: process.platform === "darwin" });
  assert.equal(snapshot.main, true); assert.equal(snapshot.conversation, true);
  if (process.platform === "darwin") {
    const collapsedBoundary = await evaluate(`document.querySelector('body > .sidebar-toggle-button').click(); getComputedStyle(document.documentElement).getPropertyValue('--chrome-sidebar-boundary').trim()`);
    assert.equal(collapsedBoundary, "0px");
    const edgeVisibleBoundary = await evaluate(`setSidebarEdgeVisible(true); getComputedStyle(document.documentElement).getPropertyValue('--chrome-sidebar-boundary').trim()`);
    assert.equal(edgeVisibleBoundary, `${snapshot.sidebar.width}px`);
    const edgeHiddenBoundary = await evaluate(`setSidebarEdgeVisible(false); getComputedStyle(document.documentElement).getPropertyValue('--chrome-sidebar-boundary').trim()`);
    assert.equal(edgeHiddenBoundary, "0px");
    const restoredBoundary = await evaluate(`document.querySelector('body > .sidebar-toggle-button').click(); getComputedStyle(document.documentElement).getPropertyValue('--chrome-sidebar-boundary').trim()`);
    assert.equal(restoredBoundary, `${snapshot.sidebar.width}px`);
  }
  assert.equal(snapshot.node, "undefined"); assert.deepEqual(snapshot.bridge, ["getSessions", "switchSession", "subscribeSessions", "nextSessionEvent", "unsubscribeSessions"]); assert.equal(snapshot.windows, 0);
  assert.equal(await evaluate(`window.rozsa.getSessions('/forbidden').then(()=>false,error=>error.message)`), "getSessions accepts no arguments");
  assert.equal(await evaluate(`window.RozsaUIHost.port.request('get_sessions').then(value=>value.length)`), 2);
  await evaluate(`document.querySelector('#sessionList .session-item').click()`);
  const switched = await poll(async () => {
    const state = await evaluate(`({name:document.querySelector('#currentSessionName')?.textContent,chat:document.querySelector('#chatMessages')?.textContent})`);
    return state.name === "Review session catalog" && state.chat.includes("Review session catalog") ? state : undefined;
  }, "real session switch into the conversation view");
  assert.match(switched.chat, /Review session catalog/);
  const screenshot = await send("Page.captureScreenshot", {}, attached.sessionId);
  await writeFile(join(output, "full-ui.png"), Buffer.from(screenshot.data, "base64"));
  assert.equal(await readFile(fixture, "utf8"), fixtureText); assert.equal((await stat(fixture)).mtimeMs, before.mtimeMs);
  await poll(async () => /Session window shown/.test(logs) ? true : undefined, "native Electron window shown");
  // Normal window close travels through window-all-closed and backend shutdown.
  socket.send(JSON.stringify({ id: ++id, method: "Runtime.evaluate", params: { expression: "window.close()" }, sessionId: attached.sessionId }));
  const exitCode = await Promise.race([exited, new Promise<never>((_, reject) => { const timer = setTimeout(() => reject(new Error("Electron close timed out")), 10000); timer.unref(); })]);
  assert.equal(exitCode, 0);
  const backendPID = Number(/Backend started: (\d+)/.exec(logs)?.[1]);
  assert.ok(backendPID > 0); assert.match(logs, new RegExp(`Backend stopped: ${backendPID} \\(0\\)`));
  assert.throws(() => process.kill(backendPID, 0), /ESRCH/);
  closedNormally = true;
  await writeFile(join(output, "result.json"), JSON.stringify({ platform: process.platform, snapshot, windowShown: true, electronExitCode: exitCode, backendPID, backendAbsent: true, fixtureUnchanged: true, screenshot: join(output, "full-ui.png") }, null, 2));
  console.log(`Native session-list PASS: ${output}`);
} finally {
  socket?.close();
  if (!closedNormally && child.exitCode === null && child.signalCode === null) {
    // Only the subprocess owned by this test is addressed.
    child.kill("SIGTERM");
    await Promise.race([exited, new Promise(resolve => setTimeout(resolve, 3000))]);
    if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
  }
  await writeFile(join(output, "electron.log"), logs);
}
