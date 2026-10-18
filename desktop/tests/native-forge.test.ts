// Real Forge start/Vite HMR, only isolated source copies and owned subprocesses.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, writeFile, symlink, rename } from "node:fs/promises";
import { dirname, join, resolve, basename } from "node:path";
import { fileURLToPath } from "node:url";
import { createIsolatedBackendEnvironment } from "./helpers/backend-config.ts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const backendEnvironment = await createIsolatedBackendEnvironment();
const base = join(root, "tmp/migration-native-forge"); await mkdir(base, { recursive: true });
const output = await mkdtemp(join(base, "run-")), fixture = join(output, "project");
await mkdir(fixture);
await cp(join(root, "contracts"), join(fixture, "contracts"), { recursive: true });
await cp(join(root, "desktop"), join(fixture, "desktop"), { recursive: true, filter: path => !["node_modules", ".vite", "out", "dist"].includes(basename(path)) });
await symlink(join(root, "desktop/node_modules"), join(fixture, "desktop/node_modules"), process.platform === "win32" ? "junction" : "dir");
await symlink(join(root, "backend"), join(fixture, "backend"), process.platform === "win32" ? "junction" : "dir");
const sessions = join(output, "sessions"); await mkdir(sessions);
const jsonl = JSON.stringify({ type: "session", version: 3, id: "hmr", timestamp: "t", cwd: "/synthetic" }) + "\n"
  + JSON.stringify({ type: "message", id: "m", parentId: null, timestamp: "t", message: { role: "user", content: "Forge stdio history" } }) + "\n";
