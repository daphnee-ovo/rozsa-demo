import assert from "node:assert/strict";
import test from "node:test";
import { createElectronAdapter } from "../renderer/runtime/adapters/electron.ts";

function fixtureBridge(events) {
  const target = { path: "/sessions/one.jsonl", catalogId: "one" };
  const session = { ...target, sessionId: "one", cwd: "/workspace", contextMessageCount: 0, messages: [] };
  let activeOperationId;
  let sequence = 0;
  return {
    async getSessions() {
      return { ok: true, sessions: [{ id: "one", path: target.path, name: "Hello", modified: "2026-10-17T00:00:00Z", message_count: session.messages.length }] };
    },
    async switchSession(value) {
      assert.deepEqual(value, target);
      return { ok: true, snapshot: structuredClone(session) };
    },
    async startMessage(text) {
      assert.equal(text, "hello");
      activeOperationId = "operation-1";
      return { operationId: activeOperationId };
    },
    async getActiveMessage() { return activeOperationId ? { operationId: activeOperationId } : null; },
    async subscribeMessageEvents(operationId) {
      assert.equal(operationId, activeOperationId);
      return { subscriptionId: "subscription-1", operationId };
    },
    async nextMessageEvent(subscriptionId) {
      assert.equal(subscriptionId, "subscription-1");
      const event = events.shift();
      assert.ok(event, "expected a queued message event");
      sequence++;
      if (["completed", "error", "canceled"].includes(event.kind)) {
        activeOperationId = undefined;
        session.messages = [
          { entryId: "entry-user", role: "user", content: [{ type: "text", text: "hello" }] },
          { entryId: "entry-assistant", role: "assistant", content: [{ type: "text", text: event.kind === "completed" ? "reply" : "partial" }], errorMessage: event.kind === "error" ? event.error : undefined },
        ];
      }
      return { operationId: "operation-1", subscriptionId, event: { ...event, sequence } };
    },
    async unsubscribeMessageEvents() {},
    async cancelMessage(operationId) { assert.equal(operationId, "operation-1"); },
  };
}

async function until(predicate) {
  const deadline = Date.now() + 1000;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await new Promise(resolve => setTimeout(resolve, 5));
  }
  assert.fail("timed out waiting for adapter update");
}

test("Electron adapter connects message start, ordered stream, persisted history and terminal errors", async () => {
  const states = [];
  const adapter = createElectronAdapter(fixtureBridge([
    { kind: "text_delta", delta: "partial" },
    { kind: "completed" },
  ]));
  const dispose = adapter.subscribe("ui-state", value => states.push(value));
  try {
    await adapter.request("get_sessions");
    await adapter.request("switch_session", { path: "/sessions/one.jsonl" });
    await adapter.request("send_message", { message: "hello" });
    await until(() => states.some(state => state.messages.some(message => message.entryId === "stream:operation-1" && message.message.content[0]?.text === "partial")));
    await until(() => states.some(state => state.messages.some(message => message.entryId === "entry-assistant" && message.message.content[0]?.text === "reply")));
    assert.ok(states.findIndex(state => state.messages.some(message => message.entryId === "stream:operation-1")) < states.length - 1);
    assert.equal(states.at(-1).isStreaming, false);
    assert.equal(states.at(-1).messages.at(-1).message.content[0].text, "reply");
  } finally { dispose(); }
});

test("Electron adapter keeps error and cancellation terminal events visible", async () => {
  for (const event of [{ kind: "error", error: "provider failed" }, { kind: "canceled" }]) {
    const states = [], errors = [];
    const adapter = createElectronAdapter(fixtureBridge([event]));
    const disposeState = adapter.subscribe("ui-state", value => states.push(value));
    const disposeError = adapter.subscribe("error", value => errors.push(value));
    try {
      await adapter.request("get_sessions");
      await adapter.request("switch_session", { path: "/sessions/one.jsonl" });
      await adapter.request("send_message", { message: "hello" });
      await until(() => states.some(state => state.isStreaming === false && state.messages.some(message => message.entryId === "entry-assistant")));
      if (event.kind === "error") {
        assert.deepEqual(errors, ["provider failed"]);
        assert.equal(states.at(-1).messages.at(-1).message.errorMessage, "provider failed");
      } else assert.equal(states.at(-1).isStreaming, false);
    } finally { disposeState(); disposeError(); }
  }
});
