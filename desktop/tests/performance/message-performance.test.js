import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const base = join(root, "tmp/migration-message-performance");
await mkdir(base, { recursive: true });
const output = await mkdtemp(join(base, "run-"));
const config = join(output, "config"), projectConfig = join(output, "project-config"), sessions = join(output, "sessions");
for (const dir of [join(config, "models"), projectConfig, sessions]) await mkdir(dir, { recursive: true });
const token = Array.from({ length: 32 }, (_, index) => `stream-${String(index).padStart(2, "0")} `);
let modelRequests = 0;
const provider = createServer((request, response) => {
  modelRequests++;
  request.resume();
  request.on("end", () => {
    response.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache", connection: "keep-alive" });
    let index = 0;
    const timer = setInterval(() => {
      if (index < token.length) {
        response.write(`data: ${JSON.stringify({ choices: [{ delta: { content: token[index] } }] })}\n\n`);
        index++;
      } else {
        response.write(`data: ${JSON.stringify({ choices: [{ delta: {}, finish_reason: "stop" }], usage: { prompt_tokens: 10, completion_tokens: 32 } })}\n\n`);
        response.write("data: [DONE]\n\n");
        clearInterval(timer);
        response.end();
      }
    }, 30);
    response.on("close", () => clearInterval(timer));
  });
});
await new Promise((resolve, reject) => provider.listen(0, "127.0.0.1", resolve).once("error", reject));
const address = provider.address();
assert.ok(address && typeof address === "object");
await writeFile(join(config, "settings.json"), JSON.stringify({ defaultProvider: "synthetic", defaultModel: "stream" }));
await writeFile(join(config, "models", "synthetic.json"), JSON.stringify({ providers: { synthetic: {
  api: "openai-completions", apiKey: "fixture-key", baseUrl: `http://127.0.0.1:${address.port}/v1`,
  models: [{ id: "stream", contextWindow: 16384, maxTokens: 512 }],
} } }));
await writeFile(join(projectConfig, "settings.json"), "{}\n");
const header = JSON.stringify({ type: "session", version: 3, id: "history", timestamp: "2026-10-18T00:00:00Z", cwd: output }) + "\n";
const history = Array.from({ length: 600 }, (_, index) => {
  const userId = `user-${index}`, assistantId = `assistant-${index}`;
  const user = JSON.stringify({ type: "message", id: userId, parentId: index ? `assistant-${index - 1}` : null, timestamp: String(index * 2), message: { role: "user", content: `Earlier request ${index}: preserve the conversation branch and scroll position.` } });
  const assistant = JSON.stringify({ type: "message", id: assistantId, parentId: userId, timestamp: String(index * 2 + 1), message: { role: "assistant", model: "synthetic", content: [{ type: "text", text: `Earlier response ${index}: the saved history remains readable.` }] } });
  return `${user}\n${assistant}\n`;
}).join("");
await writeFile(join(sessions, "history.jsonl"), `${header}${history}`);

const electronBinary = join(root, "desktop/node_modules/electron/dist", (await readFile(join(root, "desktop/node_modules/electron/path.txt"), "utf8")).trim());
let logs = "";
const child = spawn(process.env.ROZSA_NATIVE_EXECUTABLE || electronBinary,
  [...(process.env.ROZSA_NATIVE_EXECUTABLE ? [] : [join(root, "desktop")]), "--remote-debugging-port=0", `--user-data-dir=${join(output, "profile")}`, "--session-dir", sessions],
  { cwd: output, env: { ...process.env, ROZSA_CONFIG_DIR: config, ROZSA_PROJECT_CONFIG_DIR: projectConfig, TMPDIR: join(root, "tmp/install-tmp") }, stdio: ["ignore", "pipe", "pipe"] });
const exited = new Promise((resolve, reject) => { child.once("error", reject); child.once("exit", resolve); });
const closed = new Promise(resolve => child.once("close", (code, signal) => resolve({ code, signal })));
child.stdout.on("data", chunk => { logs += chunk.toString(); });
child.stderr.on("data", chunk => { logs += chunk.toString(); });
async function poll(get, label, timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const value = await get();
    if (value !== undefined) return value;
    if (child.exitCode !== null || child.signalCode !== null) {
      await closed;
      throw new Error(`Electron exited during ${label} (${child.exitCode ?? child.signalCode}):\n${logs}`);
    }
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  throw new Error(`Timed out during ${label}:\n${logs}`);
}

