import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { getByPlaceholderText, getByRole, getByText } from "@testing-library/dom";
import { test } from "vitest";
import { JSDOM } from "jsdom";
import { installInterruptionBridge } from "../main/interruption-bridge.ts";
import { PERMISSION_RESPOND_CHANNEL } from "../main/session-contract.ts";
import approvalLifecycleFixture from "../../testdata/fixtures/desktop/approval-question-lifecycle.json";

const uiFixtures = approvalLifecycleFixture.ui;

let mountInterruptionPanel;

async function withDom(run) {
  const dom = new JSDOM("<!doctype html><html><body></body></html>", { url: "http://localhost/" });
  const globals = {
    window: dom.window,
    document: dom.window.document,
    Element: dom.window.Element,
    HTMLElement: dom.window.HTMLElement,
    HTMLInputElement: dom.window.HTMLInputElement,
    CustomEvent: dom.window.CustomEvent,
    requestAnimationFrame: (callback) => setTimeout(callback, 0),
    cancelAnimationFrame: clearTimeout,
    ResizeObserver: class {
      observe() {}
      disconnect() {}
    },
  };
  const previous = new Map(
    Object.keys(globals).map((key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)]),
  );
  Object.assign(globalThis, globals);
  try {
    mountInterruptionPanel ||= (
      await import("../renderer/scripts/components/interruption-panel.tsx")
    ).mountInterruptionPanel;
    await run();
    await new Promise((resolve) => setTimeout(resolve, 50));
  } finally {
    for (const [key, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
    dom.window.close();
  }
}

function createBridge(states) {
  const calls = { permissions: [], answers: [], cancellations: [] };
  let permissionFailure;
  return {
    calls,
    failNextPermission(message = "injected response failure") {
      permissionFailure = new Error(message);
    },
    async getSessions() {
      return { ok: true, sessions: [] };
    },
    async getInterruptionState(sessionId) {
      return structuredClone(states[sessionId] || { approvals: [] });
    },
    async respondPermission(response) {
      calls.permissions.push(response);
      if (permissionFailure) {
        const failure = permissionFailure;
        permissionFailure = undefined;
        throw failure;
      }
      const state = states[response.sessionId];
      if (!state?.approvals.some((item) => item.requestId === response.requestId))
        throw new Error("stale permission request");
      state.approvals = state.approvals.filter((item) => item.requestId !== response.requestId);
      if (state.approvals[0]?.status === "queued") state.approvals[0].status = "pending";
    },
    async respondUserQuestion(requestId, answers) {
      calls.answers.push({ requestId, answers });
      const state = states["session-a"];
      if (state?.question?.requestId !== requestId) throw new Error("stale user question");
      state.question = undefined;
    },
    async cancelUserQuestion(requestId) {
      calls.cancellations.push(requestId);
      const state = states["session-a"];
      if (state?.question?.requestId !== requestId) throw new Error("stale user question");
      state.question = undefined;
    },
  };
}

async function waitFor(predicate, label, timeoutMs = 2500) {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  assert.fail(`Timed out waiting for ${label}`);
}

function mount(bridge) {
  document.body.replaceChildren();
  const target = document.createElement("div");
  document.body.append(target);
  const panel = mountInterruptionPanel(target, bridge);
  panel.setSessionId("session-a");
  return { target, panel };
}

function setText(input, value) {
  const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(input), "value").set;
  setter.call(input, value);
  input.dispatchEvent(new window.Event("input", { bubbles: true }));
}

function createInterruptionHost(backend) {
  const trustedURL = "file:///synthetic/index.html";
  const webContents = Object.assign(new EventEmitter(), { mainFrame: { url: trustedURL } });
  const window = Object.assign(new EventEmitter(), { webContents, isDestroyed: () => false });
  const handlers = new Map();
  const ipc = {
    handle(channel, handler) {
      handlers.set(channel, handler);
    },
    removeHandler(channel) {
      handlers.delete(channel);
    },
  };
  installInterruptionBridge(window, ipc, backend, trustedURL);
  const event = { sender: webContents, senderFrame: webContents.mainFrame };
  return { window, webContents, handlers, event };
}

