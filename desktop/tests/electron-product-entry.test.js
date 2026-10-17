import assert from "node:assert/strict";
import test from "node:test";
import { createElectronAdapter } from "../renderer/runtime/adapters/electron.ts";

test("Electron host adapter maps the implemented session slice and reports unsupported business methods", async () => {
  const target = { path: "/sessions/one.jsonl", catalogId: "one" };
  const snapshot = {
    ...target, sessionId: "one", cwd: "/workspace", contextMessageCount: 1,
    messages: [{ entryId: "entry-1", role: "user", content: [{ type: "text", text: "hello" }] }],
  };
  const bridge = {
    async getSessions() { return { ok: true, sessions: [{ id: "one", path: target.path, name: "Hello", modified: "2026-10-17T00:00:00Z", message_count: 1 }] }; },
    async switchSession(value) { assert.deepEqual(value, target); return { ok: true, snapshot }; },
  };
  const adapter = createElectronAdapter(bridge);

  const sessions = await adapter.request("get_sessions");
  assert.equal(sessions[0].path, target.path);
  assert.equal(sessions[0].name, "Hello");
  assert.equal(await adapter.request("switch_session", { path: target.path }), true);
  const state = await adapter.request("get_state");
  assert.equal(state.sessionId, "one");
  assert.equal(state.messages[0].message.content[0].text, "hello");
  await assert.rejects(adapter.request("new_session"), /does not support new_session yet/);
  assert.throws(() => adapter.subscribe("ui-state", () => {}), /event ui-state is not implemented yet/);
});