await writeFile(join(sessions, "hmr.jsonl"), jsonl);
const cli = join(root, "desktop/node_modules/@electron-forge/cli/dist/electron-forge.js");
let logs = "", normal = false, socket: WebSocket | undefined;
const child = spawn(process.execPath, [cli, "start", "--", "--remote-debugging-port=0", `--user-data-dir=${join(output, "profile")}`, "--session-dir", sessions], {
  cwd: join(fixture, "desktop"), stdio: ["ignore", "pipe", "pipe"],
  env: { ...process.env, ...backendEnvironment, TMPDIR: join(root, "tmp/install-tmp") },
});
const exited = new Promise<number | null>((accept, reject) => { child.once("error", reject); child.once("exit", accept); });
child.stdout.on("data", b => logs += b.toString()); child.stderr.on("data", b => logs += b.toString());
async function poll<T>(get: () => Promise<T | undefined>, label: string): Promise<T> {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const result = await get(); if (result !== undefined) return result;
    if (child.exitCode !== null || child.signalCode !== null) throw new Error(`Forge exited: ${logs}`);
    await new Promise(accept => setTimeout(accept, 100));
  }
  throw new Error(`Timed out: ${label}\n${logs}`);
}
try {
  const url = await poll(async () => /DevTools listening on (ws:\/\/\S+)/.exec(logs)?.[1], "Forge start");
  socket = new WebSocket(url);
  await new Promise<void>((accept, reject) => { socket!.addEventListener("open", () => accept(), { once: true }); socket!.addEventListener("error", reject, { once: true }); });
  let id = 0;
  const pending = new Map<number, { accept(value: any): void; reject(error: Error): void; timer: NodeJS.Timeout }>();
  socket.addEventListener("message", event => {
    const data = JSON.parse(String(event.data));
    const request = pending.get(data.id); if (!request) return;
    pending.delete(data.id); clearTimeout(request.timer);
    if (data.error) request.reject(new Error(JSON.stringify(data.error))); else request.accept(data.result);
  });
  const send = (method: string, params: object = {}, sessionId?: string): Promise<any> => new Promise((accept, reject) => {
    const request = ++id, timer = setTimeout(() => { pending.delete(request); reject(new Error(`DevTools timeout: ${method}`)); }, 5000);
    pending.set(request, { accept, reject, timer }); socket!.send(JSON.stringify({ id: request, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
  const target = await poll(async () => (await send("Target.getTargets")).targetInfos.find((t: { type: string; url: string }) => t.type === "page" && t.url.startsWith("http://127.0.0.1:")), "Vite renderer");
  const { sessionId } = await send("Target.attachToTarget", { targetId: target.targetId, flatten: true });
  const evaluate = async (expression: string) => {
    const response = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, sessionId);
    if (response.exceptionDetails) throw new Error(JSON.stringify(response.exceptionDetails));
    return response.result.value;
  };
  await poll(async () => await evaluate("document.querySelectorAll('#sessionList .session-item').length===1 && !!document.querySelector('#chatMessages')") ? true : undefined, "full UI and stdio catalog");
  await poll(async () => await evaluate("document.body && getComputedStyle(document.body).getPropertyValue('--font-ui').trim().length > 0") ? true : undefined, "shared tokens in development");
  if (process.platform === "darwin") {
    const titlebarLayout = await evaluate(`(() => {
      const toggle = document.querySelector('body > .sidebar-toggle-button');
      const header = document.querySelector('[data-od-id="panel-header"]').getBoundingClientRect();
      const sidebar = document.querySelector('[data-od-id="sidebar"]');
      const sidebarHeader = sidebar.querySelector('[data-od-id="sidebar-sessions"] > .sidebar-header').getBoundingClientRect();
      const shadow = getComputedStyle(sidebar).boxShadow;
      const headerStyle = getComputedStyle(document.querySelector('[data-od-id="panel-header"]'));
      return {
        noDuplicateChromeContainer: !document.querySelector('#native-titlebar'),
        sidebarToggleVisible: !!toggle && getComputedStyle(toggle).display !== 'none',
        sidebarToggleSharesMainHeaderRow: !!toggle && toggle.getBoundingClientRect().top === header.top,
        sidebarHeaderStartsWithWindow: sidebarHeader.top === 0,
        sidebarHasSoftCastShadow: !shadow.includes('inset') && shadow.includes('px'),
        mainHeaderKeepsGlassTreatment: headerStyle.backdropFilter !== 'none' && headerStyle.boxShadow !== 'none',
      };
    })()`);
    assert.deepEqual(titlebarLayout, {
      noDuplicateChromeContainer: true,
      sidebarToggleVisible: true,
      sidebarToggleSharesMainHeaderRow: true,
      sidebarHeaderStartsWithWindow: true,
      sidebarHasSoftCastShadow: true,
      mainHeaderKeepsGlassTreatment: true,
    });
    const sidebarToggleRoundTrip = await evaluate(`(() => {
      const button = document.querySelector('body > .sidebar-toggle-button');
      const initiallyCollapsed = document.body.classList.contains('sidebar-collapsed');
      button.click();
      const collapsedAfterFirstClick = document.body.classList.contains('sidebar-collapsed');
      const firstLabel = button.getAttribute('aria-label');
      button.click();
      return {
        initiallyCollapsed,
        collapsedAfterFirstClick,
        restoredAfterSecondClick: document.body.classList.contains('sidebar-collapsed') === initiallyCollapsed,
        restoredLabel: button.getAttribute('aria-label') === (initiallyCollapsed ? 'Show sidebar' : 'Hide sidebar'),
        firstLabel,
      };
    })()`);
    assert.equal(sidebarToggleRoundTrip.collapsedAfterFirstClick, !sidebarToggleRoundTrip.initiallyCollapsed);
    assert.equal(sidebarToggleRoundTrip.restoredAfterSecondClick, true);
    assert.equal(sidebarToggleRoundTrip.restoredLabel, true);
    const surfaces = await evaluate(`(() => {
      document.documentElement.setAttribute('data-theme-translucent-sidebar', 'true');
      return {
      enabled: document.documentElement.getAttribute('data-theme-translucent-sidebar'),
      host: document.body.className,
      sidebar: getComputedStyle(document.querySelector('[data-od-id="sidebar"]')).backgroundColor,
      main: getComputedStyle(document.querySelector('[data-od-id="main-panel"]')).backgroundColor,
      sidebarFilter: getComputedStyle(document.querySelector('[data-od-id="sidebar"]')).backdropFilter,
      };
    })()`);
    assert.equal(surfaces.enabled, "true");
    assert.match(surfaces.host, /macos-host/);
    assert.match(surfaces.sidebar, /rgba?\(0, 0, 0, 0\)/);
    assert.doesNotMatch(surfaces.main, /rgba?\(0, 0, 0, 0\)/);
    assert.equal(surfaces.sidebarFilter, "none");
    const opaque = await evaluate(`(() => {
      document.documentElement.setAttribute('data-theme-translucent-sidebar', 'false');
      return {
        sidebar: getComputedStyle(document.querySelector('[data-od-id="sidebar"]')).backgroundColor,
      };
    })()`);
    assert.doesNotMatch(opaque.sidebar, /rgba?\(0, 0, 0, 0\)/);
  }
  assert.deepEqual(await evaluate("({node:typeof window.require,bridge:Object.keys(window.rozsa)})"), { node: "undefined", bridge: ["getSessions", "switchSession", "subscribeSessions", "nextSessionEvent", "unsubscribeSessions", "startMessage", "continueMessage", "getActiveMessage", "subscribeMessageEvents", "nextMessageEvent", "unsubscribeMessageEvents", "cancelMessage"] });
  await evaluate("document.querySelector('#sessionList .session-item').click()");
  await poll(async () => await evaluate("document.querySelector('#chatMessages')?.textContent.includes('Forge stdio history')") ? true : undefined, "conversation history");
  const backendPID = Number(/Backend started: (\d+)/.exec(logs)?.[1]); assert.ok(backendPID > 0);
  const shot = async (name: string) => { const image = await send("Page.captureScreenshot", {}, sessionId); await writeFile(join(output, name), Buffer.from(image.data, "base64")); };
  await shot("before.png");
  await evaluate("window.__hmrIdentity='retained'");
  const css = join(fixture, "desktop/renderer/styles/legacy/main.css");
  await writeFile(css + ".save", (await readFile(css, "utf8")) + "\nbody { background-color: rgb(12, 34, 56) !important; }\n"); await rename(css + ".save", css);
  await poll(async () => await evaluate("getComputedStyle(document.body).backgroundColor==='rgb(12, 34, 56)'") ? true : undefined, "shared CSS HMR");
  assert.equal(await evaluate("window.__hmrIdentity"), "retained"); // CSS updated without a renderer reload.
  assert.match(await evaluate("document.querySelector('#chatMessages').textContent"), /Forge stdio history/);
  const ts = join(fixture, "desktop/renderer/runtime/entries/main.ts");
  await writeFile(ts + ".save", (await readFile(ts, "utf8")) + "\ndocument.body.dataset.forgeTsUpdate = 'updated';\n"); await rename(ts + ".save", ts);
  await poll(async () => await evaluate("document.body?.dataset.forgeTsUpdate==='updated'") ? true : undefined, "runtime TS automatic update");
  await poll(async () => await evaluate("document.querySelectorAll('#sessionList .session-item').length===1") ? true : undefined, "catalog after TS update");
  const html = join(fixture, "desktop/renderer/index.html");
  await writeFile(html + ".save", (await readFile(html, "utf8")).replace('id="mainContentScene"', 'id="mainContentScene" data-forge-refresh="updated"')); await rename(html + ".save", html);
  await poll(async () => await evaluate("document.querySelector('#mainContentScene')?.dataset.forgeRefresh==='updated'") ? true : undefined, "assembled HTML automatic update");
  await shot("after.png");
  const trustedURL = await evaluate("location.href");
  await evaluate("location.href='/untrusted-navigation'");
  await new Promise(accept => setTimeout(accept, 200));
  assert.equal(await evaluate("location.href"), trustedURL);
  assert.equal([...logs.matchAll(/Backend started:/g)].length, 1, "Renderer hot updates must not restart Go");
  assert.equal(await readFile(join(sessions, "hmr.jsonl"), "utf8"), jsonl);
  const workerPIDs = await send("SystemInfo.getProcessInfo");
  socket.send(JSON.stringify({ id: ++id, method: "Runtime.evaluate", params: { expression: "window.close()" }, sessionId }));
  const code = await Promise.race([exited, new Promise<never>((_, reject) => { const timer = setTimeout(() => reject(new Error("Forge quit timeout")), 10000); timer.unref(); })]);
  assert.equal(code, 0); assert.match(logs, new RegExp(`Backend stopped: ${backendPID} \\(0\\)`));
  assert.throws(() => process.kill(backendPID, 0), /ESRCH/);
  for (const info of workerPIDs.processInfo) {
    await poll(async () => { try { process.kill(info.id, 0); return undefined; } catch (error) { if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error; return true; } }, `Electron process ${info.id} exit`);
  }
  await assert.rejects(fetch(target.url)); // Forge closed the Vite dev server.
  normal = true;
  await writeFile(join(output, "result.json"), JSON.stringify({ platform: process.platform, target: target.url, cssHMR: true, cssRetainedWindow: true, tsAutomaticUpdate: true, htmlAutomaticUpdate: true, stdioHistory: true, fixtureUnchanged: true, backendPID, backendAbsent: true, electronProcessesAbsent: true, forgeExitCode: code, viteServerClosed: true }, null, 2));
  console.log(`Native Forge HMR PASS: ${output}`);
} finally {
  socket?.close();
  if (!normal && child.exitCode === null && child.signalCode === null) {
    child.kill("SIGINT");
    await Promise.race([exited, new Promise(accept => setTimeout(accept, 3000))]);
  }
  if (!normal) {
    for (const match of logs.matchAll(/Backend started: (\d+)/g)) {
      try { process.kill(Number(match[1]), "SIGTERM"); } catch (error) { if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error; }
    }
  }
  await writeFile(join(output, "forge.log"), logs);
}
