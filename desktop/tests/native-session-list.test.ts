// Native Electron acceptance for the current product UI and real session catalog.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdir, mkdtemp, writeFile, readFile, stat, rename } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createIsolatedBackendEnvironment } from "./helpers/backend-config.ts";
import { assertProductBridgeCapabilityKeys } from "./helpers/bridge-capabilities.ts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const backendEnvironment = await createIsolatedBackendEnvironment();
const base = join(root, "tmp/migration-native-session-list");
await mkdir(base, { recursive: true });
const output = await mkdtemp(join(base, "run-"));
const global = join(output, "global"), project = join(output, "project");
await mkdir(global); await mkdir(project);
const skillDirectory = join(backendEnvironment.ROZSA_CONFIG_DIR!, "skills/page-check");
await mkdir(skillDirectory, { recursive: true });
await writeFile(join(output, "page-reference.txt"), "controlled path completion\n");
await writeFile(join(skillDirectory, "SKILL.md"), "---\nname: page-check\ndescription: Verify skill injection from the Composer\n---\nPAGE_CHECK_SKILL_BODY_7f31\n");
let providerRequest: Record<string, unknown> | undefined;
let resolveProviderRequest: ((request: Record<string, unknown>) => void) | undefined;
const provider = createServer(async (request, response) => {
  const chunks: Buffer[] = [];
  for await (const chunk of request) chunks.push(Buffer.from(chunk));
  providerRequest = JSON.parse(Buffer.concat(chunks).toString("utf8"));
  resolveProviderRequest?.(providerRequest);
  response.writeHead(200, { "content-type": "text/event-stream" });
  response.end('data: {"choices":[{"delta":{"content":"controlled reply"}}]}\n\ndata: {"choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":2,"completion_tokens":2}}\n\ndata: [DONE]\n\n');
});
await new Promise<void>(resolve => provider.listen(0, "127.0.0.1", resolve));
const providerAddress = provider.address();
assert.ok(providerAddress && typeof providerAddress !== "string");
const modelDirectory = join(backendEnvironment.ROZSA_CONFIG_DIR!, "models");
await writeFile(join(modelDirectory, "synthetic.json"), JSON.stringify({ providers: { synthetic: {
  api: "openai-completions", apiKey: "fixture-key", baseUrl: `http://127.0.0.1:${providerAddress.port}/v1`,
  models: [{ id: "test", contextWindow: 4096, maxTokens: 256 }],
} } }));
function session(id: string, text: string, timestamp: string): string {
  return JSON.stringify({ type: "session", version: 3, id, timestamp: "2026-10-13T00:00:00Z", cwd: output }) + "\n"
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
const child = spawn(process.env.ROZSA_NATIVE_EXECUTABLE || electronBinary, ["--remote-debugging-port=0", `--user-data-dir=${join(output, "profile")}`, ...(process.env.ROZSA_NATIVE_EXECUTABLE ? [] : [join(root, "desktop")]), "--session-dir", global, "--session-dir", project], { cwd: output, env: { ...process.env, ...backendEnvironment, TMPDIR: join(root, "tmp/install-tmp") }, stdio: ["ignore", "pipe", "pipe"] });
const exited = new Promise<number | null>((resolve, reject) => { child.once("error", reject); child.once("exit", code => resolve(code)); });
child.stdout.on("data", (data: Buffer) => { logs += data.toString(); });
child.stderr.on("data", (data: Buffer) => { logs += data.toString(); });
async function poll<T>(get: () => Promise<T | undefined>, label: string): Promise<T> {
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    const value = await get();
    if (value !== undefined) return value;
    if (child.exitCode !== null || child.signalCode !== null) throw new Error(`Electron exited during ${label} (code=${child.exitCode}, signal=${child.signalCode}): ${logs}`);
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
  let observedTargets: unknown[] = [];
  const target = await poll(async () => {
    const result = await send("Target.getTargets");
    observedTargets = result.targetInfos.map((item: { type: string; url: string; title: string }) => ({ type: item.type, url: item.url, title: item.title }));
    return result.targetInfos.find((item: { type: string; url: string }) => {
      if (item.type !== "page") return false;
      try { return new URL(item.url).pathname.endsWith("/index.html") || /^http:\/\/127\.0\.0\.1:\d+\/$/.test(item.url); }
      catch { return false; }
    });
  }, "renderer target").catch(error => { throw new Error(`${String(error)}\nObserved targets: ${JSON.stringify(observedTargets)}`); });
  const attached = await send("Target.attachToTarget", { targetId: target.targetId, flatten: true });
  const evaluate = async (expression: string): Promise<any> => {
    try {
      const result = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, attached.sessionId);
      if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
      return result.result.value;
    } catch (error) {
      throw new Error(`CDP evaluate failed for ${expression.slice(0, 180)}: ${String(error)}`);
    }
  };
  const clickSession = async (index: number) => {
    const point = await evaluate(`(()=>{const rect=document.querySelectorAll('#sessionList .session-item')[${index}].getBoundingClientRect();return{x:rect.left+rect.width/2,y:rect.top+rect.height/2}})()`);
    await send("Input.dispatchMouseEvent", { type: "mousePressed", x: point.x, y: point.y, button: "left", clickCount: 1 }, attached.sessionId);
    await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: point.x, y: point.y, button: "left", clickCount: 1 }, attached.sessionId);
  };
  const composerState = () => evaluate(`(()=>{const input=document.querySelector('#msgInput'),selection=getSelection();return{text:input?.textContent??null,selectionStart:input?.contains(selection?.anchorNode)?selection.anchorOffset:null,selectionEnd:input?.contains(selection?.focusNode)?selection.focusOffset:null}})()`);
  const typeDraft = async (text: string, caret: number) => {
    await evaluate(`(()=>{const input=document.querySelector('#msgInput');input.focus();const range=document.createRange();range.selectNodeContents(input);range.collapse(false);const selection=getSelection();selection.removeAllRanges();selection.addRange(range)})()`);
    await send("Input.insertText", { text }, attached.sessionId);
    for (let move = text.length - caret; move > 0; move--) {
      await send("Input.dispatchKeyEvent", { type: "keyDown", key: "ArrowLeft", code: "ArrowLeft", windowsVirtualKeyCode: 37 }, attached.sessionId);
      await send("Input.dispatchKeyEvent", { type: "keyUp", key: "ArrowLeft", code: "ArrowLeft", windowsVirtualKeyCode: 37 }, attached.sessionId);
    }
    await evaluate("new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))");
  };
  const pressKey = async (key: string, code: string, windowsVirtualKeyCode: number) => {
    await send("Input.dispatchKeyEvent", { type: "keyDown", key, code, windowsVirtualKeyCode }, attached.sessionId);
    await send("Input.dispatchKeyEvent", { type: "keyUp", key, code, windowsVirtualKeyCode }, attached.sessionId);
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
  assert.equal(snapshot.node, "undefined"); assertProductBridgeCapabilityKeys(snapshot.bridge, { events: true }); assert.equal(snapshot.windows, 0);
  assert.equal(await evaluate(`window.rozsa.getSessions('/forbidden').then(()=>false,error=>error.message)`), "getSessions accepts no arguments");
  assert.equal(await evaluate(`window.RozsaUIHost.port.request('get_sessions').then(value=>value.length)`), 2);
  await clickSession(0);
  const switched = await poll(async () => {
    const state = await evaluate(`({name:document.querySelector('#currentSessionName')?.textContent,chat:document.querySelector('#chatMessages')?.textContent})`);
    return state.name === "Review session catalog" && state.chat.includes("Review session catalog") ? state : undefined;
  }, "real session switch into the conversation view");
  assert.match(switched.chat, /Review session catalog/);
  await evaluate("document.querySelector('#msgInput').textContent = ''");
  await typeDraft("A 草稿🙂尾", "A 草稿🙂".length);
  assert.deepEqual(await composerState(), { text: "A 草稿🙂尾", selectionStart: "A 草稿🙂".length, selectionEnd: "A 草稿🙂".length });
  const screenshot = await send("Page.captureScreenshot", {}, attached.sessionId);
  await writeFile(join(output, "full-ui.png"), Buffer.from(screenshot.data, "base64"));
  await clickSession(1);
  const secondSession = await poll(async () => {
    const state = await evaluate(`({name:document.querySelector('#currentSessionName')?.textContent,chat:document.querySelector('#chatMessages')?.textContent,draft:document.querySelector('#msgInput')?.textContent})`);
    return state.name?.includes("Project session") && state.chat.includes("Project session") && state.draft === "" ? state : undefined;
  }, "second session with its own empty composer");
  assert.equal(secondSession.draft, "");
  await typeDraft("B 草稿", "B 草稿".length);
  assert.deepEqual(await composerState(), { text: "B 草稿", selectionStart: "B 草稿".length, selectionEnd: "B 草稿".length });
  await clickSession(0);
  const restoredFirstDraft = await poll(async () => {
    const state = await evaluate(`({name:document.querySelector('#currentSessionName')?.textContent,draft:document.querySelector('#msgInput')?.textContent,selection:(()=>{const input=document.querySelector('#msgInput'),selection=getSelection();return input?.contains(selection?.anchorNode)?selection.anchorOffset:null})()})`);
    return state.name === "Review session catalog" && state.draft === "A 草稿🙂尾" ? state : undefined;
  }, "first session draft restoration");
  const firstDraftScreenshot = await send("Page.captureScreenshot", {}, attached.sessionId);
  await writeFile(join(output, "draft-session-a.png"), Buffer.from(firstDraftScreenshot.data, "base64"));
  assert.deepEqual(restoredFirstDraft, { name: "Review session catalog", draft: "A 草稿🙂尾", selection: "A 草稿🙂".length });
  await clickSession(1);
  const restoredSecondDraft = await poll(async () => {
    const state = await evaluate(`({name:document.querySelector('#currentSessionName')?.textContent,draft:document.querySelector('#msgInput')?.textContent,selection:(()=>{const input=document.querySelector('#msgInput'),selection=getSelection();return input?.contains(selection?.anchorNode)?selection.anchorOffset:null})()})`);
    return state.name?.includes("Project session") && state.draft === "B 草稿" ? state : undefined;
  }, "second session draft restoration");
  assert.deepEqual(restoredSecondDraft, { name: "Project session · 玫瑰🌹", draft: "B 草稿", selection: "B 草稿".length });
  const secondDraftScreenshot = await send("Page.captureScreenshot", {}, attached.sessionId);
  await writeFile(join(output, "draft-session-b.png"), Buffer.from(secondDraftScreenshot.data, "base64"));
  const beginComposerInput = async () => evaluate("(()=>{const input=document.querySelector('#msgInput');input.focus();input.textContent='';const selection=getSelection();const range=document.createRange();range.selectNodeContents(input);range.collapse(false);selection.removeAllRanges();selection.addRange(range)})()");
  await beginComposerInput();
  await send("Input.insertText", { text: "@page" }, attached.sessionId);
  await poll(async () => evaluate(`document.querySelector('#autocomplete .ac-item')?.textContent.includes('page-reference.txt') ? true : undefined`), "real @ path suggestions in the Composer");
  await pressKey("ArrowDown", "ArrowDown", 40);
  await pressKey("Enter", "Enter", 13);
  const completedPath = await poll(async () => evaluate(`({text:document.querySelector('#msgInput')?.textContent,highlight:!!document.querySelector('#msgInput .valid-token-text')})`).then(value => value.text === "@page-reference.txt " && value.highlight ? value : undefined), "selecting and highlighting an @ path suggestion");
  assert.deepEqual(completedPath, { text: "@page-reference.txt ", highlight: true });
  const slashLatencyMs: number[] = [];
  for (let attempt = 0; attempt < 10; attempt++) {
    await beginComposerInput();
    await evaluate("hideAutocomplete()");
    await evaluate(`(()=>{const popup=document.querySelector('#autocomplete');window.__slashStart=performance.now();window.__slashTiming=new Promise(resolve=>{const observer=new MutationObserver(()=>{if(popup.classList.contains('visible')&&popup.querySelector('.ac-item')){observer.disconnect();resolve(performance.now()-window.__slashStart)}});observer.observe(popup,{attributes:true,childList:true,subtree:true})});return true})()`);
    await send("Input.insertText", { text: "/" }, attached.sessionId);
    const latency = Number(await evaluate("window.__slashTiming"));
    slashLatencyMs.push(latency);
    assert.ok(await evaluate(`document.querySelector('#autocomplete')?.classList.contains('visible')`), `slash popup not visible on sample ${attempt + 1}`);
    await poll(async () => evaluate(`[...document.querySelectorAll('#autocomplete .ac-item')].some(item => item.textContent.includes('/settings')) ? true : undefined`), `slash suggestion items on sample ${attempt + 1}`);
  }
  const orderedSlashLatencyMs = [...slashLatencyMs].sort((left, right) => left - right);
  const slashLatency = {
    samplesMs: slashLatencyMs,
    p50Ms: orderedSlashLatencyMs[Math.floor((orderedSlashLatencyMs.length - 1) * 0.5)],
    p95Ms: orderedSlashLatencyMs[Math.ceil(orderedSlashLatencyMs.length * 0.95) - 1],
    maxMs: orderedSlashLatencyMs.at(-1),
  };
  assert.ok(slashLatency.maxMs <= 200, `complete slash menu latency exceeded 200 ms: ${JSON.stringify(slashLatency)}`);
  await writeFile(join(output, "slash-latency.json"), JSON.stringify(slashLatency, null, 2));
  await beginComposerInput();
  await evaluate("hideAutocomplete()");
  await send("Input.insertText", { text: "/skill:page" }, attached.sessionId);
  await poll(async () => evaluate(`document.querySelector('#autocomplete .ac-item')?.textContent.includes('/skill:page-check') ? true : undefined`), "Go-backed skill suggestion in the Composer");
  await beginComposerInput();
  await evaluate("hideAutocomplete()");
  await send("Input.insertText", { text: "/skill:page" }, attached.sessionId);
  await poll(async () => evaluate(`document.querySelector('#autocomplete .ac-item')?.textContent.includes('/skill:page-check') ? true : undefined`), "Go-backed skill suggestion in the Composer");
  await pressKey("ArrowDown", "ArrowDown", 40);
  const highlightedSkill = await poll(async () => evaluate(`document.querySelector('#autocomplete .ac-item.active .ac-cmd')?.textContent`)
    .then(value => value === "/skill:page-check" ? value : undefined), "highlighting the Go-backed skill suggestion").catch(async error => {
      const state = await evaluate(`({input:document.querySelector('#msgInput')?.textContent,focused:document.activeElement?.id,visible:document.querySelector('#autocomplete')?.classList.contains('visible'),items:[...document.querySelectorAll('#autocomplete .ac-item')].map(item=>({text:item.textContent,active:item.classList.contains('active')})),selectedIndex:typeof acSelectedIndex==='number'?acSelectedIndex:null,acVisible:typeof acVisible==='boolean'?acVisible:null})`);
      throw new Error(`${String(error)}\nSuggestion state: ${JSON.stringify(state)}`);
    });
  assert.equal(highlightedSkill, "/skill:page-check");
  await pressKey("Enter", "Enter", 13);
  const selectedSkill = await poll(async () => {
    const state = await composerState();
    return state.text === "/skill:page-check " ? state : undefined;
  }, "selecting the Go-backed skill suggestion").catch(async error => {
    const diagnostics = await evaluate(`({text:document.querySelector('#msgInput')?.textContent,items:[...document.querySelectorAll('#autocomplete .ac-item')].map(item=>item.textContent)})`);
    throw new Error(`${String(error)}\nComposer selection diagnostics: ${JSON.stringify(diagnostics)}`);
  });
  assert.equal(selectedSkill.text, "/skill:page-check ");
  const selectedSkillHighlight = await evaluate(`document.querySelector('#msgInput .valid-token-text')?.textContent ?? null`);
  assert.equal(selectedSkillHighlight, "/skill:page-check", "the selected Go-backed skill command must render as a highlighted token");
  const providerRequestPromise = new Promise<Record<string, unknown>>(resolve => { resolveProviderRequest = resolve; });
  await pressKey("Enter", "Enter", 13);
  const capturedProviderRequest = await poll(async () => providerRequest ?? await Promise.race([
    providerRequestPromise,
    new Promise<undefined>(resolve => setTimeout(() => resolve(undefined), 50)),
  ]), "controlled local provider receiving the selected skill prompt").catch(async error => {
    const diagnostics = await evaluate(`({composer:document.querySelector('#msgInput')?.textContent,chat:document.querySelector('#chatMessages')?.textContent,alerts:[...document.querySelectorAll('[role="alert"],.notification,.toast')].map(item=>item.textContent),activeMessageApi:typeof window.rozsa.getActiveMessage})`);
    throw new Error(`${String(error)}\nProvider request: ${JSON.stringify(providerRequest)}\nSubmission diagnostics: ${JSON.stringify(diagnostics)}`);
  });
  const serializedProviderRequest = JSON.stringify(capturedProviderRequest);
  assert.match(serializedProviderRequest, /PAGE_CHECK_SKILL_BODY_7f31/);
  assert.match(serializedProviderRequest, /<name>page-check<\/name>/);
  assert.doesNotMatch(serializedProviderRequest, /"content":"\/skill:page-check"/);
  const updatedFixture = await poll(async () => {
    const value = await readFile(fixture, "utf8");
    return value.includes("controlled reply") ? value : undefined;
  }, "assistant response persisted to the selected JSONL session");
  assert.ok(updatedFixture.startsWith(fixtureText), "sending the skill prompt must preserve the existing session records");
  assert.notEqual((await stat(fixture)).mtimeMs, before.mtimeMs, "the selected session must persist the submitted skill prompt");
  assert.match(updatedFixture, /PAGE_CHECK_SKILL_BODY_7f31/);
  assert.match(updatedFixture, /controlled reply/);
  await poll(async () => /Session window shown/.test(logs) ? true : undefined, "native Electron window shown");
  // Normal window close travels through window-all-closed and backend shutdown.
  socket.send(JSON.stringify({ id: ++id, method: "Runtime.evaluate", params: { expression: "window.close()" }, sessionId: attached.sessionId }));
  const exitCode = await Promise.race([exited, new Promise<never>((_, reject) => { const timer = setTimeout(() => reject(new Error("Electron close timed out")), 10000); timer.unref(); })]);
  assert.equal(exitCode, 0);
  const backendPID = Number(/Backend started: (\d+)/.exec(logs)?.[1]);
  assert.ok(backendPID > 0); assert.match(logs, new RegExp(`Backend stopped: ${backendPID} \\(0\\)`));
  assert.throws(() => process.kill(backendPID, 0), /ESRCH/);
  closedNormally = true;
  await new Promise<void>(resolve => provider.close(() => resolve()));
  await writeFile(join(output, "result.json"), JSON.stringify({ platform: process.platform, snapshot, windowShown: true, electronExitCode: exitCode, backendPID, backendAbsent: true, fixtureExistingHistoryPreserved: true, draftRetention: { firstSession: restoredFirstDraft, secondSession: restoredSecondDraft }, slashLatency, selectedSkillProviderRequest: capturedProviderRequest, atPathCompletion: completedPath, screenshots: [join(output, "full-ui.png"), join(output, "draft-session-a.png"), join(output, "draft-session-b.png")] }, null, 2));
  console.log(`Native session-list PASS: ${output}; slash latency ${JSON.stringify(slashLatency)}`);
} finally {
  socket?.close();
  if (provider.listening) await new Promise<void>(resolve => provider.close(() => resolve()));
  if (!closedNormally && child.exitCode === null && child.signalCode === null) {
    // Only the subprocess owned by this test is addressed.
    child.kill("SIGTERM");
    await Promise.race([exited, new Promise(resolve => setTimeout(resolve, 3000))]);
    if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
  }
  await writeFile(join(output, "electron.log"), logs);
}
