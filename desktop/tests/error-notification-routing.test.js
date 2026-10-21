import assert from "node:assert/strict";
import { act } from "react";
import { test } from "vitest";
import { JSDOM } from "jsdom";

test("interruption polling failures use a stable app notification and resolve on recovery", async () => {
  const dom = new JSDOM("<!doctype html><html><body></body></html>", { url: "http://localhost/" });
  const globals = {
    window: dom.window, document: dom.window.document, Element: dom.window.Element,
    HTMLElement: dom.window.HTMLElement, HTMLInputElement: dom.window.HTMLInputElement,
    CustomEvent: dom.window.CustomEvent, requestAnimationFrame: callback => setTimeout(callback, 0),
    cancelAnimationFrame: clearTimeout, ResizeObserver: class { observe() {} disconnect() {} },
    IS_REACT_ACT_ENVIRONMENT: true,
  };
  const previous = new Map(Object.keys(globals).map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  Object.assign(globalThis, globals);
  try {
    const { mountInterruptionPanel } = await import("../renderer/scripts/components/interruption-panel.tsx");
    const events = [];
    window.addEventListener("rozsa:runtime-notification", event => events.push(event.detail));
    let reads = 0;
    const bridge = {
      async getSessions() { return { ok: true, sessions: [] }; },
      async getInterruptionState() {
        reads += 1;
        if (reads <= 2) throw new Error(reads === 1 ? "provider offline" : "still offline");
        return { approvals: [] };
      },
    };
    const target = document.createElement("div");
    document.body.append(target);
    let panel;
    await act(async () => {
      panel = mountInterruptionPanel(target, bridge);
      panel.setSessionId("session-a");
    });
    await act(async () => waitFor(() => events.filter(item => item.type === "upsert").length === 2, "repeated poll error notifications"));
    const upserts = events.filter(item => item.type === "upsert");
    const upsert = upserts[0];
    assert.equal(upsert.id, "interruption-poll:session-a");
    assert.match(upsert.message, /provider offline/);
    assert.equal(upserts[1].id, upsert.id, "repeated errors must reuse the same notification identity");
    assert.match(upserts[1].message, /still offline/);
    assert.equal(target.querySelector(".interruption-poll-error"), null);
    assert.equal(target.textContent, "");

    await act(async () => waitFor(() => events.some(item => item.type === "resolve" && item.id === upsert.id), "poll recovery resolution"));
    await act(async () => { panel.dispose(); });
  } finally {
    for (const [key, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
    dom.window.close();
  }
});

async function waitFor(predicate, label, timeoutMs = 3500) {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    if (predicate()) return;
    await new Promise(resolve => setTimeout(resolve, 10));
  }
  assert.fail(`Timed out waiting for ${label}`);
}