function deferred() {
  let resolve;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test("failed permission responses stay actionable without exposing a retry state; queued requests advance only after confirmation", async () =>
  withDom(async () => {
    const states = {
      "session-a": {
        approvals: [
          structuredClone(uiFixtures.approvals.failed),
          structuredClone(uiFixtures.approvals.queued),
        ],
      },
    };
    const bridge = createBridge(states);
    const runtimeNotifications = [];
    window.upsertNotification = (notification) =>
      runtimeNotifications.push({ type: "upsert", ...notification });
    window.resolveNotification = (id) => runtimeNotifications.push({ type: "resolve", id });
    bridge.failNextPermission();
    const mounted = mount(bridge);
    try {
      await waitFor(
        () => document.querySelector("#permPanel")?.dataset.requestId === "approval-a",
        "first approval",
      );
      assert.doesNotMatch(document.querySelector("#permPanel").textContent, /failed|retry/i);
      document.querySelector('[data-permission-action="allow"]').click();
      await waitFor(
        () =>
          runtimeNotifications.some(
            (item) =>
              item.type === "upsert" &&
              item.id === "interruption-action:permission:session-a:approval-a",
          ),
        "application error notification",
      );
      assert.match(
        runtimeNotifications.find((item) => item.type === "upsert").message,
        /injected response failure/,
      );
      assert.doesNotMatch(
        document.querySelector("#permPanel").textContent,
        /injected response failure/,
      );
      assert.equal(
        document.querySelector("#permPanel")?.dataset.requestId,
        "approval-a",
        "failure must retain the approval",
      );
      document.querySelector('[data-permission-action="allow"]').click();
      await waitFor(
        () => document.querySelector("#permPanel")?.dataset.requestId === "approval-b",
        "next queued approval",
      );
      assert.equal(states["session-a"].approvals[0].status, "pending");
      document.querySelector('[data-permission-action="allow"]').click();
      await waitFor(
        () => !document.querySelector("#permPanel.visible"),
        "confirmed queue completion",
      );
      assert.ok(
        runtimeNotifications.some(
          (item) =>
            item.type === "resolve" &&
            item.id === "interruption-action:permission:session-a:approval-a",
        ),
        "successful retry must resolve its error notification",
      );
      assert.deepEqual(
        bridge.calls.permissions.map((item) => item.requestId),
        ["approval-a", "approval-a", "approval-b"],
      );
    } finally {
      mounted.panel.dispose();
    }
  }));

test("responding requests disable approval actions without exposing a submitting state", async () =>
  withDom(async () => {
    const bridge = createBridge({
      "session-a": { approvals: [structuredClone(uiFixtures.approvals.responding)] },
    });
    const mounted = mount(bridge);
    try {
      await waitFor(() => Boolean(document.querySelector("#permPanel")), "responding approval");
      assert.doesNotMatch(document.querySelector("#permPanel").textContent, /Applying response/i);
      assert.equal(document.querySelector('[data-permission-action="allow"]').disabled, true);
    } finally {
      mounted.panel.dispose();
    }
  }));

test("permission keyboard shortcuts open trust and submit a trust level", async () =>
  withDom(async () => {
    const bridge = createBridge({
      "session-a": { approvals: [structuredClone(uiFixtures.approvals.keyboard)] },
    });
    const mounted = mount(bridge);
    try {
      await waitFor(() => Boolean(document.querySelector("#permPanel")), "keyboard approval");
      document
        .querySelector("#permPanel")
        .dispatchEvent(
          new window.KeyboardEvent("keydown", { key: "t", bubbles: true, cancelable: true }),
        );
      await waitFor(
        () => Boolean(document.querySelector("#permTrustActions")),
        "keyboard trust page",
      );
      getByRole(document, "button", { name: "Back" }).click();
      await waitFor(() => Boolean(document.querySelector("#permPanelMain")), "trust back action");
      document.querySelector('[data-permission-action="trust"]').click();
      await waitFor(
        () => Boolean(document.querySelector("#permTrustActions")),
        "trust page reopened",
      );
      document
        .querySelector("#permTrustActions")
        .dispatchEvent(
          new window.KeyboardEvent("keydown", { key: "1", bubbles: true, cancelable: true }),
        );
      await waitFor(() => !document.querySelector("#permPanel.visible"), "keyboard trust response");
      assert.deepEqual(bridge.calls.permissions, [
        {
          sessionId: "session-a",
          requestId: "approval-keyboard",
          choice: "allow-project",
          trustKeys: ["Write:/workspace/a.txt"],
        },
      ]);
    } finally {
      mounted.panel.dispose();
    }
  }));

test("permission keyboard shortcuts apply allow, hint, and direct deny responses", async () =>
  withDom(async () => {
    const states = {
      "session-a": { approvals: [structuredClone(uiFixtures.approvals.allow)] },
    };
    const bridge = createBridge(states);
    const mounted = mount(bridge);
    try {
      await waitFor(() => Boolean(document.querySelector("#permPanel")), "allow shortcut approval");
      document
        .querySelector("#permPanel")
        .dispatchEvent(
          new window.KeyboardEvent("keydown", { key: "y", bubbles: true, cancelable: true }),
        );
      await waitFor(() => !document.querySelector("#permPanel.visible"), "allow shortcut response");
      states["session-a"].approvals = [structuredClone(uiFixtures.approvals.hint)];
      await waitFor(
        () => document.querySelector("#permPanel")?.dataset.requestId === "approval-hint",
        "hint shortcut approval",
      );
      document
        .querySelector("#permPanel")
        .dispatchEvent(
          new window.KeyboardEvent("keydown", { key: "h", bubbles: true, cancelable: true }),
        );
      await waitFor(
        () => Boolean(document.querySelector("[data-permission-hint]")),
        "hint shortcut page",
      );
      setText(document.querySelector("[data-permission-hint]"), "Deny, inspect first");
      document
        .querySelector("[data-permission-hint]")
        .dispatchEvent(
          new window.KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }),
        );
      await waitFor(() => !document.querySelector("#permPanel.visible"), "hint shortcut response");
      states["session-a"].approvals = [structuredClone(uiFixtures.approvals.deny)];
      await waitFor(
        () => document.querySelector("#permPanel")?.dataset.requestId === "approval-deny",
        "deny shortcut approval",
      );
      document
        .querySelector("#permPanel")
        .dispatchEvent(
          new window.KeyboardEvent("keydown", { key: "n", bubbles: true, cancelable: true }),
        );
      await waitFor(() => !document.querySelector("#permPanel.visible"), "deny shortcut response");
      assert.deepEqual(
        bridge.calls.permissions.map(({ requestId, choice }) => ({ requestId, choice })),
        [
          { requestId: "approval-allow", choice: "allow" },
          { requestId: "approval-hint", choice: "deny-hint" },
          { requestId: "approval-deny", choice: "deny" },
        ],
      );
    } finally {
      mounted.panel.dispose();
    }
  }));

