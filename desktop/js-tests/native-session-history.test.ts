// T003's actual candidate, isolated JSONL and owned DevTools socket. No mocks.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, mkdtemp, writeFile, readFile, stat } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const base = join(root,"tmp/migration-native-session-history"); await mkdir(base,{recursive:true});
const output = await mkdtemp(join(base,"run-")); const dir = join(output,"sessions"); await mkdir(dir);
const header = (id:string) => JSON.stringify({type:"session",version:3,id,timestamp:"t",cwd:"/synthetic"})+"\n";
const entry = (id:string,parentId:string|null,message:object) => JSON.stringify({type:"message",id,parentId,timestamp:id,message})+"\n";
const path = join(dir,"filename.jsonl"), empty = join(dir,"empty.jsonl"), bad = join(dir,"bad.jsonl");
const long = "History line · 玫瑰🌹\n".repeat(90);
const original = header("header-id")+entry("a",null,{role:"user",content:"Before compaction"})
  +entry("fork","a",{role:"user",content:"Off branch"})
  +entry("b","a",{role:"assistant",content:[{type:"thinking",thinking:"Saved thinking"},{type:"text",text:long}],api:"openai-completions",provider:"openai",model:"synthetic",stopReason:"stop"})
  +JSON.stringify({type:"compaction",id:"c",parentId:"b",timestamp:"c",summary:"s",firstKeptEntryId:"b",tokensBefore:100})+"\n"
  +entry("d","c",{role:"toolResult",toolCallId:"call",toolName:"Read",content:[{type:"text",text:"Tool result text"}],isError:false});
