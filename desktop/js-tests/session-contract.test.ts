import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdir, mkdtemp, readFile, writeFile, chmod } from "node:fs/promises";
import { join, resolve } from "node:path";
import { StreamMessageReader, StreamMessageWriter, type Message } from "vscode-jsonrpc/node";
import { BackendService, RPCFailure } from "../main/backend-service.ts";
import { RozsaBridge } from "../preload/bridge.ts";
import { ProductClient } from "../renderer/product-client.ts";
import { ProductFailure, remoteFault, validatedBridgeFault, bridgeFault } from "../../contracts/ipc/session.ts";
import { historyReply, sessionReply, sessionSnapshot, type ProductBridge, type SessionSnapshot } from "../main/session-contract.ts";

const root = resolve(import.meta.dirname, "../..");
const binary = join(root, "dist/migration", process.platform === "win32" ? "rozsa-backend.exe" : "rozsa-backend");
const target = { path: "/synthetic/file-stem.jsonl", catalogId: "header-id" };
const snapshot: SessionSnapshot = { ...target, sessionId: "file-stem", cwd: "/synthetic", messages: [{ entryId: "entry-id", role: "user", content: [{ type: "text", text: "text" }] }], contextMessageCount: 1 };
const cases = JSON.parse(await readFile(join(root, "tests/fixtures/migration/session-rpc.json"), "utf8"));

test("shared Go/JS inputs use library transport and preserve transport/product identities", {timeout:15000}, async () => {
  for (const item of cases) {
    const child = spawn(binary, [], { stdio: "pipe" });
    const reader = new StreamMessageReader(child.stdout), writer = new StreamMessageWriter(child.stdin);
    const responses: any[] = [];
    const received = Promise.withResolvers<void>();
    let diagnostic = ""; child.stderr.on("data", chunk => diagnostic += chunk.toString());
    reader.listen(value => {
      responses.push(value);
      if (responses.length === (item.notification || item.terminationCode ? 1 : 2)) received.resolve();
    });
    const closed = new Promise<number | null>((yes, no) => { child.once("error", no); child.once("close", yes); });
    try {
      await writer.write({ jsonrpc: "2.0", id: "init", method: "initialize", params: { protocolVersion: 1 } } as Message);
      if (item.packet) child.stdin.write(item.packet); // Literal malformed fixture, not an encoder.
      else await writer.write(item.request as Message);
      // Closing stdin now cancels in-flight delivery. Complete ordinary RPC
      // contract exchanges first; malformed input still exercises termination.
      if (!item.terminationCode) {
        const timer = setTimeout(() => received.reject(new Error(`Missing response: ${item.name}`)), 3000);
        try { await received.promise; } finally { clearTimeout(timer); }
      }
      writer.end(); const exit = await closed; await new Promise(resolve => setImmediate(resolve));
      if (item.terminationCode) {
        assert.equal(exit,1,item.name); assert.equal(responses.length,1,item.name);
        assert.match(diagnostic, item.terminationCode === -32700 ? /parse error/ : /invalid request/);
        // The diagnostic code is asserted in Go. There is no wire error here.
        continue;
      }
      assert.equal(exit,0,diagnostic);
      assert.equal(responses.length, item.notification ? 1 : 2, item.name);
      if (item.notification) continue;
      const response = responses[1]; assert.equal(response.id, item.id, item.name);
      if ("result" in item) { assert.equal(response.result, item.result); continue; }
      const error = remoteFault(response.error.code, response.error.message, response.error.data);
      assert.equal(error.kind, item.kind, item.name); assert.equal(error.code, item.code, item.name);
      const reply = { ok: false, error };
      const bridge = item.request?.method === "switch_session" ? historyReply(reply, target) : sessionReply(reply);
      assert.deepEqual(bridge, reply);
    } finally { reader.dispose(); writer.dispose(); child.kill(); }
  }
  assert.equal(sessionSnapshot(snapshot, target).sessionId, "file-stem");
  for (const identity of ["transport-only", "header-id", "entry-id"]) {
    assert.throws(() => sessionSnapshot({ ...snapshot, path: identity }, target), /identity/);
  }
});