test("session switching restores the pending request and its hint page", async () =>
  withDom(async () => {
    const states = {
      "session-a": { approvals: [structuredClone(uiFixtures.approvals.sessionSwitch)] },
      "session-b": { approvals: [] },
    };
    const bridge = createBridge(states);
    const mounted = mount(bridge);
    try {
      await waitFor(() => Boolean(document.querySelector("#permPanel")), "session A approval");
      document.querySelector('[data-permission-action="hint"]').click();
      await waitFor(
        () => Boolean(document.querySelector("[data-permission-hint]")),
        "session A hint page",
      );
      const hint = document.querySelector("[data-permission-hint]");
      setText(hint, "Deny, use Read for inspection");
      mounted.panel.setSessionId("session-b");
      await waitFor(() => !document.querySelector("#permPanel.visible"), "session B panel hidden");
      mounted.panel.setSessionId("session-a");
      await waitFor(
        () =>
          document.querySelector("#permPanelHint") &&
          !document.querySelector("#permHintInput")?.disabled,
        "session A hint restored",
      );
      assert.equal(document.querySelector("#permHintInput").value, "Deny, use Read for inspection");
      assert.equal(document.activeElement, document.querySelector("#permHintInput"));
    } finally {
      mounted.panel.dispose();
    }
  }));

