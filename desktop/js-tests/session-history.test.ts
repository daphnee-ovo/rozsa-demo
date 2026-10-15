import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, writeFile, readFile, stat } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";
import { JSDOM } from "jsdom";
import { BackendService } from "../main/backend-service.ts";
import { sessionSnapshot, type SessionSnapshot, type SessionTarget, type ProductBridge } from "../main/session-contract.ts";
import { ProductClient } from "../renderer/product-client.ts";
import { SessionHistory, type HistorySource } from "../renderer/features/session-history.ts";
import { SessionList } from "../renderer/features/session-list.ts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const binary = join(root, "dist/migration", process.platform === "win32" ? "rozsa-backend.exe" : "rozsa-backend");
const a = { id: "header-a", path: "/synthetic/a.jsonl", name: "A", modified: "t", message_count: 1 };
const b = { ...a, id: "header-b", path: "/synthetic/b.jsonl", name: "B" };
function snapshot(target: SessionTarget): SessionSnapshot {
  return { ...target, sessionId: target.path.endsWith("a.jsonl") ? "a" : "b", cwd: "/synthetic", contextMessageCount: 1,
    messages: [{ entryId: "shared-entry", role: "assistant", content: [{ type: "thinking", text: "reason" }, { type: "text", text: "<img src=x> **text**" }] }] };
}
function setup(source: HistorySource) {
  const dom = new JSDOM('<h2 id="title"></h2><p id="status"></p><button id="retry" hidden>Retry</button><div id="history"></div>');
  const doc = dom.window.document, history = doc.querySelector<HTMLElement>("#history")!;
  const title = doc.querySelector<HTMLElement>("#title")!, status = doc.querySelector<HTMLElement>("#status")!, retry = doc.querySelector<HTMLButtonElement>("#retry")!;
  Object.defineProperties(history, { scrollHeight: { get: () => 1000 }, clientHeight: { get: () => 200 } });
  const selected: string[] = [];
  return { dom, history, title, status, retry, selected, feature: new SessionHistory(source, history, title, status, retry, path => selected.push(path)) };
}
function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

