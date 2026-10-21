// Current Electron product UI, isolated JSONL and an owned DevTools socket.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, mkdtemp, writeFile, readFile, stat } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createIsolatedBackendEnvironment } from "./helpers/backend-config.ts";
import { assertProductBridgeCapabilityKeys } from "./helpers/bridge-capabilities.ts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const backendEnvironment = await createIsolatedBackendEnvironment();
const base = join(root,"tmp/migration-native-session-history"); await mkdir(base,{recursive:true});
const output = await mkdtemp(join(base,"run-")); const dir = join(output,"sessions"); await mkdir(dir);
const header = (id:string) => JSON.stringify({type:"session",version:3,id,timestamp:"t",cwd:"/synthetic"})+"\n";
const entry = (id:string,parentId:string|null,message:object) => JSON.stringify({type:"message",id,parentId,timestamp:id,message})+"\n";
const path = join(dir,"filename.jsonl"), second = join(dir,"second.jsonl"), empty = join(dir,"empty.jsonl"), bad = join(dir,"bad.jsonl");
const long = "History line · 玫瑰🌹\n".repeat(260);
const original = header("header-id")+entry("a",null,{role:"user",content:"Before compaction"})
  +entry("fork","a",{role:"user",content:"Off branch"})
  +entry("b","a",{role:"assistant",content:[{type:"thinking",thinking:"Saved thinking"},{type:"text",text:long}],api:"openai-completions",provider:"openai",model:"synthetic",stopReason:"stop"})
  +JSON.stringify({type:"compaction",id:"c",parentId:"b",timestamp:"c",summary:"s",firstKeptEntryId:"b",tokensBefore:100})+"\n"
  +entry("d","c",{role:"user",content:"Latest branch message"});
const secondText = header("second")+entry("second-user",null,{role:"user",content:"Second session history\n"+long})
  +entry("second-thinking", "second-user", {role:"assistant",content:[{type:"thinking",thinking:"Second thought"},{type:"text",text:"Second response"}],api:"openai-completions",provider:"openai",model:"synthetic",stopReason:"stop"});
