import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdir, mkdtemp, writeFile, chmod, readFile, cp, stat } from "node:fs/promises";
import { spawn } from "node:child_process";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { BackendService } from "../main/backend-service.ts";
import { ProductFailure } from "../main/session-contract.ts";
import { RPCFailure } from "../main/backend-service.ts";
import { RozsaBridge } from "../preload/bridge.ts";
import { ProductClient } from "../renderer/product-client.ts";
import { createIsolatedBackendEnvironment } from "./helpers/backend-config.ts";
import { assertProductBridgeCapabilityKeys } from "./helpers/bridge-capabilities.ts";

const root = resolve(import.meta.dirname, "../..");
const binary = join(root, "dist", process.platform === "win32" ? "rozsa-backend.exe" : "rozsa-backend");
const backendEnvironment = await createIsolatedBackendEnvironment();
const capabilities = ["get_sessions", "switch_session", "subscribe_sessions", "next_session_event", "unsubscribe_sessions", "runtime_status", "start_message", "continue_message", "subscribe_message_events", "next_message_event", "unsubscribe_message_events", "cancel_message", "get_interruption_state", "respond_permission", "respond_user_question", "cancel_user_question", "get_settings", "autocomplete_input", "update_setting", "reset_setting", "list_themes", "get_theme", "save_theme"];
async function poll(check: () => boolean | Promise<boolean>): Promise<void> {
  const deadline = Date.now() + 6000;
  while (Date.now() < deadline) { if (await check()) return; await delay(20); }
  assert.fail("Lifecycle condition timed out");
}
function absent(pid: number): boolean {
  try { process.kill(pid, 0); return false; }
  catch (error) { if ((error as NodeJS.ErrnoException).code === "ESRCH") return true; throw error; }
}
async function endpoint(body: string) {
  const base = join(root, "tmp/migration-lifecycle"); await mkdir(base, { recursive: true });
  const directory = await mkdtemp(join(base, "run-")), executable = join(directory, "backend.cjs"), calls = join(directory, "calls.jsonl");
  await writeFile(executable, [
    `#!${process.execPath}`,
    `const {createMessageConnection,ResponseError}=require(${JSON.stringify(join(root, "desktop/node_modules/vscode-jsonrpc/lib/node/main.js"))});`,
    `const {appendFileSync}=require('node:fs'); const calls=${JSON.stringify(calls)};`,
    "const conn=createMessageConnection(process.stdin,process.stdout);",
    `conn.onRequest('initialize',()=>({protocolVersion:1,capabilities:${JSON.stringify(capabilities)}}));`,
    "conn.onRequest('runtime_status',()=>({state:'ready'}));",
    "conn.onRequest('shutdown',()=>{appendFileSync(calls,JSON.stringify('shutdown')+'\\n');setTimeout(()=>process.exit(0),30);return true;});",
    body, "conn.listen();", "",
  ].join("\n"));
  await chmod(executable, 0o700);
  return { executable, calls, directory };
}

test("fatal handshake mismatch reaps its live owned process without waiting for window close", { skip: process.platform === "win32" }, async () => {
  const fake = await endpoint("conn.onRequest('initialize',()=>({protocolVersion:99,capabilities:[]}));");
  const backend = new BackendService(fake.executable, []);
  try {
    await assert.rejects(backend.initialize(), /Incompatible backend handshake/);
    await poll(() => absent(backend.process.pid!));
  } finally { await backend.stop(); }
});

test("product catalog calls wait for the initialize handshake and run afterward", { skip: process.platform === "win32" }, async () => {
  const fake = await endpoint([
    `conn.onRequest('initialize',()=>{appendFileSync(calls,JSON.stringify('initialize')+'\\n');return new Promise(resolve=>setTimeout(()=>resolve({protocolVersion:1,capabilities:${JSON.stringify(capabilities)}}),250));});`,
    "conn.onRequest('get_sessions',()=>{appendFileSync(calls,JSON.stringify('get_sessions')+'\\n');return [];});",
  ].join("\n"));
  const backend = new BackendService(fake.executable, []);
  try {
    const initialized = backend.initialize();
    const catalog = backend.getSessions();
    assert.deepEqual(await catalog, []);
    await initialized;
    assert.equal(await readFile(fake.calls, "utf8"), '"initialize"\n"get_sessions"\n');
  } finally { await backend.stop(); }
});