test("session-history switching preserves per-session scroll/expansion and treats untrusted history as text", async () => {
  const ui = setup({ switchSession: async target => snapshot(target) });
  try {
    await ui.feature.switchTo(a);
    assert.equal(ui.history.dataset.sessionId, "a"); assert.equal(ui.history.querySelectorAll("img").length, 0);
    assert.match(ui.history.textContent!, /<img src=x> \*\*text\*\*/);
    ui.history.scrollTop = 140; ui.history.querySelector("details")!.open = true;
    await ui.feature.switchTo(b); assert.equal(ui.history.querySelector("details")!.open, false);
    ui.history.scrollTop = 330;
    await ui.feature.switchTo(a); assert.equal(ui.history.scrollTop, 140); assert.equal(ui.history.querySelector("details")!.open, true);
    await ui.feature.switchTo(b); assert.equal(ui.history.scrollTop, 330);
    assert.deepEqual(ui.selected, [a.path,b.path,a.path,b.path]);
  } finally { ui.dom.window.close(); }
});
test("session-history empty history, failure retains current view, manual retry and identity mismatch", async () => {
  let mode: "valid" | "empty" | "error" | "wrong" = "valid";
  const ui = setup({ switchSession: async target => {
    if (mode === "error") throw new Error("Failed to open session file");
    const value = snapshot(target);
    if (mode === "empty") { value.messages = []; value.contextMessageCount = 0; }
    if (mode === "wrong") value.path = a.path;
    return value;
  } });
  try {
    await ui.feature.switchTo(a);
    mode = "error"; await ui.feature.switchTo(b);
    assert.equal(ui.title.textContent, "A"); assert.equal(ui.history.dataset.sessionId, "a"); assert.equal(ui.retry.hidden, false);
    assert.equal(ui.status.getAttribute("role"), "alert"); assert.match(ui.status.textContent!, /Failed to open/);
    mode = "empty"; ui.retry.click(); await new Promise(resolve => setImmediate(resolve));
    assert.equal(ui.history.dataset.sessionId, "b"); assert.equal(ui.history.children.length, 0); assert.match(ui.status.textContent!, /No messages yet/); assert.equal(ui.retry.hidden, true);
    mode = "wrong"; await ui.feature.switchTo(b); assert.match(ui.status.textContent!, /identity/);
  } finally { ui.dom.window.close(); }
});
test("session-history follows the existing GUI's user content projection", async () => {
  const ui = setup({ switchSession: async target => ({...snapshot(target),messages:[{entryId:"user",role:"user",displayText:"separate model field",content:[{type:"text",text:"visible content"}]}]}) });
  try { await ui.feature.switchTo(a); assert.match(ui.history.textContent!,/visible content/); assert.ok(!ui.history.textContent!.includes("separate model field")); }
  finally { ui.dom.window.close(); }
});
test("session-history late success/error and A→B→A races cannot overwrite the latest request", async () => {
  const requests: ReturnType<typeof deferred<SessionSnapshot>>[] = [];
  const signals: AbortSignal[] = [];
  const ui = setup({ switchSession: (_, options) => { signals.push(options!.signal!); const item = deferred<SessionSnapshot>(); requests.push(item); return item.promise; } });
  try {
    const first = ui.feature.switchTo(a), second = ui.feature.switchTo(b), third = ui.feature.switchTo(a);
    assert.deepEqual(signals.map(signal => signal.aborted), [true, true, false]);
    assert.match(ui.status.textContent!, /Loading/);
    const fresh = snapshot({ path:a.path,catalogId:a.id }); fresh.messages[0].content = [{type:"text",text:"latest"}];
    requests[2].resolve(fresh); await third;
    requests[0].resolve(snapshot({ path:a.path,catalogId:a.id })); await first;
    requests[1].reject(new Error("late failure")); await second;
    assert.match(ui.history.textContent!, /latest/); assert.equal(ui.status.getAttribute("role"), "status"); assert.equal(ui.retry.hidden, true);
    assert.deepEqual(ui.selected, [a.path]);
  } finally { ui.dom.window.close(); }
});
test("session-history real list selection and refresh keep active path", async () => {
  const dom = new JSDOM('<ul></ul><p></p><button>Refresh</button>');
  const doc = dom.window.document, picked: string[] = [];
  const feature = new SessionList({ getSessions: async () => [a,b] }, doc.querySelector("ul")!,doc.querySelector("p")!,doc.querySelector("button")!,entry => { picked.push(entry.path); feature.setActive(entry.path); });
  try {
    await feature.load(); (doc.querySelectorAll(".session-select")[1] as HTMLButtonElement).click();
    await feature.load(); assert.deepEqual(picked, [b.path]); assert.equal(doc.querySelectorAll(".session-select")[1].getAttribute("aria-pressed"),"true");
  } finally { dom.window.close(); }
});
test("session-history runtime response and restricted preload validate payload/identity", async () => {
  const target = {path:a.path,catalogId:a.id}, good = snapshot(target);
  for (const invalid of [null, {...good,path:b.path}, {...good,catalogId:b.id}, {...good,sessionId:null}, {...good,messages:null}, {...good,contextMessageCount:99}, {...good,messages:[{entryId:"x",role:"other",content:[]}]}]) assert.throws(() => sessionSnapshot(invalid,target));
  const calls: unknown[][] = []; let bridge!: ProductBridge;
  vm.runInNewContext(await readFile(join(root,"desktop/.vite/build/preload.cjs"),"utf8"), { require: (name: string) => {
    assert.equal(name,"electron"); return { contextBridge:{exposeInMainWorld: (_:string,value:ProductBridge) => bridge=value}, ipcRenderer:{invoke:async (...args:unknown[]) => {calls.push(args);return {ok:true,snapshot:good};}} };
  } });
  assert.deepEqual(Object.keys(bridge),["getSessions","switchSession","subscribeSessions","nextSessionEvent","unsubscribeSessions"]); assert.equal(Object.isFrozen(bridge),true);
  assert.deepEqual(await new ProductClient(bridge).switchSession(target),good);
  for (const invalid of [null,{}, {path:a.path,catalogId:1}, {...target,method:"write"}]) await assert.rejects(() => (bridge.switchSession as (value:unknown)=>Promise<unknown>)(invalid),/Invalid session target/);
  assert.equal(calls.length,1); assert.equal(calls[0][0],"product:switch-session");
  assert.deepEqual(JSON.parse(JSON.stringify(calls[0])),["product:switch-session",target]);
});
test("session-history actual Go process restores branch history, empty, read failure/retry, cache and readonly input", async () => {
  const base = join(root,"tmp/session-history-tests"); await mkdir(base,{recursive:true}); const dir = await mkdtemp(join(base,"case-"));
  const header = (id:string) => JSON.stringify({type:"session",version:3,id,timestamp:"t",cwd:"/synthetic"})+"\n";
  const message = (id:string,parentId:string|null,text:string) => JSON.stringify({type:"message",id,parentId,timestamp:id,message:{role:"user",content:text}})+"\n";
  const path = join(dir,"filename.jsonl"), empty = join(dir,"empty.jsonl"), bad = join(dir,"bad.jsonl"), blank = join(dir,"blank.jsonl");
  const text = header("different-header")+message("a",null,"old")+message("fork","a","off branch")+message("b","a","kept")+JSON.stringify({type:"compaction",id:"c",parentId:"b",timestamp:"c",summary:"s",firstKeptEntryId:"b",tokensBefore:100})+"\n"+message("d","c","new");
  await writeFile(path,text); await writeFile(empty,header("empty")); await writeFile(bad,Buffer.concat([Buffer.from(header("bad")),Buffer.from([255,10])]));
  await writeFile(blank,header("blank")+JSON.stringify({type:"message",id:"tool",timestamp:"t",message:{role:"assistant",content:[{type:"text",text:""},{type:"toolCall",id:"call",name:"",arguments:{}}],api:"openai-completions",provider:"openai",model:"test",stopReason:"toolUse"}})+"\n");
  const before = await stat(path); const service = new BackendService(binary,[dir]);
  try {
    await service.initialize();
    const value = await service.switchSession({path,catalogId:"different-header"});
    assert.equal(value.sessionId,"filename"); assert.equal(value.catalogId,"different-header"); assert.deepEqual(value.messages.map(m=>m.entryId),["a","b","d"]); assert.equal(value.contextMessageCount,2);
    assert.equal(await readFile(path,"utf8"),text); assert.equal((await stat(path)).mtimeMs,before.mtimeMs);
    assert.deepEqual((await service.switchSession({path:empty,catalogId:"empty"})).messages,[]);
    const blankSnapshot = await service.switchSession({path:blank,catalogId:"blank"});
    assert.equal(blankSnapshot.messages[0].content[0].type,"text"); assert.equal(blankSnapshot.messages[0].content[1].type,"toolCall");
    await assert.rejects(()=>service.switchSession({path:bad,catalogId:"bad"}),/invalid UTF-8/);
    await writeFile(bad,header("bad")); assert.deepEqual((await service.switchSession({path:bad,catalogId:"bad"})).messages,[]);
    await assert.rejects(()=>service.switchSession({path,catalogId:"wrong"}),/identity/);
    await assert.rejects(()=>service.switchSession({path:join(root,"VERSION"),catalogId:"x"}),/configured catalog/);
    assert.deepEqual(await service.switchSession({path,catalogId:"different-header"}),value);
  } finally { await service.stop(); }
  assert.equal(service.process.exitCode,0); assert.throws(()=>process.kill(service.process.pid!,0),/ESRCH/);
});