test("typed errors survive main, preload, and renderer without recategorizing", async () => {
  for (const fault of [
    remoteFault(-32602, "Invalid params", { kind: "invalid_params" }),
    remoteFault(-32002, "Not initialized", { kind: "not_initialized" }),
    remoteFault(-32001, "Catalog failed", { kind: "catalog" }),
    remoteFault(-32800, "Delivery canceled", { kind: "canceled" }),
    remoteFault(-32004, "Request timed out", { kind: "timeout" }),
    remoteFault(-32005, "Disconnected", { kind: "disconnected" }),
    remoteFault(-32006, "Busy", { kind: "busy" }),
    remoteFault(-32007, "Snapshot required", { kind: "resync_required" }),
    { kind: "transport", reason: "timeout", message: "Timed out, outcome unknown" } as const,
  ]) {
    const reply = { ok: false, error: fault };
    const bridge = RozsaBridge.create(async () => reply);
    const client = new ProductClient(bridge);
    await assert.rejects(() => client.getSessions(), error => error instanceof ProductFailure && JSON.stringify(error.failure) === JSON.stringify(fault));
    assert.deepEqual(bridgeFault(new ProductFailure(fault)), fault);
  }
  for (const invalid of [null, {}, {kind:"catalog",code:-32602,message:"wrong"}, {kind:"catalog",code:-32001,message:null}, {kind:"transport",reason:"retry",message:"x"}, {kind:"transport",reason:"closed",message:"x",code:-32001}]) assert.throws(() => validatedBridgeFault(invalid));
  assert.throws(() => remoteFault(-32001, "ambiguous"));
  assert.throws(() => remoteFault(-32602, "mismatch", { kind: "session" }));
  assert.throws(() => historyReply({ok:false,error:{kind:"catalog",code:-32001,message:"wrong owner"}}, target));
});

test("UI cancellation settles reception, leaves actual work running and never sends abort", async () => {
  let finish!: (value: unknown) => void, worked = false;
  const calls: unknown[][] = [];
  const bridge = RozsaBridge.create(async (...args) => {
    calls.push(args);
    if (args[0] === "product:get-sessions") return {ok:true,sessions:[]};
    const value = await new Promise(resolve => { finish = resolve; });
    worked = true; return value;
  });
  const client = new ProductClient(bridge), controller = new AbortController();
  const reception = client.switchSession(target, { signal: controller.signal });
  await new Promise(resolve => setImmediate(resolve));
  controller.abort("scene changed");
  await assert.rejects(reception, error => error instanceof ProductFailure && error.failure.kind === "ui_cancelled" && error.cause === "scene changed");
  assert.equal(worked, false);
  finish({ ok: true, snapshot }); await new Promise(resolve => setImmediate(resolve));
  assert.equal(worked, true);
  assert.deepEqual(await client.getSessions(), []);
  assert.deepEqual(calls, [["product:switch-session", target], ["product:get-sessions"]]);
  const preAborted = new AbortController(); preAborted.abort();
  await assert.rejects(client.getSessions({signal:preAborted.signal}), error => error instanceof ProductFailure && error.failure.kind === "ui_cancelled");
  const sameTick = new AbortController(); const pending = client.getSessions({signal:sameTick.signal}); sameTick.abort(); await assert.rejects(pending);
  assert.equal(calls.length, 2);
});

