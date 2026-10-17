import assert from "node:assert/strict";
import { test } from "node:test";
import { EventEmitter } from "node:events";
import type { BrowserWindow, IpcMain, IpcMainInvokeEvent } from "electron";
import type { BackendService } from "../main/backend-service.ts";
import { installSessionBridge } from "../main/session-bridge.ts";
import { RozsaBridge } from "../preload/bridge.ts";
import { ProductClient } from "../renderer/product-client.ts";
import { ProductFailure, SESSION_LIST_CHANNEL, SESSION_HISTORY_CHANNEL, SESSION_SUBSCRIBE_CHANNEL, SESSION_NEXT_CHANNEL, type SessionSnapshot } from "../main/session-contract.ts";
import type { SessionUpdate } from "../../contracts/ipc/session.ts";
import { setTimeout as delay } from "node:timers/promises";
import { JSDOM } from "jsdom";
import { SessionHistory } from "../renderer/features/session-history.ts";

const target = { path: "/synthetic/a.jsonl", catalogId: "header-a" };
const active: SessionSnapshot = { ...target, sessionId: "a", cwd: "/synthetic", messages: [], contextMessageCount: 0 };
const snapshot = (id: string, watermark = 0): SessionUpdate => ({ subscriptionId: id, snapshot: { epoch: "epoch", watermark, active } });
function deferred<T>() { return Promise.withResolvers<T>(); }
function host(backend: Partial<BackendService>) {
  const url = "file:///synthetic/index.html", webContents = Object.assign(new EventEmitter(), { mainFrame: { url } });
  const window = Object.assign(new EventEmitter(), { webContents, isDestroyed: () => false });
  const handlers = new Map<string, (event: IpcMainInvokeEvent, ...args: unknown[]) => Promise<any>>();
  const ipc = { handle: (channel: string, handler: any) => { handlers.set(channel, handler); }, removeHandler: (channel: string) => handlers.delete(channel) };
  installSessionBridge(window as unknown as BrowserWindow, ipc as unknown as IpcMain, backend as BackendService, url);
  const event = { sender: webContents, senderFrame: webContents.mainFrame } as unknown as IpcMainInvokeEvent;
  const invoke = async (channel: string, ...args: unknown[]) => {
    const handler = handlers.get(channel); if (!handler) throw new Error("No handler"); return handler(event, ...args);
  };
  return { window, webContents, handlers, event, invoke, bridge: RozsaBridge.create(invoke) };
}

test("main rejects foreign senders, subframes, destinations and arity before dispatch", async () => {
  let calls = 0;
  const ui = host({ getSessions: async () => { calls++; return []; }, switchSession: async () => { calls++; return active; } });
  const handle = ui.handlers.get(SESSION_LIST_CHANNEL)!;
  for (const event of [
    { ...ui.event, sender: {} },
    { ...ui.event, senderFrame: { url: ui.webContents.mainFrame.url } },
    { ...ui.event, senderFrame: null },
  ]) await assert.rejects(handle(event as IpcMainInvokeEvent), /Unauthorized/);
  ui.webContents.mainFrame.url = "https://untrusted.example/";
  await assert.rejects(handle(ui.event), /Unauthorized/);
  ui.webContents.mainFrame.url = "file:///synthetic/index.html";
  await assert.rejects(handle(ui.event, "/forbidden"), /Unauthorized/);
  const invalid = await ui.invoke(SESSION_HISTORY_CHANNEL, { ...target, arbitrary: true });
  assert.equal(invalid.error.kind, "invalid_params"); assert.equal(calls, 0);
  assert.deepEqual(await ui.invoke(SESSION_LIST_CHANNEL), { ok: true, sessions: [] }); assert.equal(calls, 1);
  ui.window.emit("closed"); assert.equal(ui.handlers.size, 0);
});

