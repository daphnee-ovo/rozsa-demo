import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, readFile, stat } from "node:fs/promises";
import { dirname, resolve, join } from "node:path";
import { fileURLToPath } from "node:url";
import { JSDOM } from "jsdom";
import { BackendService, RPCFailure } from "../main/backend-service.ts";
import { sessionEntries, type ProductBridge } from "../main/session-contract.ts";
import { ProductClient } from "../renderer/product-client.ts";
import { SessionList } from "../renderer/features/session-list.ts";

if (process.argv[2] && process.argv[2] !== "session-list") throw new Error("Only the session-list slice is available");
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const binary = join(root, "dist/migration", process.platform === "win32" ? "rozsa-backend.exe" : "rozsa-backend");
const entry = { id: "synthetic", path: "/isolated/example.jsonl", name: "<img src=x onerror=alert(1)>", modified: "2026-10-13T00:00:00Z", message_count: 2 };

test("session-list controlled preload retains the no-argument list method", async () => {
  const vm = await import("node:vm");
  const preload = await readFile(join(root, "desktop/.vite/build/preload.cjs"), "utf8");
  let bridge: ProductBridge | undefined;
  const calls: string[] = [];
  vm.runInNewContext(preload, { require: (name: string) => {
    assert.equal(name, "electron");
    return { contextBridge: { exposeInMainWorld: (key: string, value: ProductBridge) => { assert.equal(key, "rozsa"); bridge = value; } }, ipcRenderer: { invoke: async (channel: string) => { calls.push(channel); return { ok: true, sessions: [] }; } } };
  } });
  assert.deepEqual(Object.keys(bridge!), ["getSessions", "switchSession"]);
  assert.equal(Object.isFrozen(bridge), true);
  assert.deepEqual(await new ProductClient(bridge!).getSessions(), []);
  await assert.rejects(() => (bridge!.getSessions as (arg: string) => Promise<unknown>)("/private"), /no arguments/);
  assert.deepEqual(calls, ["product:get-sessions"]);
});

test("session-list renderer: content is text, empty/error/loading states and retry", async () => {
  const dom = new JSDOM('<ul id="list"></ul><p id="status"></p><button id="refresh">Refresh</button>');
  const doc = dom.window.document;
  let response: "valid" | "empty" | "error" = "valid";
  const client = new ProductClient({ getSessions: async () => response === "error" ? { ok: false, error: { kind: "catalog", code: -32001, message: "Failed to read session dir: /isolated" } } : { ok: true, sessions: response === "empty" ? [] : [entry] } });
  const feature = new SessionList(client, doc.querySelector("#list")!, doc.querySelector("#status")!, doc.querySelector("button")!);
  const loading = feature.load(); assert.equal(doc.querySelector("button")!.disabled, true); assert.match(doc.querySelector("#status")!.textContent!, /Loading/); await loading;
  assert.equal(doc.querySelector(".session-name")!.textContent, entry.name);
  assert.equal(doc.querySelectorAll("img").length, 0);
  assert.match(doc.querySelector(".session-meta")!.textContent!, /2 messages/);
  response = "empty"; await feature.load(); assert.equal(doc.querySelectorAll("li").length, 0); assert.equal(doc.querySelector("#status")!.textContent, "No sessions yet.");
  response = "error"; await feature.load(); assert.equal(doc.querySelector("#status")!.getAttribute("role"), "alert"); assert.match(doc.querySelector("#status")!.textContent!, /Failed to read session dir/); assert.equal(doc.querySelector("button")!.disabled, false);
  response = "valid"; doc.querySelector("button")!.click(); await new Promise(resolve => setImmediate(resolve)); assert.equal(doc.querySelector("#status")!.getAttribute("role"), "status"); assert.equal(doc.querySelectorAll("li").length, 1);
  dom.window.close();
});

test("session-list validates runtime payload and rejects transport failures", async () => {
  for (const invalid of [null, {}, [ { ...entry, message_count: -1 } ], [{ ...entry, name: null }]]) assert.throws(() => sessionEntries(invalid));
  const malformed = new ProductClient({ getSessions: async () => ({ ok: true, sessions: null }) } as unknown as ProductBridge);
  await assert.rejects(() => malformed.getSessions(), /Invalid session list/);
  const failed = new ProductClient({ getSessions: async () => { throw new Error("pipe closed"); } });
  await assert.rejects(() => failed.getSessions(), /pipe closed/);
});

test("session-list real Go process: layers, empty/corrupt input, directory errors, read-only and shutdown", async () => {
  const base = join(root, "tmp/session-list-tests"); await mkdir(base, { recursive: true });
  const dir = await mkdtemp(join(base, "case-"));
  const global = join(dir, "global"), project = join(dir, "project");
  await mkdir(global); await mkdir(project);
  const content = (name: string, timestamp: string) => `${JSON.stringify({ type: "session", version: 3, id: "shared", timestamp: "created", cwd: "/synthetic" })}\n${JSON.stringify({ type: "message", id: "e", timestamp, message: { role: "user", content: name } })}\n`;
  const file = join(project, "session.jsonl"); const text = content("project 玫🌹", "2026-01");
  await writeFile(join(global, "session.jsonl"), content("global", "2026-09")); await writeFile(file, text);
  await writeFile(join(project, "broken.jsonl"), "not-json\n");
  const before = await stat(file);
  const service = new BackendService(binary, [global, project]);
  try {
    await service.initialize();
    const [first, second] = await Promise.all([service.getSessions(), service.getSessions()]);
    assert.deepEqual(first, second); assert.equal(first.length, 1); assert.equal(first[0].name, "project 玫🌹"); assert.equal(first[0].path, file);
    assert.equal(await readFile(file, "utf8"), text); assert.equal((await stat(file)).mtimeMs, before.mtimeMs);
  } finally { await service.stop(); }
  assert.equal(service.process.exitCode, 0);
  assert.throws(() => process.kill(service.process.pid!, 0), /ESRCH/);
  const empty = new BackendService(binary, [join(dir, "missing")]);
  try { await empty.initialize(); assert.deepEqual(await empty.getSessions(), []); } finally { await empty.stop(); }
  const bad = new BackendService(binary, [file]);
  try { await bad.initialize(); await assert.rejects(() => bad.getSessions(), (e: unknown) => e instanceof RPCFailure && e.code === -32001 && e.message === `Failed to read session dir: ${file}`); } finally { await bad.stop(); }
  const unconfigured = new BackendService(binary, []);
  try { await unconfigured.initialize(); await assert.rejects(() => unconfigured.getSessions(), /No session directories configured/); } finally { await unconfigured.stop(); }
});

test("session-list backend exit and failed spawn reject rather than return empty", async () => {
  const missing = new BackendService(join(root, "tmp/missing-backend-binary"), []);
  try { await assert.rejects(() => missing.initialize(), /ENOENT/); } finally { await missing.stop(); }
  const backend = new BackendService(binary, []);
  await backend.initialize();
  const closed = new Promise(resolve => backend.process.once("close", resolve));
  const pending = backend.getSessions();
  backend.process.kill("SIGKILL");
  // The pipe may complete a read before the kill; either outcome must not survive
  // into a later request after the process has closed.
  await pending.catch(() => undefined);
  await closed;
  await assert.rejects(() => backend.getSessions(), /exited|closed|EPIPE/);
  await backend.stop();
});