test("session switching restores the selected interruption and question input", async () =>
  withDom(async () => {
    const states = {
      "session-a": { approvals: [structuredClone(uiFixtures.approvals.sessionA)] },
      "session-b": {
        approvals: [],
        question: structuredClone(uiFixtures.questions.sessionB),
      },
    };
    const mounted = mount(createBridge(states));
    try {
      await waitFor(
        () => document.querySelector("#permPanel")?.dataset.requestId === "approval-session-a",
        "session A permission",
      );
      mounted.panel.setSessionId("session-b");
      await waitFor(
        () => document.querySelector("#questionPanel")?.dataset.requestId === "question-session-b",
        "session B question",
      );
      assert.equal(
        document.querySelector("#permPanel"),
        null,
        "only the selected session's interruption is rendered",
      );
      getByRole(document, "heading", { name: /Choose a mode/ });
      getByRole(document, "radio", { name: /Other/ }).click();
      const answer = getByPlaceholderText(document, "Type your own answer…");
      setText(answer, "Keep this answer");

      mounted.panel.setSessionId("session-a");
      await waitFor(
        () => document.querySelector("#permPanel")?.dataset.requestId === "approval-session-a",
        "session A permission restored",
      );
      assert.equal(document.querySelector("#questionPanel"), null);
      mounted.panel.setSessionId("session-b");
      await waitFor(
        () => document.querySelector("#questionPanel")?.dataset.requestId === "question-session-b",
        "session B question restored",
      );
      assert.equal(getByRole(document, "radio", { name: /Other/ }).checked, true);
      assert.equal(
        getByPlaceholderText(document, "Type your own answer…").value,
        "Keep this answer",
      );
    } finally {
      mounted.panel.dispose();
    }
  }));

test("Bash approval keeps its prompt outside highlighted source when switching sessions", async () =>
  withDom(async () => {
    const shortApproval = structuredClone(uiFixtures.bashApprovals.short);
    const multilineApproval = structuredClone(uiFixtures.bashApprovals.multiline);
    const states = {
      "session-a": { approvals: [shortApproval] },
      "session-b": { approvals: [multilineApproval] },
    };
    const mounted = mount(createBridge(states));
    const assertBashCommand = (requestId, command) => {
      const panel = document.querySelector("#permPanel");
      assert.equal(panel?.dataset.requestId, requestId);
      const code = panel?.querySelector(".fold-full code.md-highlighted-code.language-bash");
      assert.ok(code, "Bash source uses the shared highlighter");
      assert.equal(code.textContent, command, "the command stays readable without its prompt");
      assert.equal(code.previousElementSibling?.classList.contains("perm-syn-prompt"), true);
      assert.equal(code.previousElementSibling?.textContent, "$ ");
    };
    try {
      await waitFor(
        () => document.querySelector("#permPanel")?.dataset.requestId === "bash-short",
        "short Bash approval",
      );
      assertBashCommand("bash-short", shortApproval.command);

      mounted.panel.setSessionId("session-b");
      await waitFor(
        () => document.querySelector("#permPanel")?.dataset.requestId === "bash-multiline",
        "multiline Bash approval",
      );
      assertBashCommand("bash-multiline", multilineApproval.command);

      mounted.panel.setSessionId("session-a");
      await waitFor(
        () => document.querySelector("#permPanel")?.dataset.requestId === "bash-short",
        "short Bash approval restored",
      );
      assertBashCommand("bash-short", shortApproval.command);
    } finally {
      mounted.panel.dispose();
    }
  }));