test("late rejected UI work is consumed, transport failures keep their type", async () => {
  let reject!: (error: Error) => void;
  const client = new ProductClient({ getSessions: () => new Promise((_, no) => { reject = no; }) });
  const controller = new AbortController(), pending = client.getSessions({signal:controller.signal});
  await new Promise(resolve => setImmediate(resolve)); controller.abort(); await assert.rejects(pending);
  reject(new Error("late pipe failure")); await new Promise(resolve => setImmediate(resolve));
  const malformed = new ProductClient({getSessions:async()=>({ok:true,sessions:null})} as unknown as ProductBridge);
  await assert.rejects(malformed.getSessions(), error => error instanceof ProductFailure && error.failure.kind === "transport" && error.failure.reason === "protocol");
  await assert.rejects(client.switchSession({path:null,catalogId:"x"} as unknown as typeof target));
  const invalidTarget = new ProductClient({getSessions:async()=>({ok:true,sessions:[]}),switchSession:async()=>{throw new Error("must not invoke");}});
  await assert.rejects(invalidTarget.switchSession({path:null,catalogId:"x"} as unknown as typeof target), error => error instanceof ProductFailure && error.failure.kind === "invalid_params");
});

test("real backend typed errors remain usable; exit rejects without replay", async () => {
  const backend = new BackendService(binary, []);
  try {
    await assert.rejects(backend.getSessions(), error => error instanceof RPCFailure && error.remote.kind === "not_initialized");
    await backend.initialize();
    await assert.rejects(backend.getSessions(), error => error instanceof RPCFailure && error.remote.kind === "catalog");
    await assert.rejects(backend.switchSession(target), error => error instanceof RPCFailure && error.remote.kind === "session");
    const closed = new Promise(resolve => backend.process.once("close", resolve)); backend.process.kill("SIGKILL"); await closed;
    await assert.rejects(backend.getSessions(), error => error instanceof ProductFailure && error.failure.kind === "transport");
  } finally { await backend.stop(); }
});

test("timeout, pipe exit and malformed typed error never resend an uncertain request", {skip:process.platform === "win32",timeout:20000}, async () => {
 for (const mode of ["timeout", "exit", "invalid-error"]) {
  const base = join(root,"tmp/session-contract-tests"); await mkdir(base,{recursive:true}); const dir = await mkdtemp(join(base,"timeout-"));
  const fake = join(dir,"backend.cjs"), marker = join(dir,"received.txt");
  // Synthetic endpoint uses the same Node library. No wire codec in the test.
  await writeFile(fake, [
    `#!${process.execPath}`,
    `const {createMessageConnection,ResponseError}=require(${JSON.stringify(join(root,"desktop/node_modules/vscode-jsonrpc/lib/node/main.js"))});`,
    "const {appendFileSync}=require('node:fs');",
    "const conn=createMessageConnection(process.stdin,process.stdout);",
    "conn.onRequest('initialize',()=>({protocolVersion:1,capabilities:['get_sessions','switch_session','subscribe_sessions','next_session_event','unsubscribe_sessions']}));",
    "conn.onRequest('shutdown',()=>true);",
    `conn.onRequest('get_sessions',()=>{appendFileSync(${JSON.stringify(marker)},'one\\n');${mode === "exit" ? "process.exit(0);" : mode === "invalid-error" ? "throw new ResponseError(-32001,'Missing product error kind');" : "return new Promise(()=>{});"}});`,
    "conn.listen(); console.error('synthetic ready');", "",
  ].join("\n"));
  await chmod(fake,0o700);
  const backend = new BackendService(fake, [], {requestTimeoutMs:3000});
  try {
    // Synchronize the synthetic server's startup before measuring a request
    // timeout. The production timeout still includes a real process startup.
    const [ready] = await once(backend.process.stderr,"data",{signal:AbortSignal.timeout(10000)});
    assert.match(ready.toString(),/synthetic ready/);
    await backend.initialize();
    await assert.rejects(backend.getSessions(), error => error instanceof ProductFailure && error.failure.kind === "transport" && error.failure.reason === (mode === "timeout" ? "timeout" : mode === "exit" ? "closed" : "protocol"));
    await assert.rejects(backend.getSessions());
    assert.equal(await readFile(marker,"utf8"),"one\n");
  } finally { await backend.stop(); }
  assert.throws(() => process.kill(backend.process.pid!,0), /ESRCH/);
 }
});
