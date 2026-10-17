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

const root = resolve(import.meta.dirname, "../..");
const binary = join(root, "dist", process.platform === "win32" ? "rozsa-backend.exe" : "rozsa-backend");
const backendEnvironment = await createIsolatedBackendEnvironment();
const capabilities = ["get_sessions", "switch_session", "subscribe_sessions", "next_session_event", "unsubscribe_sessions", "runtime_status", "start_message", "continue_message", "subscribe_message_events", "next_message_event", "unsubscribe_message_events", "cancel_message", "get_settings", "update_setting", "reset_setting", "list_themes", "get_theme", "save_theme"];
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
if (process.argv.includes("--native")) test("actual Electron candidate lifecycle and runtime security boundaries", { timeout: 150000 }, async () => {
  const electron = join(root, "desktop/node_modules/electron/dist", (await readFile(join(root, "desktop/node_modules/electron/path.txt"), "utf8")).trim());
  for (const mode of ["normal", "crash", "pipe", "version", "missing", "handshake-timeout", "shutdown-timeout", "quit-startup"]) {
    const base = join(root, "tmp/migration-lifecycle-native"); await mkdir(base, { recursive: true });
    const directory = await mkdtemp(join(base, `${mode}-`)), app = join(directory, "desktop"), backendDirectory = join(directory, "dist");
    await mkdir(app); await mkdir(backendDirectory, { recursive: true });
    await cp(join(root, "desktop/.vite"), join(app, ".vite"), { recursive: true });
    await writeFile(join(app, "package.json"), JSON.stringify({ name: "rozsa-migration-native-test", main: ".vite/build/acceptance.cjs" }));
    await writeFile(join(app, ".vite/build/acceptance.cjs"), [
      "globalThis.candidateElectron=require('electron');",
      "candidateElectron.dialog.showErrorBox=(title,message)=>console.error('Native startup error:',title,message);",
      "const childProcess=require('node:child_process'),spawn=childProcess.spawn;childProcess.spawn=(...args)=>{const child=spawn(...args);if(String(args[0]).includes('rozsa-backend'))globalThis.candidateOwnedBackend=child;return child;};",
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
      + JSON.stringify({ type: "message", id: "entry", parentId: null, timestamp: "t", message: { role: "user", content: '<img src=x onerror="window.pwned=true"><script>window.pwned=true</script>' } }) + "\n";
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
      const pending = new Map<number, { resolve(value: any): void; reject(error: Error): void; timer: NodeJS.Timeout }>();
      socket.addEventListener("message", event => {
        const value = JSON.parse(String(event.data)), item = pending.get(value.id); if (!item) return;
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
      return { send, evaluate, socket };
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
        const target = await wait(async () => (await browser.send("Target.getTargets")).targetInfos.find((item: any) => item.type === "page" && item.url.endsWith("/sessions/index.html")), "page");
        const attached = await browser.send("Target.attachToTarget", { targetId: target.targetId, flatten: true });
        const evaluate = (expression: string) => browser.evaluate(expression, attached.sessionId);
        await wait(async () => await evaluate("document.querySelector('#status')?.textContent==='1 sessions' || document.querySelector('#status')?.textContent==='No sessions yet.'") ? true : undefined, "session catalog");
        const pid = Number(/Backend started: (\d+)/.exec(logs)?.[1]); assert.ok(pid > 0);
        const window = "candidateElectron.BrowserWindow.getAllWindows()[0]";
        const preferences = await main.evaluate(`(()=>{const p=${window}.webContents.getLastWebPreferences();return {contextIsolation:p.contextIsolation,sandbox:p.sandbox,nodeIntegration:p.nodeIntegration};})()`);
        assert.deepEqual(preferences, { contextIsolation: true, sandbox: true, nodeIntegration: false });
        assert.deepEqual(await evaluate("({require:typeof require,process:typeof process,Buffer:typeof Buffer,keys:Object.keys(window.rozsa)})"), { require: "undefined", process: "undefined", Buffer: "undefined", keys: ["getSessions", "switchSession", "subscribeSessions", "nextSessionEvent", "unsubscribeSessions", "startMessage", "continueMessage", "getActiveMessage", "subscribeMessageEvents", "nextMessageEvent", "unsubscribeMessageEvents", "cancelMessage"] });
        if (mode !== "shutdown-timeout") {
          await evaluate("document.querySelector('.session-select').click()");
          await wait(async () => await evaluate("document.querySelector('#history').dataset.sessionId==='native'") ? true : undefined, "history");
        }
        if (mode === "normal") {
          assert.equal(await evaluate("document.querySelector('#history').querySelectorAll('img,script').length"), 0);
          assert.match(await evaluate("document.querySelector('#history').textContent"), /<img src=x/);
          await evaluate("(()=>{const s=document.createElement('script');s.textContent='window.pwned=true';document.body.append(s);})()");
          await delay(100); assert.equal(await evaluate("window.pwned===true"), false);
          assert.equal(await evaluate("fetch('https://example.com/').then(()=>false,()=>true)"), true);
          assert.equal(await evaluate("window.open('https://example.com/')===null"), true);
          const url = await evaluate("location.href");
          await evaluate("location.href='https://example.com/'"); await delay(100); assert.equal(await evaluate("location.href"), url);
          // A real second renderer uses the compiled preload but has no authority.
          const denied = await main.evaluate(`(async()=>{const owner=${window};const other=new candidateElectron.BrowserWindow({show:false,webPreferences:{preload:${JSON.stringify(join(app, ".vite/build/preload.cjs"))},contextIsolation:true,sandbox:true,nodeIntegration:false}});try{await other.loadURL(owner.webContents.getURL());return await other.webContents.executeJavaScript("window.rozsa.getSessions().then(()=>false,error=>error.message)");}finally{other.destroy();}})()`);
          assert.match(denied, /Unauthorized session request/);
          for (let reload = 0; reload < 2; reload++) {
            await browser.send("Page.reload", {}, attached.sessionId);
            await delay(100);
            await wait(async () => {
              try { return await evaluate("document.querySelector('#history')?.dataset.sessionId==='native' && document.querySelector('#status')?.textContent==='1 sessions'") ? true : undefined; }
              catch (error) { if (/context|navigat/i.test(String(error))) return undefined; throw error; }
            }, "reload snapshot");
          }
          // Crash only this candidate's renderer, then explicitly reload it.
          await main.evaluate(`${window}.webContents.forcefullyCrashRenderer()`);
          await delay(100); await main.evaluate(`${window}.webContents.reload()`); await delay(150);
          await wait(async () => await evaluate("document.querySelector('#history')?.dataset.sessionId==='native'") ? true : undefined, "renderer crash recovery");
          assert.equal((logs.match(/Backend started:/g) || []).length, 1);
        } else if (mode === "crash" || mode === "pipe") {
          if (mode === "crash") process.kill(pid, "SIGKILL");
          else {
            assert.equal(await main.evaluate("candidateOwnedBackend.stdout.readable"), true);
            // The native peer's stdout is a genuine OS pipe; terminate its read
            // end through the child handle recorded only by this test bootstrap.
            await main.evaluate("candidateOwnedBackend.stdout.destroy()");
          }
          await wait(async () => await evaluate("document.querySelector('#history-status')?.getAttribute('role')==='alert'") ? true : undefined, "visible transport failure");
          assert.match(await evaluate("document.querySelector('#history-status').textContent"), /Session sync failed/);
          await wait(async () => absent(pid) ? true : undefined, "owned backend cleanup");
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