test("runtime initialization failure is reported before any message is sent while catalog stays usable", { skip: process.platform === "win32" }, async () => {
  const fake = await endpoint([
    "let statusReads=0;",
    "conn.onRequest('runtime_status',()=>{statusReads++;if(statusReads===1)return {state:'initializing'};throw new ResponseError(-32009,'model credentials are missing',{kind:'runtime_initialization'});});",
    "conn.onRequest('get_sessions',()=>[]);",
    "conn.onRequest('start_message',()=>{appendFileSync(calls,JSON.stringify('start_message')+'\\n');return {operationId:'unexpected'};});",
  ].join("\n"));
  const backend = new BackendService(fake.executable, []);
  try {
    await backend.initialize();
    assert.deepEqual(await backend.getSessions(), []);
    await assert.rejects(backend.waitForRuntime(), error => error instanceof RPCFailure && error.remote.kind === "runtime_initialization" && error.message === "model credentials are missing");
    assert.equal(await readFile(fake.calls, "utf8").catch(() => ""), "");
  } finally { await backend.stop(); }
});

test("fatal request timeout reaps its process and never replays accepted work", { skip: process.platform === "win32" }, async () => {
  const fake = await endpoint("conn.onRequest('get_sessions',()=>{appendFileSync(calls,JSON.stringify('get_sessions')+'\\n');return new Promise(()=>{});});");
  const backend = new BackendService(fake.executable, [], { requestTimeoutMs: 5000 });
  try {
    await backend.initialize();
    await assert.rejects(backend.getSessions(), error => error instanceof ProductFailure && error.failure.kind === "transport" && error.failure.reason === "timeout");
    await poll(() => absent(backend.process.pid!));
    await assert.rejects(backend.getSessions());
    assert.equal(await readFile(fake.calls, "utf8"), '"get_sessions"\n');
  } finally { await backend.stop(); }
});

test("failed spawn and backend crash reject explicitly and leave no owned process", async () => {
  const missing = new BackendService(join(root, "tmp/absent-backend-T012"), []);
  await assert.rejects(missing.initialize(), /ENOENT/); await missing.stop();
  const backend = new BackendService(binary, [], { env: backendEnvironment });
  try {
    await backend.initialize();
    const subscribed = await backend.subscribeSessions();
    const receiving = backend.nextSessionEvent(subscribed.subscriptionId);
    const rejected = assert.rejects(receiving, error => error instanceof ProductFailure && error.failure.kind === "transport");
    backend.process.kill("SIGKILL"); await rejected;
    await poll(() => absent(backend.process.pid!));
    await assert.rejects(backend.getSessions());
  } finally { await backend.stop(); }
});

test("real Go subscription survives idle, reload restores active snapshot without switching or writing again", async () => {
  const base = join(root, "tmp/migration-lifecycle"); await mkdir(base, { recursive: true });
  const directory = await mkdtemp(join(base, "sessions-")), path = join(directory, "a.jsonl");
  const original = JSON.stringify({ type: "session", version: 3, id: "header-a", timestamp: "t", cwd: "/synthetic" }) + "\n"
    + JSON.stringify({ type: "message", id: "entry", parentId: null, timestamp: "t", message: { role: "user", content: "Hello" } }) + "\n";
  await writeFile(path, original);
  const backend = new BackendService(binary, [directory], { requestTimeoutMs: 500, env: backendEnvironment });
  const calls: string[] = [];
  const bridge = RozsaBridge.create(async (channel, input) => {
    calls.push(channel);
    if (channel === "product:get-sessions") return { ok: true, sessions: await backend.getSessions() };
    if (channel === "product:switch-session") return { ok: true, snapshot: await backend.switchSession(input as any) };
    if (channel === "product:subscribe-sessions") return { ok: true, update: await backend.subscribeSessions() };
    if (channel === "product:next-session-event") {
      try { return { ok: true, update: await backend.nextSessionEvent(input as string) }; }
      catch (error) { if (error instanceof RPCFailure) return { ok: false, error: error.remote }; throw error; }
    }
    await backend.unsubscribeSessions(input as string); return { ok: true };
  });
  try {
    await backend.initialize();
    const client = new ProductClient(bridge), signal = new AbortController(), received: any[] = [], failures: unknown[] = [];
    const sync = client.syncSessions(value => received.push(value), error => failures.push(error), { signal: signal.signal });
    await poll(() => received.length === 1); assert.equal(received[0], null);
    await delay(650); assert.equal(failures.length, 0); assert.equal(absent(backend.process.pid!), false);
    await client.switchSession({ path, catalogId: "header-a" });
    await poll(() => received.length === 2); assert.equal(received[1].messages[0].entryId, "entry");
    signal.abort("reload"); await sync;
    const reloaded: any[] = [], reloadSignal = new AbortController();
    const reload = new ProductClient(bridge).syncSessions(value => { reloaded.push(value); reloadSignal.abort(); }, error => failures.push(error), { signal: reloadSignal.signal });
    await reload;
    assert.deepEqual(reloaded, [received[1]]); assert.equal(failures.length, 0);
    assert.equal(calls.filter(value => value === "product:switch-session").length, 1);
    assert.equal(await readFile(path, "utf8"), original);
    const subscription = await backend.subscribeSessions();
    const pending = backend.nextSessionEvent(subscription.subscriptionId);
    const rejected = assert.rejects(pending, error => error instanceof RPCFailure && error.remote.kind === "closing");
    const stopped = backend.stop(); assert.equal(backend.stop(), stopped); await rejected; await stopped;
    assert.ok(absent(backend.process.pid!));
  } finally { await backend.stop(); }
});