const emptyText = header("empty"), badText = Buffer.concat([Buffer.from(header("bad")+entry("broken",null,{role:"user",content:"Retry corrupted history"})),Buffer.from([255,10])]);
await writeFile(path,original); await writeFile(second,secondText); await writeFile(empty,emptyText); await writeFile(bad,badText);
const before = await stat(path), secondBefore = await stat(second), emptyBefore = await stat(empty), badBefore = await stat(bad);
const electron = join(root,"desktop/node_modules/electron/dist",(await readFile(join(root,"desktop/node_modules/electron/path.txt"),"utf8")).trim());
let logs = "";
const child = spawn(process.env.ROZSA_NATIVE_EXECUTABLE || electron,[...(process.env.ROZSA_NATIVE_EXECUTABLE ? [] : [join(root,"desktop")]),"--remote-debugging-port=0",`--user-data-dir=${join(output,"profile")}`,"--session-dir",dir],{cwd:output,env:{...process.env,...backendEnvironment,TMPDIR:join(root,"tmp/install-tmp")},stdio:["ignore","pipe","pipe"]});
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
  const target=await poll(async()=> (await send("Target.getTargets")).targetInfos.find((t:{type:string;url:string})=>t.type==="page"&&(t.url.endsWith("/index.html")||/^http:\/\/127\.0\.0\.1:\d+\/$/.test(t.url))),"renderer");
  const attached=await send("Target.attachToTarget",{targetId:target.targetId,flatten:true});
  const evaluate=async(expression:string):Promise<any>=>{
    const result=await send("Runtime.evaluate",{expression,returnByValue:true,awaitPromise:true},attached.sessionId);
    if(result.exceptionDetails)throw new Error(JSON.stringify(result.exceptionDetails));return result.result.value;
  };
  await poll(async()=> await evaluate(`document.querySelectorAll('#sessionList .session-item').length===4 && !!document.querySelector('#chatMessages')`) ? true:undefined,"current product session list");
  const boundary=await evaluate(`({node:typeof window.require,bridge:Object.keys(window.rozsa),frames:window.frames.length})`);
  assert.equal(boundary.node,"undefined"); assert.equal(boundary.frames,0); assertProductBridgeCapabilityKeys(boundary.bridge,{events:true});
  const evaluateSessionRows=()=>evaluate(`[...document.querySelectorAll('#sessionList .session-item')].map(row=>({name:row.querySelector('.session-name')?.textContent.trim(),text:row.textContent.trim()}))`);
  const rows=await evaluateSessionRows();
  const rowIndex=(rows: Array<{name:string}>, name:string)=>rows.findIndex(row=>row.name.startsWith(name));
  const catalogPaths=await evaluate(`window.rozsa.getSessions().then(reply=>reply.ok?reply.sessions.map(session=>session.path):[])`);
  const pathIndex=(value:string)=>catalogPaths.indexOf(value);
  const clickElement=async(selector:string)=>{
    const point=await evaluate(`(()=>{const rect=document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect();return{x:rect.left+rect.width/2,y:rect.top+rect.height/2}})()`);
    await send("Input.dispatchMouseEvent",{type:"mousePressed",x:point.x,y:point.y,button:"left",clickCount:1},attached.sessionId);
    await send("Input.dispatchMouseEvent",{type:"mouseReleased",x:point.x,y:point.y,button:"left",clickCount:1},attached.sessionId);
  };
  const clickSession=async(index:number)=>clickElement(`#sessionList .session-item:nth-child(${index+1})`);
  const currentChat=()=>evaluate(`({text:document.querySelector('#chatMessages')?.textContent||'',scrollTop:document.querySelector('#chatMessages')?.scrollTop||0,scrollHeight:document.querySelector('#chatMessages')?.scrollHeight||0,clientHeight:document.querySelector('#chatMessages')?.clientHeight||0})`);
  let wheelEvidence: unknown;
  const waitForScrolledView=async(before:{scrollTop:number;scrollHeight:number;clientHeight:number},label:string)=>{
    let previous:{scrollTop:number;scrollHeight:number}|undefined,stable=0;
    let last: Awaited<ReturnType<typeof currentChat>>|undefined;
    return poll(async()=>{
      const view=await currentChat();
      last=view;
      const movedUp=before.scrollTop-view.scrollTop>=30;
      const awayFromBottom=view.scrollTop+view.clientHeight<view.scrollHeight-8;
      if(!movedUp||!awayFromBottom){previous=undefined;stable=0;return undefined;}
      if(previous&&Math.abs(previous.scrollTop-view.scrollTop)<=1&&Math.abs(previous.scrollHeight-view.scrollHeight)<=1)stable++;
      else stable=0;
      previous=view;
      return stable>=2?view:undefined;
    },label).catch(error=>{const start={scrollTop:before.scrollTop,scrollHeight:before.scrollHeight,clientHeight:before.clientHeight};const summary=last&&{scrollTop:last.scrollTop,scrollHeight:last.scrollHeight,clientHeight:last.clientHeight,textTail:last.text.slice(-80)};throw new Error(`${String(error)}\nScroll before: ${JSON.stringify(start)}\nScroll after wheel: ${JSON.stringify(summary)}\nWheel event: ${JSON.stringify(wheelEvidence)}`);});
  };
  const waitForRestoredScroll=async(expected:number,text:string,label:string)=>{
    return poll(async()=>{
      const view=await currentChat();
      const expanded=await evaluate(`document.querySelector('#chatMessages .thinking-header')?.getAttribute('aria-expanded')==='true'`);
      const atExpected=Math.abs(view.scrollTop-expected)<=2;
      const awayFromBottom=view.scrollTop+view.clientHeight<view.scrollHeight-8;
      return view.text.includes(text)&&expanded&&atExpected&&awayFromBottom?view:undefined;
    },label);
  };
  const scrollConversation=async(deltaY:number)=>{
    const point=await evaluate(`(()=>{const chat=document.querySelector('#chatMessages'),rect=chat.getBoundingClientRect();const points=[[.5,.25],[.5,.5],[.5,.75],[.8,.5],[.2,.5]].map(([x,y])=>({x:rect.left+rect.width*x,y:rect.top+rect.height*y}));for(const point of points){const hit=document.elementFromPoint(point.x,point.y);if(hit?.closest('#chatMessages'))return{...point,width:innerWidth,height:innerHeight,hit:hit.id||hit.className,overflow:getComputedStyle(chat).overflowY}}return{x:rect.left+rect.width/2,y:rect.top+rect.height/2,width:innerWidth,height:innerHeight,hit:document.elementFromPoint(rect.left+rect.width/2,rect.top+rect.height/2)?.className||'',overflow:getComputedStyle(chat).overflowY}})()`);
    await evaluate("window.__historyWheelEvents=[];document.querySelector('#chatMessages').addEventListener('wheel',event=>window.__historyWheelEvents.push({deltaY:event.deltaY,target:event.target?.id||event.target?.className||'',defaultPrevented:event.defaultPrevented}),{once:true})");
    await send("Input.dispatchMouseEvent",{type:"mouseMoved",x:point.x,y:point.y,button:"none",buttons:0,pointerType:"mouse"},attached.sessionId);
    await new Promise(resolve=>setTimeout(resolve,50));
    await send("Input.synthesizeScrollGesture",{x:point.x,y:point.y,yDistance:-deltaY,speed:800,gestureSourceType:"mouse"},attached.sessionId);
    wheelEvidence=await evaluate("new Promise(resolve=>setTimeout(()=>resolve(window.__historyWheelEvents),100))");
    if(!Array.isArray(wheelEvidence)||wheelEvidence.length===0)wheelEvidence={events:wheelEvidence,point};
    return point;
  };
  const firstIndex=rowIndex(rows,"Before compaction"); const secondIndex=rowIndex(rows,"Second session history");
  const emptyIndex=pathIndex(empty); const badIndex=pathIndex(bad);
  assert.ok(firstIndex>=0 && secondIndex>=0 && emptyIndex>=0 && badIndex>=0,JSON.stringify(rows));
  await clickSession(firstIndex);
  const firstHistory=await poll(async()=>{const chat=await currentChat();return chat.text.includes("Latest branch message")?chat:undefined;},"selected branch history rendered in conversation");
  assert.match(firstHistory.text,/Before compaction/); assert.match(firstHistory.text,/Latest branch message/); assert.doesNotMatch(firstHistory.text,/Off branch/);
  const firstBytes=await readFile(path), firstStat=await stat(path);
  assert.deepEqual(firstBytes,Buffer.from(original)); assert.equal(firstStat.mtimeMs,before.mtimeMs);
  const thought=await evaluate(`document.querySelector('#chatMessages .thinking-header')?.getAttribute('aria-expanded')`);
  assert.equal(thought,"false","stored thinking starts collapsed in the current conversation renderer");
  await evaluate(`document.querySelector('#chatMessages .thinking-header').click()`);
  await poll(async()=>await evaluate(`document.querySelector('#chatMessages .thinking-header')?.getAttribute('aria-expanded')==='true'`)?true:undefined,"expanded thinking block");
  const firstAtBottom=await currentChat();
  assert.ok(firstAtBottom.scrollHeight>firstAtBottom.clientHeight);
  assert.ok(firstAtBottom.scrollTop+firstAtBottom.clientHeight>=firstAtBottom.scrollHeight-16,"first session should begin at its latest message");
  await scrollConversation(-500);
  const firstView=await waitForScrolledView(firstAtBottom,"first session moved up and settled after wheel input");
  assert.ok(Array.isArray(wheelEvidence)&&wheelEvidence.some((event:{deltaY:number})=>event.deltaY<0),"first wheel input must move toward earlier history");
  await clickSession(secondIndex);
  const secondHistory=await poll(async()=>{const chat=await currentChat();return chat.text.includes("Second response")?chat:undefined;},"second session history rendered");
  assert.match(secondHistory.text,/Second session history/); assert.doesNotMatch(secondHistory.text,/Latest branch message/);
  await evaluate(`document.querySelector('#chatMessages .thinking-header').click()`);
  await poll(async()=>await evaluate(`document.querySelector('#chatMessages .thinking-header')?.getAttribute('aria-expanded')==='true'`)?true:undefined,"second session thinking expansion");
  const secondAtBottom=await currentChat();
  assert.ok(secondAtBottom.scrollHeight>secondAtBottom.clientHeight);
  assert.ok(secondAtBottom.scrollTop+secondAtBottom.clientHeight>=secondAtBottom.scrollHeight-16,"second session should begin at its latest message");
  await scrollConversation(-900);
  const secondView=await waitForScrolledView(secondAtBottom,"second session moved up and settled after wheel input");
  assert.ok(Array.isArray(wheelEvidence)&&wheelEvidence.some((event:{deltaY:number})=>event.deltaY<0),"second wheel input must move toward earlier history");
  await clickSession(firstIndex);
  await poll(async()=>await evaluate(`document.querySelector('#chatMessages').textContent.includes('Latest branch message') && document.querySelector('#chatMessages .thinking-header')?.getAttribute('aria-expanded')==='true'`)?true:undefined,"first session history and thinking restored");
  const restoredFirst=await waitForRestoredScroll(firstView.scrollTop,"Latest branch message","first session scroll restored away from bottom");
  await clickSession(secondIndex);
  await poll(async()=>await evaluate(`document.querySelector('#chatMessages').textContent.includes('Second response') && document.querySelector('#chatMessages .thinking-header')?.getAttribute('aria-expanded')==='true'`)?true:undefined,"second session history and thinking restored");
  const restoredSecond=await waitForRestoredScroll(secondView.scrollTop,"Second response","second session scroll restored away from bottom");
  await clickSession(secondIndex); await clickSession(firstIndex); await clickSession(secondIndex); await clickSession(firstIndex);
  const lastSelection=await poll(async()=>{
    const view=await currentChat();
    const activeIndex=await evaluate(`Number(document.querySelector('#sessionList .session-item.active')?.dataset.sessionIndex)`);
    return activeIndex===firstIndex&&view.text.includes("Latest branch message")?view:undefined;
  },"rapid UI switching keeps the last selected conversation");
  assert.match(lastSelection.text,/Latest branch message/); assert.doesNotMatch(lastSelection.text,/Second response/);
  await clickSession(emptyIndex);
  const emptyState=await poll(async()=>{const chat=await currentChat();return chat.text.includes("Start a new conversation")?chat:undefined;},"empty session visible state");
  assert.match(emptyState.text,/Start a new conversation/);
  assert.deepEqual(await readFile(empty),Buffer.from(emptyText)); assert.equal((await stat(empty)).mtimeMs,emptyBefore.mtimeMs);
  const errorsBefore=await evaluate(`document.querySelectorAll('#notificationStack .notification-error').length`);
  await clickSession(badIndex);
  const historyError=await poll(async()=>{
    const value=await evaluate(`({count:document.querySelectorAll('#notificationStack .notification-error').length,messages:[...document.querySelectorAll('#notificationStack .notification-error .notification-message')].map(item=>item.textContent)})`);
    return value.count>errorsBefore&&value.messages.some((message:string)=>message.includes("invalid UTF-8"))?value:undefined;
  },"new visible history-read error after selecting the broken session");
  assert.ok(historyError.count>errorsBefore); assert.ok(historyError.messages.some((message:string)=>message.includes("invalid UTF-8")));
  assert.deepEqual(await readFile(bad),badText);assert.equal((await stat(bad)).mtimeMs,badBefore.mtimeMs);
  const repaired=header("bad")+entry("repaired",null,{role:"user",content:"Repaired session history"});
  await writeFile(bad,repaired);
  await clickSession(badIndex);
  const retried=await poll(async()=>{const chat=await currentChat();return chat.text.includes("Repaired session history")?chat:undefined;},"manual click retry after repairing invalid JSONL");
  assert.match(retried.text,/Repaired session history/);
  const screenshot=await send("Page.captureScreenshot",{},attached.sessionId);
  await writeFile(join(output,"full-ui.png"),Buffer.from(screenshot.data,"base64"));
  assert.deepEqual(await readFile(path),Buffer.from(original));assert.equal((await stat(path)).mtimeMs,before.mtimeMs);
  assert.deepEqual(await readFile(second),Buffer.from(secondText));assert.equal((await stat(second)).mtimeMs,secondBefore.mtimeMs);
  assert.deepEqual(await readFile(empty),Buffer.from(emptyText));assert.equal((await stat(empty)).mtimeMs,emptyBefore.mtimeMs);
  const rejected=await evaluate(`window.rozsa.switchSession({path:${JSON.stringify(join(root,"VERSION"))},catalogId:'forbidden'}).then(r=>r)`);
  assert.equal(rejected.ok,false);assert.match(rejected.error.message,/configured catalog/);
  await poll(async()=>/Session window shown/.test(logs)?true:undefined,"native Electron window shown");
  socket.send(JSON.stringify({id:++id,method:"Runtime.evaluate",params:{expression:"window.close()"},sessionId:attached.sessionId}));
  const exitCode=await Promise.race([exited,new Promise<never>((_,reject)=>{const t=setTimeout(()=>reject(new Error("Window close timeout")),10000);t.unref();})]);
  assert.equal(exitCode,0);
  const backendPID=Number(/Backend started: (\d+)/.exec(logs)?.[1]);assert.ok(backendPID>0);assert.match(logs,new RegExp(`Backend stopped: ${backendPID} \\(0\\)`));assert.throws(()=>process.kill(backendPID,0),/ESRCH/);
  normal=true;
  await writeFile(join(output,"result.json"),JSON.stringify({platform:process.platform,boundary,branchHistory:true,emptyState:true,errorRetry:true,rapidSwitch:true,scrollViews:{first:{before:firstAtBottom.scrollTop,after:firstView.scrollTop,restored:restoredFirst.scrollTop,scrollHeight:firstView.scrollHeight,clientHeight:firstView.clientHeight},second:{before:secondAtBottom.scrollTop,after:secondView.scrollTop,restored:restoredSecond.scrollTop,scrollHeight:secondView.scrollHeight,clientHeight:secondView.clientHeight}},readonly:true,windowShown:true,electronExitCode:exitCode,backendPID,backendAbsent:true},null,2));
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
