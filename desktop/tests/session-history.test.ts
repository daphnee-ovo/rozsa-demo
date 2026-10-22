import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, writeFile, readFile, stat } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";
import { BackendService } from "../main/backend-service.ts";
import { sessionSnapshot, type SessionSnapshot, type SessionTarget, type ProductBridge } from "../main/session-contract.ts";
import { ProductClient } from "../renderer/product-client.ts";
import { assertProductBridgeCapabilities } from "./helpers/bridge-capabilities.ts";
import { createIsolatedBackendEnvironment } from "./helpers/backend-config.ts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const binary = join(root, "dist", process.platform === "win32" ? "rozsa-backend.exe" : "rozsa-backend");
const backendEnvironment = await createIsolatedBackendEnvironment();
const a = { id: "header-a", path: "/synthetic/a.jsonl", name: "A", modified: "t", message_count: 1 };
const b = { ...a, id: "header-b", path: "/synthetic/b.jsonl", name: "B" };
function snapshot(target: SessionTarget): SessionSnapshot {
  return { ...target, sessionId: target.path.endsWith("a.jsonl") ? "a" : "b", cwd: "/synthetic", contextMessageCount: 1,
    messages: [{ entryId: "shared-entry", role: "assistant", content: [{ type: "thinking", text: "reason" }, { type: "text", text: "<img src=x> **text**" }] }] };
}
test("session-history runtime response and restricted preload validate payload/identity", async () => {
  const target = {path:a.path,catalogId:a.id}, good = snapshot(target);
  for (const invalid of [null, {...good,path:b.path}, {...good,catalogId:b.id}, {...good,sessionId:null}, {...good,messages:null}, {...good,contextMessageCount:99}, {...good,messages:[{entryId:"x",role:"other",content:[]}]}]) assert.throws(() => sessionSnapshot(invalid,target));
  const calls: unknown[][] = []; let bridge!: ProductBridge;
  vm.runInNewContext(await readFile(join(root,"desktop/.vite/build/preload.cjs"),"utf8"), { require: (name: string) => {
    assert.equal(name,"electron"); return { contextBridge:{exposeInMainWorld: (_:string,value:ProductBridge) => bridge=value}, ipcRenderer:{invoke:async (...args:unknown[]) => {calls.push(args);return {ok:true,snapshot:good};}} };
  } });
  assertProductBridgeCapabilities(bridge,{events:true}); assert.equal(Object.isFrozen(bridge),true);
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
  const before = await stat(path); const service = new BackendService(binary,{env:backendEnvironment});
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