test("pipe loss and unresponsive shutdown terminate and reap the owned process", { skip: process.platform === "win32" }, async () => {
  for (const mode of ["pipe", "shutdown"]) {
    const fake = await endpoint(mode === "pipe"
      ? "conn.onRequest('get_sessions',()=>{process.stdout.end();setInterval(()=>{},1000);return new Promise(()=>{});});"
      : "conn.onRequest('shutdown',()=>{appendFileSync(calls,JSON.stringify('shutdown')+'\\n');setInterval(()=>{},1000);return new Promise(()=>{});});");
    const backend = new BackendService(fake.executable, []);
    try {
      await backend.initialize();
      if (mode === "pipe") {
        await assert.rejects(backend.getSessions(), error => error instanceof ProductFailure && error.failure.kind === "transport" && error.failure.reason === "closed");
        await poll(() => absent(backend.process.pid!));
      } else await backend.stop();
      assert.ok(absent(backend.process.pid!));
      assert.equal(backend.process.signalCode, "SIGKILL");
      if (mode === "shutdown") assert.equal(await readFile(fake.calls, "utf8"), '"shutdown"\n');
    } finally { await backend.stop(); }
  }
});

// Explicit native acceptance uses a generated copy of Forge's candidate output.
// Only the test bootstrap suppresses the modal startup error box; the actual
// bundled entry, preload, renderer and owned subprocess lifecycle all run.
if (process.argv.includes("--native") || process.env.ROZSA_LIFECYCLE_NATIVE === "1") test("actual Electron candidate lifecycle and runtime security boundaries", { timeout: 150000 }, async () => {
  const electron = join(root, "desktop/node_modules/electron/dist", (await readFile(join(root, "desktop/node_modules/electron/path.txt"), "utf8")).trim());
  const modes = ["normal", "crash", "pipe", "version", "missing", "handshake-timeout", "shutdown-timeout", "quit-startup"];
  const requestedMode = process.env.ROZSA_LIFECYCLE_NATIVE_MODE;
  if (requestedMode && !modes.includes(requestedMode)) throw new Error(`Unknown native lifecycle mode: ${requestedMode}`);
  for (const mode of requestedMode ? [requestedMode] : modes) {
    const base = join(root, "tmp/migration-lifecycle-native"); await mkdir(base, { recursive: true });
    const directory = await mkdtemp(join(base, `${mode}-`)), app = join(directory, "desktop"), backendDirectory = join(directory, "dist");
    await mkdir(app); await mkdir(backendDirectory, { recursive: true });
    await cp(join(root, "desktop/.vite"), join(app, ".vite"), { recursive: true });
    await writeFile(join(app, "package.json"), JSON.stringify({ name: "rozsa-migration-native-test", main: ".vite/build/acceptance.cjs" }));
    await writeFile(join(app, ".vite/build/acceptance.cjs"), [
      "globalThis.candidateElectron=require('electron');",
      "candidateElectron.dialog.showErrorBox=(title,message)=>console.error('Native startup error:',title,message);",
      "const childProcess=require('node:child_process'),spawn=childProcess.spawn;childProcess.spawn=(...args)=>{const child=spawn(...args);if(String(args[0]).includes('rozsa-backend'))globalThis.candidateOwnedBackend=child;return child;};",
      "const ipcMain=candidateElectron.ipcMain,handle=ipcMain.handle.bind(ipcMain),pending=new Map();let requestId=0;globalThis.lifecycleIpcTraceEnabled=false;globalThis.lifecycleIpcTraceTarget=0;ipcMain.handle=function(channel,listener){return handle(channel,async(event,...args)=>{const id=++requestId,senderId=event.sender.id,started=Date.now(),frame=event.senderFrame?.url;pending.set(id,{senderId,channel});const active=()=>[...pending.entries()].filter(([,item])=>item.senderId===senderId).map(([requestId,item])=>({id:requestId,channel:item.channel}));if(globalThis.lifecycleIpcTraceEnabled&&senderId===globalThis.lifecycleIpcTraceTarget)console.error('LIFECYCLE_IPC_START',JSON.stringify({id,channel,senderId,frame,inFlight:active().length,open:active()}));try{const result=await listener(event,...args);if(globalThis.lifecycleIpcTraceEnabled&&senderId===globalThis.lifecycleIpcTraceTarget)console.error('LIFECYCLE_IPC_END',JSON.stringify({id,channel,senderId,frame,durationMs:Date.now()-started,inFlight:active().length-1,open:active().filter(item=>item.id!==id),failure:result?.ok===false?result.error:undefined}));return result;}catch(error){if(globalThis.lifecycleIpcTraceEnabled&&senderId===globalThis.lifecycleIpcTraceTarget)console.error('LIFECYCLE_IPC_THROW',JSON.stringify({id,channel,senderId,frame,durationMs:Date.now()-started,inFlight:active().length-1,open:active().filter(item=>item.id!==id),error:String(error)}));throw error;}finally{pending.delete(id);}});};",
      "candidateElectron.app.on('browser-window-created',(_event,window)=>{if(!globalThis.lifecycleIpcTraceTarget){globalThis.lifecycleIpcTraceTarget=window.webContents.id;globalThis.lifecycleIpcTraceEnabled=true;}});",
      "require('./main.cjs');", "",
    ].join("\n"));
    let source = binary;
    if (["version", "handshake-timeout", "shutdown-timeout", "quit-startup"].includes(mode)) {
      const fake = await endpoint(mode === "version" ? "conn.onRequest('initialize',()=>({protocolVersion:99,capabilities:[]}));"
        : mode === "handshake-timeout" || mode === "quit-startup" ? "conn.onRequest('initialize',()=>{setInterval(()=>{},1000);return new Promise(()=>{});});"
        : [
          "conn.onRequest('get_sessions',()=>[]);",
          "conn.onRequest('subscribe_sessions',()=>({subscriptionId:'1',snapshot:{epoch:'test',watermark:0,active:null}}));",
          "conn.onRequest('next_session_event',()=>new Promise(()=>{}));",
          "conn.onRequest('unsubscribe_sessions',()=>true);",
          "conn.onRequest('shutdown',()=>{setInterval(()=>{},1000);return new Promise(()=>{});});",
        ].join("\n"));
      source = fake.executable;
    }
    if (mode !== "missing") await cp(source, join(backendDirectory, process.platform === "win32" ? "rozsa-backend.exe" : "rozsa-backend"));
    const sessions = join(directory, "sessions"); await mkdir(sessions);
    const path = join(sessions, "native.jsonl"), original = JSON.stringify({ type: "session", version: 3, id: "native", timestamp: "t", cwd: "/synthetic" }) + "\n"
      + JSON.stringify({ type: "message", id: "entry", parentId: null, timestamp: "t", message: { role: "user", content: 'Native lifecycle history marker <img src=x onerror="window.pwned=true"><script>window.pwned=true</script>' } }) + "\n";
    await writeFile(path, original); const before = await stat(path);
    let logs = "";
    const child = spawn(electron, [app, "--remote-debugging-port=0", "--inspect=0", `--user-data-dir=${join(directory, "profile")}`, "--session-dir", sessions], { cwd: directory, env: { ...process.env, ...backendEnvironment }, stdio: ["ignore", "pipe", "pipe"] });
    const exited = new Promise<number | null>((resolve, reject) => { child.once("error", reject); child.once("exit", resolve); });
    child.stdout.on("data", (value: Buffer) => logs += value.toString()); child.stderr.on("data", (value: Buffer) => logs += value.toString());
    const sockets: WebSocket[] = [];
    const wait = async <T>(read: () => Promise<T | undefined>, label: string): Promise<T> => {
      const deadline = Date.now() + 20000;
      while (Date.now() < deadline) {
        const value = await read(); if (value !== undefined) return value;
        if (child.exitCode !== null || child.signalCode !== null) throw new Error(`${mode} exited during ${label}: ${logs}`);
        await delay(50);
      }
      throw new Error(`${mode} timeout: ${label}\n${logs}`);
    };
    const connect = async (url: string) => {
      const socket = new WebSocket(url); sockets.push(socket);
      await new Promise<void>((resolve, reject) => { socket.addEventListener("open", () => resolve(), { once: true }); socket.addEventListener("error", reject, { once: true }); });
      let serial = 0;
      const protocolEvents: Array<{ method: string; sessionId?: string; params: unknown }> = [];
      const pending = new Map<number, { resolve(value: any): void; reject(error: Error): void; timer: NodeJS.Timeout }>();
      socket.addEventListener("message", event => {
        const value = JSON.parse(String(event.data)), item = pending.get(value.id);
        if (typeof value.method === "string") protocolEvents.push({ method: value.method, sessionId: value.sessionId, params: value.params });
        if (!item) return;
        pending.delete(value.id); clearTimeout(item.timer);
        if (value.error) item.reject(new Error(JSON.stringify(value.error))); else item.resolve(value.result);
      });
      socket.addEventListener("close", () => { for (const item of pending.values()) { clearTimeout(item.timer); item.reject(new Error("DevTools closed")); } pending.clear(); });
      const send = (method: string, params: object = {}, sessionId?: string): Promise<any> => new Promise((resolve, reject) => {
        const id = ++serial, timer = setTimeout(() => { pending.delete(id); reject(new Error(`DevTools timeout: ${method}`)); }, 5000);
        pending.set(id, { resolve, reject, timer }); socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
      });
      const evaluate = async (expression: string, sessionId?: string): Promise<any> => {
        const result = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, sessionId);
        if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails)); return result.result.value;
      };
      return { send, evaluate, socket, protocolEvents };
    };
    const finish = async () => {
      const timer = setTimeout(() => { child.kill("SIGKILL"); }, 18000);
      try { assert.equal(await exited, 0, `${mode}: ${logs}`); } finally { clearTimeout(timer); }
    };
    try {
      if (["version", "missing", "handshake-timeout"].includes(mode)) {
        await finish();
        assert.match(logs, /Native startup error:/);
        assert.match(logs, mode === "version" ? /Incompatible backend handshake/ : mode === "missing" ? /ENOENT/ : /Backend request timed out: initialize/);
      } else if (mode === "quit-startup") {
        await wait(async () => /Backend started: (\d+)/.test(logs) ? true : undefined, "owned process startup");
        child.kill("SIGTERM"); await finish(); assert.doesNotMatch(logs, /Session window shown/);
      } else {
        const inspectorURL = await wait(async () => /Debugger listening on (ws:\/\/\S+)/.exec(logs)?.[1], "main inspector");
        const main = await connect(inspectorURL);
        const browserURL = await wait(async () => /DevTools listening on (ws:\/\/\S+)/.exec(logs)?.[1], "renderer DevTools");
        const browser = await connect(browserURL);
        const target = await wait(async () => (await browser.send("Target.getTargets")).targetInfos.find((item: any) => item.type === "page" && item.url.endsWith("/index.html")), "current product page");
        const attached = await browser.send("Target.attachToTarget", { targetId: target.targetId, flatten: true });
        let rendererSessionId = attached.sessionId;
        const evaluate = (expression: string) => browser.evaluate(expression, rendererSessionId);
        await browser.send("Page.enable", {}, rendererSessionId);
        await browser.send("Runtime.enable", {}, rendererSessionId);
        const expectedRows = mode === "shutdown-timeout" ? 0 : 1;
        await wait(async () => await evaluate(`document.querySelectorAll('#sessionList .session-item').length===${expectedRows} && !!document.querySelector('#chatMessages')`) ? true : undefined, "current product session catalog");
        const pid = Number(/Backend started: (\d+)/.exec(logs)?.[1]); assert.ok(pid > 0);
        const window = "candidateElectron.BrowserWindow.getAllWindows()[0]";
        const preferences = await main.evaluate(`(()=>{const p=${window}.webContents.getLastWebPreferences();return {contextIsolation:p.contextIsolation,sandbox:p.sandbox,nodeIntegration:p.nodeIntegration};})()`);
        assert.deepEqual(preferences, { contextIsolation: true, sandbox: true, nodeIntegration: false });
        const boundary = await evaluate("({require:typeof require,process:typeof process,Buffer:typeof Buffer,keys:Object.keys(window.rozsa)})");
        assert.deepEqual({ require: boundary.require, process: boundary.process, Buffer: boundary.Buffer }, { require: "undefined", process: "undefined", Buffer: "undefined" });
        assertProductBridgeCapabilityKeys(boundary.keys, { events: true });
        if (mode !== "shutdown-timeout") {
          await evaluate("document.querySelector('#sessionList .session-item').click()");
          await wait(async () => await evaluate("document.querySelector('#chatMessages')?.textContent.includes('Native lifecycle history marker')") ? true : undefined, "current conversation history");
        }
        if (mode === "normal") {
          assert.equal(await evaluate("document.querySelector('#chatMessages').querySelectorAll('img,script').length"), 0);
          assert.match(await evaluate("document.querySelector('#chatMessages').textContent"), /Native lifecycle history marker/);
          assert.equal(await evaluate("window.pwned===true"), false, "untrusted session markup must not execute in the current conversation UI");
          await evaluate("window.__blockedScriptRan=false;window.__cspViolations=[];window.addEventListener('securitypolicyviolation',event=>window.__cspViolations.push({directive:event.effectiveDirective,blockedURI:event.blockedURI}),{once:true});const script=document.createElement('script');script.src='https://example.com/blocked-csp.js';script.onload=()=>window.__blockedScriptRan=true;document.head.append(script)");
          await wait(async () => await evaluate("window.__cspViolations.length===1") ? true : undefined, "external script blocked by the current CSP");
          const cspViolation = await evaluate("window.__cspViolations[0]");
          assert.match(cspViolation.directive, /^script-src/);
          assert.match(cspViolation.blockedURI, /example\.com/);
          assert.equal(await evaluate("window.__blockedScriptRan"), false);
          assert.equal(await evaluate("fetch('https://example.com/').then(()=>false,()=>true)"), true);
          assert.equal(await evaluate("window.open('https://example.com/')===null"), true);
          const url = await evaluate("location.href");
          await evaluate("location.href='https://example.com/'"); await delay(100); assert.equal(await evaluate("location.href"), url);
          // A real second renderer uses the compiled preload but has no authority.
          const denied = await main.evaluate(`(async()=>{const owner=${window};const other=new candidateElectron.BrowserWindow({show:false,webPreferences:{preload:${JSON.stringify(join(app, ".vite/build/preload.cjs"))},contextIsolation:true,sandbox:true,nodeIntegration:false}});try{await other.loadURL(owner.webContents.getURL());return await other.webContents.executeJavaScript("window.rozsa.getSessions().then(()=>false,error=>error.message)");}finally{other.destroy();}})()`);
          assert.match(denied, /Unauthorized session request/);
          for (let reload = 0; reload < 2; reload++) {
            const previousTimeOrigin = await evaluate("performance.timeOrigin");
            await main.evaluate(`new Promise((resolve,reject)=>{const contents=${window}.webContents;const timer=setTimeout(()=>reject(new Error('reload did not finish')),20000);contents.once('did-finish-load',()=>{clearTimeout(timer);resolve(true);});contents.reload();})`);
            const reloadedTarget = await wait(async () => (await browser.send("Target.getTargets")).targetInfos.find((item: any) => item.targetId === target.targetId && item.type === "page" && item.url.endsWith("/index.html")), "same product renderer target after reload");
            rendererSessionId = (await browser.send("Target.attachToTarget", { targetId: reloadedTarget.targetId, flatten: true })).sessionId;
            await browser.send("Page.enable", {}, rendererSessionId);
            await browser.send("Runtime.enable", {}, rendererSessionId);
            await wait(async () => {
              try { return await evaluate("document.querySelectorAll('#sessionList .session-item').length===1 && document.querySelector('#chatMessages')?.textContent.includes('Native lifecycle history marker')") ? true : undefined; }
              catch (error) { if (/context|navigat/i.test(String(error))) return undefined; throw error; }
            }, "reload snapshot");
            assert.notEqual(await evaluate("performance.timeOrigin"), previousTimeOrigin, "reload must inspect a new document");
          }
          // Wait for Electron to report the renderer exit before requesting a
          // reload, then wait for that navigation's completed document.
          const originalDocument = await evaluate("({timeOrigin:performance.timeOrigin,url:location.href})");
          const crashDetails = await main.evaluate(`(async()=>{const contents=${window}.webContents;const gone=new Promise(resolve=>contents.once('render-process-gone',(_event,details)=>resolve(details)));contents.forcefullyCrashRenderer();return await gone;})()`);
          const reloadResult = await main.evaluate(`new Promise((resolve,reject)=>{const contents=${window}.webContents;const timer=setTimeout(()=>reject(new Error('reload did not finish')),20000);contents.once('did-finish-load',()=>{clearTimeout(timer);resolve({url:contents.getURL(),processId:contents.getProcessId()});});contents.reload();})`);
          const recoveredTarget = await wait(async () => (await browser.send("Target.getTargets")).targetInfos.find((item: any) => item.targetId === target.targetId && item.type === "page" && item.url.endsWith("/index.html")), "same product renderer target after crash");
          rendererSessionId = (await browser.send("Target.attachToTarget", { targetId: recoveredTarget.targetId, flatten: true })).sessionId;
          await browser.send("Page.enable", {}, rendererSessionId);
          await browser.send("Runtime.enable", {}, rendererSessionId);
          const recoveryDiagnostics = "(async()=>{const requests=await Promise.allSettled([Promise.resolve().then(()=>window.rozsa.getSessions()),Promise.resolve().then(()=>window.RozsaUIHost.port.request('get_state'))]);return {ready:document.readyState,rows:document.querySelectorAll('#sessionList .session-item').length,chat:document.querySelector('#chatMessages')?.textContent||'',frame:location.href,context:performance.timeOrigin,requests:requests.map(value=>value.status==='fulfilled'?{status:value.status,value:value.value}:{status:value.status,error:String(value.reason)})};})()";
          const recoveredView = await wait(async () => {
            const value = await evaluate("({ready:document.readyState,sidebar:!!document.querySelector('#sessionList'),conversation:!!document.querySelector('#chatMessages'),rows:document.querySelectorAll('#sessionList .session-item').length,chat:document.querySelector('#chatMessages')?.textContent||''})");
            return value.ready === "complete" && value.sidebar && value.conversation
              && value.rows === 1 && value.chat.includes("Native lifecycle history marker") ? value : undefined;
          }, "session catalog and history restored after renderer crash").catch(async error => {
            const state = await evaluate(recoveryDiagnostics);
            const requestSummary = state.requests.map((request: any, index: number) => request.status === "fulfilled"
              ? { index, status: request.status, keys: Object.keys(request.value), sessionIds: request.value.sessions?.map((session: any) => session.id), activeSessionId: request.value.sessionId, historyMarker: JSON.stringify(request.value).includes("Native lifecycle history marker") }
              : { index, status: request.status, error: request.error });
            const requestTrace = logs.split("\n").filter(line => line.startsWith("LIFECYCLE_IPC_")).join("\n");
            const rendererFailures = logs.split("\n").filter(line => /get_sessions failed|get_state failed|Concurrent request capacity reached/.test(line)).slice(-20).join("\n");
            const protocolEvents = [...browser.protocolEvents, ...main.protocolEvents];
            const eventSummary = protocolEvents.filter(event => /Target\.(targetCrashed|targetCreated|targetDestroyed)|Page\.(frameStartedLoading|frameNavigated|loadEventFired)|Runtime\.(executionContextCreated|consoleAPICalled|exceptionThrown)/.test(event.method)).map(event => {
              if (event.method !== "Runtime.consoleAPICalled") return { method: event.method, sessionId: event.sessionId, params: event.params };
              const params = event.params as { type?: string; args?: Array<{ value?: unknown; description?: string }> };
              const message = params.args?.map(argument => argument.value ?? argument.description ?? "").join(" ");
              return /get_sessions failed|get_state failed|Concurrent request capacity reached/.test(message || "")
                ? { method: event.method, sessionId: event.sessionId, type: params.type, message: message?.slice(0, 500) } : undefined;
            }).filter(Boolean).slice(-20);
            throw new Error(`${String(error).split("\n")[0]}\nCrash event: ${JSON.stringify(crashDetails)}\nReload completion: ${JSON.stringify(reloadResult)}\nOriginal document: ${JSON.stringify(originalDocument)}\nRenderer recovery probe: ${JSON.stringify({ ready: state.ready, rows: state.rows, chat: state.chat, frame: state.frame, context: state.context, requests: requestSummary })}\nLifecycle IPC trace:\n${requestTrace}\nRenderer failures:\n${rendererFailures}\nCDP events: ${JSON.stringify(eventSummary)}`);
          });
          assert.equal(await evaluate("location.href"), originalDocument.url);
          assert.notEqual(await evaluate("performance.timeOrigin"), originalDocument.timeOrigin, "crash recovery must inspect a new renderer document");
          assert.equal(recoveredView.rows, 1);
          const recoveryProbe = await evaluate(recoveryDiagnostics);
          assert.equal(recoveryProbe.requests.length, 2, "recovery probe must return both product requests");
          assert.deepEqual(recoveryProbe.requests.map((request: any) => request.status), ["fulfilled", "fulfilled"]);
          assert.equal(recoveryProbe.requests[0].value.ok, true);
          assert.ok(recoveryProbe.requests[0].value.sessions.some((session: any) => session.id === "native"));
          assert.match(JSON.stringify(recoveryProbe.requests[1].value), /Native lifecycle history marker/);
          assert.equal((logs.match(/Backend started:/g) || []).length, 1);
        } else if (mode === "crash" || mode === "pipe") {
          if (mode === "crash") process.kill(pid, "SIGKILL");
          else {
            assert.equal(await main.evaluate("candidateOwnedBackend.stdout.readable"), true);
            // The native peer's stdout is a genuine OS pipe; terminate its read
            // end through the child handle recorded only by this test bootstrap.
            await main.evaluate("candidateOwnedBackend.stdout.destroy()");
          }
          await wait(async () => absent(pid) ? true : undefined, "owned backend cleanup after transport failure");
          await evaluate("document.querySelector('#sessionList .session-item').click()");
          await wait(async () => await evaluate("document.querySelector('#notificationStack')?.textContent.trim().length>0") ? true : undefined, "visible current-UI transport error after session retry");
          assert.match(await evaluate("document.querySelector('#notificationStack').textContent"), /Backend|pipe|closed|EPIPE|ECONNRESET/i);
          assert.equal((logs.match(/Backend started:/g) || []).length, 1);
        }
        assert.equal(await readFile(path, "utf8"), original); assert.equal((await stat(path)).mtimeMs, before.mtimeMs);
        main.socket.close();
        void evaluate("window.close()").catch(() => {});
        await finish();
        if (mode === "shutdown-timeout") assert.match(logs, /Backend shutdown timed out; terminating owned process/);
      }
      const pid = Number(/Backend started: (\d+)/.exec(logs)?.[1]);
      if (pid) assert.ok(absent(pid), `${mode} left backend ${pid}`);
      console.log(`Native lifecycle PASS: ${mode}`);
    } catch (error) { console.error(`Native ${mode}:\n${logs}`); throw error; }
    finally {
      for (const socket of sockets) socket.close();
      if (child.exitCode === null && child.signalCode === null) {
        child.kill("SIGTERM"); await Promise.race([exited, delay(4000)]);
        if (child.exitCode === null && child.signalCode === null) { child.kill("SIGKILL"); await exited; }
      }
      const pid = Number(/Backend started: (\d+)/.exec(logs)?.[1]);
      if (pid && !absent(pid)) { process.kill(pid, "SIGKILL"); await poll(() => absent(pid)); }
    }
  }
});
