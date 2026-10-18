import assert from "node:assert/strict";
import test from "node:test";
import { createElectronAdapter } from "../renderer/runtime/adapters/electron.ts";
import type { ProductBridge, SessionSnapshot } from "../main/session-contract.ts";
import type { MessageEvent } from "../../contracts/ipc/session.ts";

type TestSession = SessionSnapshot & { name: string; streamedText: string };
type MessageEventBody =
  | { kind: "text_delta"; delta: string }
  | { kind: "completed" }
  | { kind: "error"; error: string }
  | { kind: "canceled" };
type TestOperation = {
  operationId: string;
  sessionPath: string;
  prompt: string;
  sequence: number;
  events: MessageEvent[];
  waiters: Array<(event: MessageEvent) => void>;
  running: boolean;
};

function controlledBridge() {
  const sessions = new Map<string, TestSession>([
    ["/sessions/a.jsonl", { path: "/sessions/a.jsonl", catalogId: "a", sessionId: "a", name: "A", cwd: "/workspace", messages: [], contextMessageCount: 0, streamedText: "" }],
    ["/sessions/b.jsonl", { path: "/sessions/b.jsonl", catalogId: "b", sessionId: "b", name: "B", cwd: "/workspace", messages: [], contextMessageCount: 0, streamedText: "" }],
  ]);
  const operations = new Map<string, TestOperation>();
  const subscriptions = new Map<string, TestOperation>();
  const canceled: string[] = [];
  const delivered: string[] = [];
  let activePath = "";
  let operationNumber = 0;
  let subscriptionNumber = 0;

  function publish(operation: TestOperation, event: MessageEventBody): void {
    operation.sequence += 1;
    if (event.kind === "text_delta") sessions.get(operation.sessionPath)!.streamedText += event.delta;
    if (event.kind === "completed" || event.kind === "error" || event.kind === "canceled") {
      operation.running = false;
      const session = sessions.get(operation.sessionPath)!;
      session.messages.push(
        { entryId: `user:${operation.operationId}`, role: "user", content: [{ type: "text", text: operation.prompt }] },
        { entryId: `assistant:${operation.operationId}`, role: "assistant", content: session.streamedText ? [{ type: "text", text: session.streamedText }] : [], errorMessage: event.kind === "error" ? event.error : undefined },
      );
    }
    const sequenced = { ...event, sequence: operation.sequence } as MessageEvent;
    const waiter = operation.waiters.shift();
    if (waiter) waiter(sequenced);
    else operation.events.push(sequenced);
  }

  const bridge = {
    async getSessions() {
      return { ok: true as const, sessions: [...sessions.values()].map(session => ({
        id: session.catalogId, path: session.path, name: session.name, modified: "2026-10-18T00:00:00Z", message_count: session.messages.length,
      })) };
    },
    async switchSession(target: { path: string; catalogId: string }) {
      const session = sessions.get(target.path);
      assert.ok(session, `unknown test session ${target.path}`);
      assert.equal(target.catalogId, session.catalogId);
      activePath = target.path;
      const { name: _name, streamedText: _streamedText, ...snapshot } = session;
      return { ok: true as const, snapshot: structuredClone(snapshot) };
    },
    async startMessage(text: string) {
      assert.ok(activePath, "start_message requires a selected session");
      const operationId = `operation-${++operationNumber}`;
      const operation: TestOperation = { operationId, sessionPath: activePath, prompt: text, sequence: 0, events: [], waiters: [], running: true };
      operations.set(operationId, operation);
      sessions.get(activePath)!.streamedText = "";
      return { operationId };
    },
    async getActiveMessage() {
      const operation = [...operations.values()].find(value => value.sessionPath === activePath && value.running);
      return operation ? { operationId: operation.operationId } : null;
    },
    async subscribeMessageEvents(operationId: string) {
      const operation = operations.get(operationId);
      assert.ok(operation, `unknown operation ${operationId}`);
      const subscriptionId = `subscription-${++subscriptionNumber}`;
      subscriptions.set(subscriptionId, operation);
      return { subscriptionId, operationId };
    },
    async nextMessageEvent(subscriptionId: string) {
      const operation = subscriptions.get(subscriptionId);
      assert.ok(operation, `unknown subscription ${subscriptionId}`);
      const event = operation.events.shift() || await new Promise<MessageEvent>(resolve => operation.waiters.push(resolve));
      delivered.push(`${operation.operationId}:${event.sequence}`);
      return { operationId: operation.operationId, subscriptionId, event };
    },
    async unsubscribeMessageEvents(subscriptionId: string) { subscriptions.delete(subscriptionId); },
    async cancelMessage(operationId: string) {
      const operation = operations.get(operationId);
      assert.ok(operation?.running, `operation ${operationId} is not running`);
      canceled.push(operationId);
      publish(operation, { kind: "canceled" });
    },
  };

  return {
    bridge: bridge as unknown as ProductBridge,
    canceled,
    delivered,
    push(operationId: string, event: MessageEventBody): void {
      const operation = operations.get(operationId);
      assert.ok(operation, `unknown operation ${operationId}`);
      publish(operation, event);
    },
  };
}

async function until(predicate: () => boolean): Promise<void> {
  const deadline = Date.now() + 1500;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await new Promise(resolve => setTimeout(resolve, 5));
  }
  assert.fail("timed out waiting for adapter update");
}