let socket, closedNormally = false;
try {
  const debugURL = await poll(() => /DevTools listening on (ws:\/\/\S+)/.exec(logs)?.[1], "DevTools startup");
  socket = new WebSocket(debugURL);
  await new Promise((resolve, reject) => { socket.addEventListener("open", resolve, { once: true }); socket.addEventListener("error", reject, { once: true }); });
  let requestID = 0;
  const pending = new Map();
  const rendererFailures = [];
  socket.addEventListener("message", event => {
    const message = JSON.parse(String(event.data)), item = pending.get(message.id);
    if (message.method === "Runtime.exceptionThrown" || message.method === "Log.entryAdded") rendererFailures.push(message.params);
    if (!item) return;
    pending.delete(message.id); clearTimeout(item.timer);
    if (message.error) item.reject(new Error(JSON.stringify(message.error))); else item.resolve(message.result);
  });
  const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
    const id = ++requestID, timer = setTimeout(() => { pending.delete(id); reject(new Error(`DevTools timeout: ${method}`)); }, 10000);
    pending.set(id, { resolve, reject, timer });
    socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
  const target = await poll(async () => (await send("Target.getTargets")).targetInfos.find(item => item.type === "page" && (item.url.endsWith("/index.html") || /^http:\/\/127\.0\.0\.1:\d+\/$/.test(item.url))), "renderer target");
  const attached = await send("Target.attachToTarget", { targetId: target.targetId, flatten: true });
  await send("Runtime.enable", {}, attached.sessionId);
  await send("Log.enable", {}, attached.sessionId);
  const evaluate = async expression => {
    const result = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, attached.sessionId);
    if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  await poll(async () => await evaluate("document.readyState === 'complete' && typeof window.RozsaUIHost === 'object'") ? true : undefined, "renderer entry initialization");
  const readiness = await evaluate("({uiReady:typeof window.RozsaUIReady,host:typeof window.RozsaUIHost,bridge:typeof window.rozsa,conversation:typeof window.RozsaConversation,body:(document.body?.innerText || '').slice(0,240)})");
  assert.equal(readiness.uiReady, "object", `renderer initialization state: ${JSON.stringify({ target: target.url, readiness, rendererFailures })}`);
  const entries = await evaluate("window.RozsaUIHost.port.request('get_sessions')");
  assert.equal(entries.length, 1, `isolated Go session catalog: ${JSON.stringify(entries)}`);
  const productHistory = await evaluate(`window.rozsa.switchSession({path:${JSON.stringify(join(sessions, "history.jsonl"))},catalogId:'history'}).then(reply=>({ok:reply.ok,count:reply.snapshot?.messages.length,ids:reply.snapshot?.messages.slice(0,3).map(message=>message.entryId)}))`);
  assert.equal(productHistory.count, 1200, `Electron product bridge history: ${JSON.stringify(productHistory)}`);
  const historyStartedAt = Date.now();
  await evaluate(`window.RozsaUIHost.port.request('switch_session',{path:${JSON.stringify(join(sessions, "history.jsonl"))}})`);
  const loadedHistory = await evaluate("window.RozsaUIHost.port.request('get_state').then(state=>({sessionId:state.sessionId,messages:state.messages.length}))");
  assert.equal(loadedHistory.messages, 1200, `Go history snapshot: ${JSON.stringify(loadedHistory)}`);
  await poll(async () => await evaluate("Number(document.querySelector('#chatMessages [data-virtualized=true]')?.dataset.historyEntryCount) === 1200") ? true : undefined, `long history virtual data (${JSON.stringify(await evaluate("document.querySelector('#chatMessages [data-virtualized=true]')?.dataset.historyEntryCount"))} entries)`);
  const historyRenderMs = Date.now() - historyStartedAt;
  const historyMetrics = await evaluate(`(async () => { const chat=document.querySelector('#chatMessages'); const rows=chat.querySelectorAll('.msg').length; const historyEntries=Number(chat.querySelector('[data-virtualized=true]')?.dataset.historyEntryCount); chat.dispatchEvent(new WheelEvent('wheel',{deltaY:-120,bubbles:true})); chat.scrollTop=Math.floor(chat.scrollHeight*0.45); await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))); const bounds=chat.getBoundingClientRect(); const anchor=[...chat.querySelectorAll('.conversation-virtual-row')].find(row=>row.getBoundingClientRect().bottom>bounds.top); return {rows,historyEntries,scrollTop:chat.scrollTop,scrollHeight:chat.scrollHeight,clientHeight:chat.clientHeight,anchorId:anchor?.querySelector('.msg')?.dataset.odId,anchorOffset:anchor?.getBoundingClientRect().top-bounds.top}; })()`);
  assert.equal(historyMetrics.historyEntries, 1200, JSON.stringify(historyMetrics));
  assert.ok(historyMetrics.rows > 0 && historyMetrics.rows < historyMetrics.historyEntries, JSON.stringify(historyMetrics));
  const startedAt = Date.now();
  await evaluate("window.RozsaUIHost.port.request('send_message',{message:'Run the measured stream'})");
  let firstDeltaAt;
  try {
    firstDeltaAt = await poll(async () => {
      const value = await evaluate("window.RozsaUIHost.port.request('get_state').then(state=>state.messages.at(-1)?.message?.content?.map(block=>block.text||'').join(' ')||'')");
      return value.includes("stream-00") ? Date.now() : undefined;
    }, "first streamed delta", 10000);
  } catch (error) {
    const probe = await evaluate(`window.RozsaUIHost.port.request('get_state').then(state=>({rows:document.querySelectorAll('#chatMessages .msg').length,text:document.querySelector('#chatMessages')?.textContent.slice(-240),streaming:state.isStreaming,messages:state.messages.length,last:state.messages.at(-1)?.message}))`);
    throw new Error(`${String(error)}\nRenderer probe: ${JSON.stringify({ modelRequests, probe })}`);
  }
  const firstDeltaMs = firstDeltaAt - startedAt;
  await evaluate("document.querySelector('#msgInput').focus()");
  const inputStarted = Date.now();
  for (const character of "Measure responsive input while Go streams 32 updates. ") await send("Input.insertText", { text: character }, attached.sessionId);
  const typingMs = Date.now() - inputStarted;
  const inputText = await evaluate("document.querySelector('#msgInput').textContent");
  assert.equal(inputText, "Measure responsive input while Go streams 32 updates. ");
  await poll(async () => (await evaluate("window.RozsaUIHost.port.request('get_state').then(state=>state.messages.at(-1)?.message?.content?.map(block=>block.text||'').join(' ')||'')")).includes("stream-31"), "stream completion", 15000);
  const finalMetrics = await evaluate(`(() => { const chat=document.querySelector('#chatMessages'); const bounds=chat.getBoundingClientRect(); const anchor=[...chat.querySelectorAll('.conversation-virtual-row')].find(row=>row.querySelector('.msg')?.dataset.odId===${JSON.stringify(historyMetrics.anchorId)}); return {rows:chat.querySelectorAll('.msg').length,historyEntries:Number(chat.querySelector('[data-virtualized=true]')?.dataset.historyEntryCount),scrollTop:chat.scrollTop,scrollHeight:chat.scrollHeight,input:document.querySelector('#msgInput').textContent,anchorOffset:anchor?.getBoundingClientRect().top-bounds.top}; })()`);
  assert.ok(Number.isFinite(finalMetrics.anchorOffset), JSON.stringify({ historyMetrics, finalMetrics }));
  assert.ok(Math.abs(finalMetrics.anchorOffset - historyMetrics.anchorOffset) <= 8, JSON.stringify({ historyMetrics, finalMetrics }));
  assert.ok(finalMetrics.rows < finalMetrics.historyEntries, JSON.stringify(finalMetrics));
  assert.equal(finalMetrics.historyEntries, historyMetrics.historyEntries + 2);
  assert.ok(finalMetrics.rows < finalMetrics.historyEntries, JSON.stringify(finalMetrics));
  assert.equal(finalMetrics.input, inputText);
  assert.equal(modelRequests, 1);
  await writeFile(join(output, "result.json"), JSON.stringify({ platform: process.platform, history: { ...historyMetrics, renderMs: historyRenderMs }, typingMs, firstDeltaMs, completedRows: finalMetrics.rows, scrollPreserved: true, providerRequests: modelRequests }, null, 2));
  await poll(() => /Session window shown/.test(logs) ? true : undefined, "native window shown");
  socket.send(JSON.stringify({ id: ++requestID, method: "Runtime.evaluate", params: { expression: "window.close()" }, sessionId: attached.sessionId }));
  const exitCode = await Promise.race([exited, new Promise((_, reject) => { const timer = setTimeout(() => reject(new Error("Electron close timed out")), 10000); timer.unref(); })]);
  assert.equal(exitCode, 0);
  closedNormally = true;
  console.log(`Electron+Go message performance observed: ${JSON.stringify({ historyRows: historyMetrics.rows, historyRenderMs, typingMs, firstDeltaMs, rows: finalMetrics.rows, scrollPreserved: true })}; evidence ${join(output, "result.json")}`);
} finally {
  socket?.close();
  if (!closedNormally && child.exitCode === null && child.signalCode === null) {
    child.kill("SIGTERM");
    await Promise.race([exited, new Promise(resolve => setTimeout(resolve, 3000))]);
    if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
  }
  provider.close();
  await writeFile(join(output, "electron.log"), logs);
}