test("preload exposes validated session and message capabilities and rejects malformed synchronization", async () => {
  const calls: unknown[][] = [];
  const bridge = RozsaBridge.create(async (...args) => { calls.push(args); return { ok: true, update: snapshot("1") }; });
  assert.ok(Object.isFrozen(bridge));
  assert.deepEqual(Object.keys(bridge), ["getSessions", "switchSession", "subscribeSessions", "nextSessionEvent", "unsubscribeSessions", "startMessage", "continueMessage", "getActiveMessage", "subscribeMessageEvents", "nextMessageEvent", "unsubscribeMessageEvents", "cancelMessage"]);
  await assert.rejects((bridge.subscribeSessions as Function)("arbitrary"));
  await assert.rejects((bridge.nextSessionEvent as Function)("1", "extra"));
  await assert.rejects((bridge.nextSessionEvent as Function)({ subscriptionId: "1" }));
  await assert.rejects((bridge.unsubscribeSessions as Function)(""));
  assert.equal(calls.length, 0);
  await bridge.subscribeSessions(); await bridge.nextSessionEvent("1");
  assert.deepEqual(calls, [[SESSION_SUBSCRIBE_CHANNEL], [SESSION_NEXT_CHANNEL, "1"]]);
  for (const update of [
    { ...snapshot("2") },
    { subscriptionId: "1", snapshot: { epoch: "epoch", watermark: -1, active } },
    { subscriptionId: "1", snapshot: { epoch: "epoch", watermark: 0, active }, event: { epoch: "epoch", sequence: 1, active } },
    { subscriptionId: "1", event: { epoch: "epoch", sequence: 1, active: { ...active, messages: null } } },
  ]) await assert.rejects(RozsaBridge.create(async () => ({ ok: true, update })).nextSessionEvent("1"));
});

test("reload replaces the subscription and discards old list, snapshot and event delivery", async () => {
  const initial = deferred<SessionUpdate>(), event = deferred<SessionUpdate>(), list = deferred<[]>();
  let subscriptions = 0;
  const released: string[] = [];
  const ui = host({
    getSessions: () => list.promise,
    subscribeSessions: () => ++subscriptions === 1 ? initial.promise : Promise.resolve(snapshot(String(subscriptions))),
    nextSessionEvent: () => event.promise,
    unsubscribeSessions: async id => { released.push(id); },
  });
  const oldList = ui.bridge.getSessions(), oldSnapshot = ui.bridge.subscribeSessions();
  ui.webContents.emit("did-start-navigation", {}, "file:///synthetic/index.html", false, true);
  const replacement = await ui.bridge.subscribeSessions(); assert.equal(replacement.ok, true);
  initial.resolve(snapshot("1")); list.resolve([]);
  assert.equal((await oldSnapshot).ok, false); assert.equal((await oldList).ok, false);
  const pending = ui.bridge.nextSessionEvent("2");
  ui.webContents.emit("did-start-navigation", {}, "file:///synthetic/index.html", false, true);
  await ui.bridge.subscribeSessions();
  event.resolve({ subscriptionId: "2", event: { epoch: "epoch", sequence: 1, active } });
  assert.equal((await pending).ok, false);
  assert.equal((await ui.bridge.nextSessionEvent("2")).ok, false);
  assert.deepEqual(released, ["1", "2"]);
  ui.webContents.emit("render-process-gone", {}, { reason: "crashed" });
  assert.deepEqual(released, ["1", "2", "3"]);
  ui.window.emit("closed"); assert.equal(ui.handlers.size, 0);
});

test("renderer accepts snapshot then consecutive events, rejects stale delivery and resynchronizes reads only", async () => {
  const calls: unknown[][] = [], delivered: (SessionSnapshot | null)[] = [], failures: boolean[] = [];
  const signal = new AbortController(); let subscribed = 0, next = 0;
  const updates: unknown[] = [
    { ok: true, update: { subscriptionId: "1", event: { epoch: "old", sequence: 9, active } } },
    { ok: true, update: { subscriptionId: "1", event: { epoch: "epoch", sequence: 1, active } } },
    { ok: true, update: { subscriptionId: "1", event: { epoch: "epoch", sequence: 1, active } } },
    { ok: false, error: { kind: "timeout", code: -32004, message: "Idle" } },
    { ok: false, error: { kind: "resync_required", code: -32007, message: "Overflow" } },
  ];
  const bridge = RozsaBridge.create(async (channel, ...args) => {
    calls.push([channel, ...args]);
    if (channel === SESSION_SUBSCRIBE_CHANNEL) return { ok: true, update: snapshot(String(++subscribed), subscribed === 1 ? 0 : 2) };
    if (channel === SESSION_NEXT_CHANNEL) { if (next < updates.length) return updates[next++]; return new Promise(() => {}); }
    return { ok: true };
  });
  const sync = new ProductClient(bridge).syncSessions(value => { delivered.push(value); if (delivered.length === 3) signal.abort(); }, (_, resync) => failures.push(resync), { signal: signal.signal });
  await sync;
  assert.equal(delivered.length, 3); assert.equal(subscribed, 2); assert.deepEqual(failures, [true]);
  assert.equal(calls.filter(([channel]) => channel === SESSION_HISTORY_CHANNEL).length, 0);
  assert.deepEqual(calls.at(-1), ["product:unsubscribe-sessions", "2"]);
});