const emptyText = header("empty"), badText = Buffer.concat([Buffer.from(header("bad")),Buffer.from([255,10])]);
await writeFile(path,original); await writeFile(empty,emptyText); await writeFile(bad,badText);
const before = await stat(path), emptyBefore = await stat(empty), badBefore = await stat(bad);
const electron = join(root,"desktop/node_modules/electron/dist",(await readFile(join(root,"desktop/node_modules/electron/path.txt"),"utf8")).trim());
let logs = "";
const child = spawn(process.env.ROZSA_NATIVE_EXECUTABLE || electron,[...(process.env.ROZSA_NATIVE_EXECUTABLE ? [] : [join(root,"desktop")]),"--remote-debugging-port=0",`--user-data-dir=${join(output,"profile")}`,"--session-dir",dir],{cwd:output,env:{...process.env,TMPDIR:join(root,"tmp/migration/install-tmp")},stdio:["ignore","pipe","pipe"]});
const exited = new Promise<number|null>((resolve,reject)=>{child.once("error",reject);child.once("exit",resolve);});
child.stdout.on("data",(b:Buffer)=>logs+=b.toString()); child.stderr.on("data",(b:Buffer)=>logs+=b.toString());
async function poll<T>(get:()=>Promise<T|undefined>,label:string):Promise<T> {
  const deadline = Date.now()+15000;
  while(Date.now()<deadline) {
    const result = await get(); if(result!==undefined) return result;
    if(child.exitCode!==null||child.signalCode!==null) throw new Error(`Electron exited: ${logs}`);
    await new Promise(resolve=>setTimeout(resolve,100));
  }
  throw new Error(`Timed out: ${label}\n${logs}`);
}
let socket:WebSocket|undefined, normal=false;
try {
  const url = await poll(async()=>/DevTools listening on (ws:\/\/\S+)/.exec(logs)?.[1],"startup");
  socket = new WebSocket(url); await new Promise<void>((resolve,reject)=>{socket!.addEventListener("open",()=>resolve(),{once:true});socket!.addEventListener("error",reject,{once:true});});
  let id=0;
  const pending = new Map<number,{resolve(value:any):void;reject(error:Error):void;timer:NodeJS.Timeout}>();
  socket.addEventListener("message",event=>{
    const data=JSON.parse(String(event.data)), item=pending.get(data.id); if(!item)return;
    pending.delete(data.id);clearTimeout(item.timer);if(data.error)item.reject(new Error(JSON.stringify(data.error)));else item.resolve(data.result);
  });
  const send=(method:string,params:object={},sessionId?:string):Promise<any>=>new Promise((resolve,reject)=>{
    const request=++id,timer=setTimeout(()=>{pending.delete(request);reject(new Error(`DevTools timeout: ${method}`));},5000);
    pending.set(request,{resolve,reject,timer});socket!.send(JSON.stringify({id:request,method,params,...(sessionId?{sessionId}:{})}));
  });
  const target=await poll(async()=> (await send("Target.getTargets")).targetInfos.find((t:{type:string;url:string})=>t.type==="page"&&t.url.endsWith("/sessions/index.html")),"renderer");
  const attached=await send("Target.attachToTarget",{targetId:target.targetId,flatten:true});
  const evaluate=async(expression:string):Promise<any>=>{
    const result=await send("Runtime.evaluate",{expression,returnByValue:true,awaitPromise:true},attached.sessionId);
    if(result.exceptionDetails)throw new Error(JSON.stringify(result.exceptionDetails));return result.result.value;
  };
  await poll(async()=> await evaluate(`document.querySelector('#sessions')?.children.length===3`) ? true:undefined,"list");
  const boundary=await evaluate(`({node:typeof window.require,bridge:Object.keys(window.rozsa),frames:window.frames.length})`);
  assert.deepEqual(boundary,{node:"undefined",bridge:["getSessions","switchSession"],frames:0});
  const click=async(catalogID:string)=>evaluate(`document.querySelector('#sessions li[data-session-id='+${JSON.stringify(catalogID)}+'] button').click()`);
  const waitSession=async(sessionID:string)=>poll(async()=> await evaluate(`document.querySelector('#history').dataset.sessionId===${JSON.stringify(sessionID)} && document.querySelector('#history-status').getAttribute('role')==='status' && !document.querySelector('#history-status').textContent.startsWith('Loading')`) ? true:undefined,`session ${sessionID}`);
  await click("header-id"); await waitSession("filename");
  const history=await evaluate(`({id:document.querySelector('#history').dataset.sessionId,path:document.querySelector('#history').dataset.sessionPath,status:document.querySelector('#history-status').textContent,ids:[...document.querySelectorAll('#history article')].map(a=>a.dataset.entryId),text:document.querySelector('#history').textContent,pressed:document.querySelector('[aria-pressed=true]').dataset.sessionPath})`);
  assert.equal(history.id,"filename"); assert.equal(history.path,path); assert.deepEqual(history.ids,["a","b","d"]); assert.match(history.status,/3 history messages · 2 in effective context/); assert.ok(!history.text.includes("Off branch")); assert.match(history.text,/Before compaction/); assert.match(history.text,/Tool result text/);
  await evaluate(`document.querySelector('#history details').open=true; document.querySelector('#history').scrollTop=140`);
  const savedScroll=await evaluate(`document.querySelector('#history').scrollTop`); assert.equal(savedScroll,140);
  const screenshot=async(name:string)=>{const image=await send("Page.captureScreenshot",{},attached.sessionId);await writeFile(join(output,name),Buffer.from(image.data,"base64"));};
  await screenshot("history.png");
  await click("empty"); await waitSession("empty");
  const emptyState=await evaluate(`({text:document.querySelector('#history-status').textContent,count:document.querySelector('#history').children.length})`);
  assert.deepEqual(emptyState,{text:"No messages yet. · Read only",count:0}); await screenshot("empty.png");
  await click("header-id"); await waitSession("filename");
  const restored=await evaluate(`({scroll:document.querySelector('#history').scrollTop,open:document.querySelector('#history details').open})`);
  assert.deepEqual(restored,{scroll:savedScroll,open:true});
  await click("bad");
  const failure=await poll(async()=>{
    const state=await evaluate(`({text:document.querySelector('#history-status').textContent,role:document.querySelector('#history-status').getAttribute('role'),retry:!document.querySelector('#retry-history').hidden,id:document.querySelector('#history').dataset.sessionId})`);
    return state.role==="alert"?state:undefined;
  },"reader failure");
  assert.match(failure.text,/invalid UTF-8/);assert.equal(failure.retry,true);assert.equal(failure.id,"filename");await screenshot("failure.png");
  assert.deepEqual(await readFile(bad),badText);assert.equal((await stat(bad)).mtimeMs,badBefore.mtimeMs);
  // Repair only this run's synthetic input, then activate the actual Retry button.
  await writeFile(bad,header("bad")+entry("retry",null,{role:"user",content:"Retry restored history"}));
  await evaluate(`document.querySelector('#retry-history').click()`);await waitSession("bad");
  assert.match(await evaluate(`document.querySelector('#history').textContent`),/Retry restored history/);await screenshot("retry.png");
  const rejected=await evaluate(`window.rozsa.switchSession({path:${JSON.stringify(join(root,"VERSION"))},catalogId:'forbidden'}).then(r=>r)`);
  assert.equal(rejected.ok,false);assert.match(rejected.error.message,/configured catalog/);
  assert.equal(await readFile(path,"utf8"),original);assert.equal((await stat(path)).mtimeMs,before.mtimeMs);
  assert.equal(await readFile(empty,"utf8"),emptyText);assert.equal((await stat(empty)).mtimeMs,emptyBefore.mtimeMs);
  assert.match(logs,/Session window shown/);
  socket.send(JSON.stringify({id:++id,method:"Runtime.evaluate",params:{expression:"window.close()"},sessionId:attached.sessionId}));
  const exitCode=await Promise.race([exited,new Promise<never>((_,reject)=>{const t=setTimeout(()=>reject(new Error("Window close timeout")),10000);t.unref();})]);
  assert.equal(exitCode,0);
  const backendPID=Number(/Backend started: (\d+)/.exec(logs)?.[1]);assert.ok(backendPID>0);assert.match(logs,new RegExp(`Backend stopped: ${backendPID} \\(0\\)`));assert.throws(()=>process.kill(backendPID,0),/ESRCH/);
  normal=true;
  await writeFile(join(output,"result.json"),JSON.stringify({platform:process.platform,boundary,history,emptyState,restored,failure,retry:true,readonly:true,windowShown:true,electronExitCode:exitCode,backendPID,backendAbsent:true},null,2));
  console.log(`Native session-history PASS: ${output}`);
} finally {
  socket?.close();
  if(!normal&&child.exitCode===null&&child.signalCode===null) {
    child.kill("SIGTERM");await Promise.race([exited,new Promise(resolve=>setTimeout(resolve,3000))]);
    if(child.exitCode===null&&child.signalCode===null){child.kill("SIGKILL");await exited;}
  }
  if(!normal) {
    const pid=Number(/Backend started: (\d+)/.exec(logs)?.[1]);
    if(pid>0){try{process.kill(pid,0);process.kill(pid,"SIGTERM");}catch(error){if((error as NodeJS.ErrnoException).code!=="ESRCH")throw error;}}
  }
  await writeFile(join(output,"electron.log"),logs);
}
