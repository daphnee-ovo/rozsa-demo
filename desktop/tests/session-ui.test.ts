import { afterEach, describe, expect, it } from "vitest";
import { JSDOM } from "jsdom";
import { fireEvent, getByRole, getByText } from "@testing-library/dom";
import type { ProductBridge, SessionSnapshot, SessionTarget } from "../main/session-contract.ts";
import { createElectronAdapter } from "../renderer/runtime/adapters/electron.ts";
import { init, sessionsMarkup } from "../renderer/scripts/features/sidebar.js";

let dom: JSDOM | undefined;
afterEach(() => {
  dom?.window.close();
  dom = undefined;
});

describe("current session UI", () => {
  it("keeps unsafe session names as text and lets users choose sessions by click or keyboard", () => {
    dom = new JSDOM('<aside id="sidebar"><div id="sessionList"></div></aside>');
    const document = dom.window.document;
    const list = document.querySelector("#sessionList")!;
    list.innerHTML = sessionsMarkup([
      { name: '<img src=x onerror="window.pwned=true">', date: "2d", activity: "idle", active: false },
      { name: "Project session", date: "1d", activity: "running", active: true },
    ]);
    const selected: number[] = [];
    init({
      boundModules: new Set(),
      byId: (id: string) => document.getElementById(id),
      activateSession: (index: number) => selected.push(index),
      newSession() {}, toggleSettings() {}, openDevFlowDashboard() {},
    } as never);

    const projectChoice = getByRole(list, "button", { name: /Project session/ });
    const unsafeName = getByText(list, '<img src=x onerror="window.pwned=true">', { exact: true });
    expect(list.querySelector("img")).toBeNull();
    fireEvent.click(projectChoice);
    fireEvent.keyDown(unsafeName, { key: "Enter" });
    expect(selected).toEqual([1, 0]);
  });

  it("keeps the last visible session after delayed success and failure in A→B→A selection", async () => {
    dom = new JSDOM('<aside id="sidebar"><div id="sessionList"></div></aside><h2 id="currentSession"></h2><div id="conversation" role="log"></div>');
    const document = dom.window.document;
    const list = document.querySelector<HTMLElement>("#sessionList")!;
    const currentSession = document.querySelector<HTMLElement>("#currentSession")!;
    const conversation = document.querySelector<HTMLElement>("#conversation")!;
    const catalog = [
      { id: "alpha", path: "/sessions/alpha.jsonl", name: "Alpha session", modified: "2026-10-21", message_count: 1 },
      { id: "beta", path: "/sessions/beta.jsonl", name: "Beta session", modified: "2026-10-21", message_count: 1 },
    ];
    const pending: Array<{ target: SessionTarget; resolve(value: { ok: true; snapshot: SessionSnapshot }): void; reject(error: Error): void }> = [];
    const bridge = {
      getSessions: async () => ({ ok: true, sessions: catalog }),
      switchSession: (target: SessionTarget) => new Promise<{ ok: true; snapshot: SessionSnapshot }>((resolve, reject) => pending.push({ target, resolve, reject })),
    } as unknown as ProductBridge;
    const adapter = createElectronAdapter(bridge);
    await adapter.request("get_sessions");
    const selectionResults: Promise<unknown>[] = [];
    const reportState = (value: unknown) => {
      const state = value as { sessionName: string; messages: Array<{ message: { content: Array<{ text?: string }> } }> };
      currentSession.textContent = state.sessionName;
      conversation.textContent = state.messages.flatMap(message => message.message.content.map(block => block.text || "")).join("\n");
    };
    const unsubscribe = adapter.subscribe("ui-state", reportState);
    list.innerHTML = sessionsMarkup(catalog.map(entry => ({ name: entry.name, date: "today", activity: "idle", active: false })));
    init({
      boundModules: new Set(),
      byId: (id: string) => document.getElementById(id),
      activateSession: (index: number) => {
        const request = adapter.request("switch_session", { path: catalog[index].path });
        selectionResults.push(request);
        void request.catch(() => undefined);
      },
      newSession() {}, toggleSettings() {}, openDevFlowDashboard() {},
    } as never);

    const select = (name: string) => fireEvent.click(getByRole(list, "button", { name: new RegExp(name) }));
    const waitForPending = async (count: number) => {
      const deadline = Date.now() + 1000;
      while (pending.length < count && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 0));
      expect(pending).toHaveLength(count);
    };
    const snapshot = (target: SessionTarget, entryId: string, contentText: string, displayText: string): SessionSnapshot => ({
      ...target,
      sessionId: target.catalogId,
      cwd: "/workspace",
      contextMessageCount: 1,
      messages: [{ entryId, role: "user", displayText, content: [{ type: "text", text: contentText }] }],
    });
    try {
      select("Alpha session");
      await waitForPending(1);
      select("Beta session");
      select("Alpha session");
      const alphaTarget = { path: catalog[0].path, catalogId: "alpha" };
      pending[0].resolve({ ok: true, snapshot: snapshot(alphaTarget, "alpha-old", "older alpha content", "older alpha label") });
      await selectionResults[0];
      await waitForPending(2);
      expect(pending[1].target.path).toBe(catalog[1].path);
      pending[1].reject(new Error("delayed beta history failure"));
      await expect(selectionResults[1]).rejects.toThrow("delayed beta history failure");
      await waitForPending(3);
      expect(pending[2].target.path).toBe(catalog[0].path);
      pending[2].resolve({ ok: true, snapshot: snapshot(alphaTarget, "alpha-latest", "visible alpha content", "different displayText value") });
      await selectionResults[2];

      expect(currentSession.textContent).toBe("Alpha session");
      expect(getByText(conversation, "visible alpha content", { exact: true })).toBeTruthy();
      expect(conversation.textContent).not.toContain("different displayText value");
    } finally {
      unsubscribe();
    }
  });

});