test("renderer navigation invalidates an in-flight permission response", async () => {
  const reply = deferred();
  const ui = createInterruptionHost({ respondPermission: async () => reply.promise });
  const handler = ui.handlers.get(PERMISSION_RESPOND_CHANNEL);
  const pending = handler(ui.event, {
    sessionId: "session-a",
    requestId: "approval-a",
    choice: "allow",
  });
  ui.webContents.emit("did-start-navigation", {}, "file:///replacement.html", false, true);
  reply.resolve();
  const result = await pending;
  assert.equal(result.ok, false);
  assert.equal(result.error.reason, "closed");
  ui.window.emit("closed");
});

test("window close invalidates pending responses and removes interruption handlers", async () => {
  const reply = deferred();
  const ui = createInterruptionHost({ respondPermission: async () => reply.promise });
  const handler = ui.handlers.get(PERMISSION_RESPOND_CHANNEL);
  const pending = handler(ui.event, {
    sessionId: "session-a",
    requestId: "approval-a",
    choice: "allow",
  });
  ui.window.emit("closed");
  assert.equal(ui.handlers.size, 0);
  reply.resolve();
  const result = await pending;
  assert.equal(result.ok, false);
  assert.equal(result.error.reason, "closed");
});

test("multi-question flow supports Next, checkbox Other, and cancellation rejects late answers", async () =>
  withDom(async () => {
    const states = {
      "session-a": { approvals: [], question: structuredClone(uiFixtures.questions.default) },
    };
    const bridge = createBridge(states);
    const mounted = mount(bridge);
    try {
      await waitFor(() => Boolean(document.querySelector("#questionPanel")), "first question");
      document.querySelector('#questionPanelOptions input[data-other="true"]').click();
      setText(document.querySelector("[data-question-other]"), "Custom mode");
      document.querySelector("#questionPanelSubmit").click();
      await waitFor(
        () =>
          document.querySelector("#questionPanelTitle")?.textContent.includes("Choose features"),
        "Next question",
      );
      document.querySelector('#questionPanelOptions input[value="Tests"]').click();
      document.querySelector('#questionPanelOptions input[data-other="true"]').click();
      setText(document.querySelector("[data-question-other]"), "Review");
      document.querySelector("#questionPanelSubmit").click();
      await waitFor(
        () => !document.querySelector("#questionPanel.visible"),
        "question response confirmation",
      );
      assert.deepEqual(bridge.calls.answers, [
        {
          requestId: "question-a",
          answers: { Mode: "Custom mode", Features: ["Tests", "Review"] },
        },
      ]);

      states["session-a"].question = structuredClone(uiFixtures.questions.cancel);
      await waitFor(
        () => document.querySelector("#questionPanel")?.dataset.requestId === "question-cancel",
        "second question",
      );
      assert.ok(getByRole(document, "button", { name: "Cancel" }));
      const lateId = document.querySelector("#questionPanel").dataset.requestId;
      document
        .querySelector("#questionPanel")
        .dispatchEvent(
          new window.KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }),
        );
      await waitFor(
        () => !document.querySelector("#questionPanel.visible"),
        "question cancellation",
      );
      await assert.rejects(
        bridge.respondUserQuestion(lateId, { Mode: "late" }),
        /stale user question/,
      );
      assert.deepEqual(bridge.calls.cancellations, ["question-cancel"]);
    } finally {
      mounted.panel.dispose();
    }
  }));