function textOf(state: { messages: Array<{ message: { content: Array<{ type: string; text?: string }> } }> }): string[] {
  return state.messages.flatMap(message => message.message.content.filter(block => block.type === "text").map(block => block.text || ""));
}

test("message deltas and terminal history stay with their originating session across switches", async () => {
  const fixture = controlledBridge();
  const states: Array<{ sessionId: string; isStreaming: boolean; messages: Array<{ entryId: string; message: { content: Array<{ type: string; text?: string }> } }> }> = [];
  const adapter = createElectronAdapter(fixture.bridge);
  const dispose = adapter.subscribe("ui-state", value => states.push(value as typeof states[number]));
  try {
    await adapter.request("get_sessions");
    await adapter.request("switch_session", { path: "/sessions/a.jsonl" });
    await adapter.request("send_message", { message: "prompt A" });
    fixture.push("operation-1", { kind: "text_delta", delta: "A-start" });
    await until(() => fixture.delivered.includes("operation-1:1"));
    await until(() => textOf(states.at(-1)!).some(text => text.includes("A-start")));

    await adapter.request("switch_session", { path: "/sessions/b.jsonl" });
    fixture.push("operation-1", { kind: "text_delta", delta: "A-background" });
    await until(() => fixture.delivered.includes("operation-1:2"));
    assert.equal(states.at(-1)?.sessionId, "b");
    assert.ok(!textOf(states.at(-1)!).some(text => text.includes("A-")), "A's background delta must not appear in B");

    await adapter.request("send_message", { message: "prompt B" });
    fixture.push("operation-2", { kind: "text_delta", delta: "B-output" });
    await until(() => fixture.delivered.includes("operation-2:1") && textOf(states.at(-1)!).some(text => text.includes("B-output")));

    await adapter.request("switch_session", { path: "/sessions/a.jsonl" });
    assert.equal(states.at(-1)?.sessionId, "a");
    assert.ok(textOf(states.at(-1)!).some(text => text.includes("A-startA-background")), "switching back must restore A's in-flight content");
    assert.equal(states.at(-1)?.isStreaming, true);
    await adapter.request("switch_session", { path: "/sessions/b.jsonl" });
    fixture.push("operation-1", { kind: "completed" });
    await until(() => fixture.delivered.includes("operation-1:3"));
    assert.equal(states.at(-1)?.sessionId, "b");
    assert.equal(states.at(-1)?.isStreaming, true, "A's terminal event must not clear B's running state");
    assert.ok(textOf(states.at(-1)!).some(text => text.includes("B-output")));
    assert.ok(!textOf(states.at(-1)!).some(text => text.includes("A-")), "A's terminal event must remain hidden in B");

    await adapter.request("switch_session", { path: "/sessions/a.jsonl" });
    assert.equal(states.at(-1)?.isStreaming, false);
    assert.ok(textOf(states.at(-1)!).includes("A-startA-background"), "switching back must restore A's persisted terminal history");
    assert.ok(!states.at(-1)!.messages.some(message => message.entryId === "stream:operation-1"), "persisted A history must not be duplicated by its stream overlay");
    await adapter.request("switch_session", { path: "/sessions/b.jsonl" });
    fixture.push("operation-2", { kind: "completed" });
    await until(() => fixture.delivered.includes("operation-2:2") && states.at(-1)?.isStreaming === false);
  } finally { dispose(); }
});

test("cancel addresses only the selected session's running operation", async () => {
  const fixture = controlledBridge();
  const states: Array<{ sessionId: string; isStreaming: boolean; messages: Array<{ entryId: string; message: { content: Array<{ type: string; text?: string }> } }> }> = [];
  const adapter = createElectronAdapter(fixture.bridge);
  const dispose = adapter.subscribe("ui-state", value => states.push(value as typeof states[number]));
  try {
    await adapter.request("get_sessions");
    await adapter.request("switch_session", { path: "/sessions/a.jsonl" });
    await adapter.request("send_message", { message: "prompt A" });
    fixture.push("operation-1", { kind: "text_delta", delta: "A-running" });
    await until(() => fixture.delivered.includes("operation-1:1") && textOf(states.at(-1)!).some(text => text.includes("A-running")));

    await adapter.request("switch_session", { path: "/sessions/b.jsonl" });
    await adapter.request("send_message", { message: "prompt B" });
    fixture.push("operation-2", { kind: "text_delta", delta: "B-running" });
    await until(() => fixture.delivered.includes("operation-2:1") && textOf(states.at(-1)!).some(text => text.includes("B-running")));
    await adapter.request("abort");
    await until(() => fixture.delivered.includes("operation-2:2") && states.at(-1)?.sessionId === "b" && states.at(-1)?.isStreaming === false);
    assert.deepEqual(fixture.canceled, ["operation-2"]);

    await adapter.request("switch_session", { path: "/sessions/a.jsonl" });
    assert.equal(states.at(-1)?.isStreaming, true);
    assert.ok(textOf(states.at(-1)!).some(text => text.includes("A-running")));
    fixture.push("operation-1", { kind: "completed" });
    await until(() => fixture.delivered.includes("operation-1:2") && states.at(-1)?.sessionId === "a" && states.at(-1)?.isStreaming === false);
    assert.deepEqual(fixture.canceled, ["operation-2"]);
  } finally { dispose(); }
});