test("explicit UI abort stops sync reception and unsubscribes without sending a product abort", async () => {
  const next = deferred<unknown>(), calls: string[] = [], signal = new AbortController();
  const client = new ProductClient(RozsaBridge.create(async channel => {
    calls.push(channel);
    if (channel === SESSION_SUBSCRIBE_CHANNEL) return { ok: true, update: snapshot("1") };
    if (channel === SESSION_NEXT_CHANNEL) return next.promise;
    return { ok: true };
  }));
  const delivered: unknown[] = [], failures: unknown[] = [];
  const sync = client.syncSessions(value => delivered.push(value), error => failures.push(error), { signal: signal.signal });
  await delay(0); signal.abort(); await sync;
  next.resolve({ ok: true, update: { subscriptionId: "1", event: { epoch: "epoch", sequence: 1, active } } }); await delay(0);
  assert.equal(delivered.length, 1); assert.equal(failures.length, 0);
  assert.deepEqual(calls, [SESSION_SUBSCRIBE_CHANNEL, SESSION_NEXT_CHANNEL, "product:unsubscribe-sessions"]);
  const aborted = new AbortController(); aborted.abort();
  await client.syncSessions(() => assert.fail("delivery after abort"), () => assert.fail("report after abort"), { signal: aborted.signal });
  assert.equal(calls.length, 3);
});

test("authoritative snapshot blocks late success without hiding the pending user's failure", async () => {
  for (const failed of [false, true]) {
    const dom = new JSDOM('<h2></h2><p></p><button hidden></button><div></div>');
    const document = dom.window.document, receiving = deferred<SessionSnapshot>();
    const container = document.querySelector("div")!, status = document.querySelector("p")!, retry = document.querySelector("button")!;
    const feature = new SessionHistory({ switchSession: () => receiving.promise }, container, document.querySelector("h2")!, status, retry);
    try {
      const pending = feature.switchTo({ id: target.catalogId, path: target.path, name: "A", modified: "t", message_count: 0 });
      const newer = { ...active, path: "/synthetic/b.jsonl", sessionId: "b", catalogId: "header-b" };
      feature.restore(newer);
      if (failed) receiving.reject(new Error("Reader failed")); else receiving.resolve(active);
      await pending;
      assert.equal(container.dataset.sessionId, "b");
      if (failed) { assert.match(status.textContent!, /Reader failed/); assert.equal(retry.hidden, false); }
      else { assert.equal(retry.hidden, true); assert.doesNotMatch(status.textContent!, /Loading/); }
      feature.restore(null); assert.equal(container.children.length, 0); assert.equal(container.dataset.sessionId, undefined);
    } finally { dom.window.close(); }
  }
});

test("renderer reports a sequence gap before a fresh snapshot and rejects a regressed snapshot", async () => {
  for (const regressed of [false, true]) {
    const signal = new AbortController(), deliveries: unknown[] = [], failures: boolean[] = [];
    let subscriptions = 0;
    const bridge = RozsaBridge.create(async (channel, id) => {
      if (channel === SESSION_SUBSCRIBE_CHANNEL) return { ok: true, update: snapshot(String(++subscriptions), subscriptions === 1 ? 2 : regressed ? 1 : 4) };
      if (channel === SESSION_NEXT_CHANNEL) return { ok: true, update: { subscriptionId: id, event: { epoch: "epoch", sequence: 4, active } } };
      return { ok: true };
    });
    await new ProductClient(bridge).syncSessions(value => { deliveries.push(value); if (deliveries.length === 2) signal.abort(); }, (_, recovering) => failures.push(recovering), { signal: signal.signal });
    assert.equal(subscriptions, 2);
    assert.equal(deliveries.length, regressed ? 1 : 2);
    assert.deepEqual(failures, regressed ? [true, false] : [true]);
  }
});

test("subscription startup timeout is visible and does not silently retry", async () => {
  let calls = 0; const failures: unknown[] = [];
  const bridge = RozsaBridge.create(async () => { calls++; return { ok: false, error: { kind: "timeout", code: -32004, message: "Snapshot timed out" } }; });
  await new ProductClient(bridge).syncSessions(() => assert.fail("Unexpected snapshot"), error => failures.push(error));
  assert.equal(calls, 1); assert.equal(failures.length, 1);
  assert.ok(failures[0] instanceof ProductFailure); assert.equal(failures[0].failure.kind, "timeout");
});
