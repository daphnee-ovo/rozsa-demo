import assert from "node:assert/strict";
import { test } from "node:test";
import { EventEmitter } from "node:events";
import type { BrowserWindow, IpcMain, IpcMainInvokeEvent } from "electron";
import type { BackendService } from "../main/backend-service.ts";
import { installSessionBridge } from "../main/session-bridge.ts";
import { INPUT_AUTOCOMPLETE_CHANNEL } from "../main/session-contract.ts";
import { RozsaBridge } from "../preload/bridge.ts";
import { createElectronAdapter } from "../renderer/runtime/adapters/electron.ts";
import { ProductClient } from "../renderer/product-client.ts";
import { assertProductBridgeCapabilities } from "./helpers/bridge-capabilities.ts";

const completion = {
  prefix: "@doc",
  items: [{ value: "@document.md ", label: "document.md", description: "document.md" }],
  validMatch: false,
  highlightRanges: [],
};

const lightTheme = { id: "rozsa", name: "Rozsa", mode: "light" as const, accent: "#D7827E", background: "#FFFFFF", foreground: "#575279", uiFont: "system-ui", codeFont: "monospace", variables: {} };
const darkTheme = { ...lightTheme, id: "rozsa-dark", name: "Rozsa Dark", mode: "dark" as const };
const settings = {
  scope: "global" as const,
  settings: {},
  appearance: { themeMode: "system" as const, fontSize: 14, translucentSidebar: false, lightTheme, darkTheme },
  themes: [{ id: lightTheme.id, name: lightTheme.name, mode: lightTheme.mode }, { id: darkTheme.id, name: darkTheme.name, mode: darkTheme.mode }],
  models: [],
};

function setup(
  autocompleteInput: (text: string, cursor: number) => Promise<unknown>,
  updateSetting: (scope: string, key: string, value: unknown) => Promise<unknown> = async () => settings,
) {
  const url = "file:///synthetic/index.html";
  const webContents = Object.assign(new EventEmitter(), { mainFrame: { url } });
  const window = Object.assign(new EventEmitter(), { webContents, isDestroyed: () => false });
  const handlers = new Map<string, (event: IpcMainInvokeEvent, ...args: unknown[]) => Promise<unknown>>();
  const ipc = {
    handle: (channel: string, handler: (event: IpcMainInvokeEvent, ...args: unknown[]) => Promise<unknown>) => handlers.set(channel, handler),
    removeHandler: (channel: string) => { handlers.delete(channel); },
  };
  const backend = { autocompleteInput, updateSetting };
  installSessionBridge(window as unknown as BrowserWindow, ipc as unknown as IpcMain, backend as unknown as BackendService, url);
  const event = { sender: webContents, senderFrame: webContents.mainFrame } as unknown as IpcMainInvokeEvent;
  const invoke = async (channel: string, ...args: unknown[]) => {
    const handler = handlers.get(channel);
    if (!handler) throw new Error("No handler");
    return handler(event, ...args);
  };
  const bridge = RozsaBridge.create(invoke);
  return { bridge, handlers, event, window };
}

test("input completion crosses renderer client, preload, trusted IPC, and backend service", async () => {
  const calls: Array<[string, number]> = [];
  const ui = setup(async (text, cursor) => { calls.push([text, cursor]); return completion; });
  assertProductBridgeCapabilities(ui.bridge, { events: false });
  const adapter = createElectronAdapter(ui.bridge);

  const result = await adapter.request("autocomplete_input", { text: "@doc", cursor: 4 });
  assert.deepEqual(result, completion);
  assert.deepEqual(calls, [["@doc", 4]]);
  const client = new ProductClient(ui.bridge);
  await assert.rejects(() => client.autocompleteInput("@doc", -1), /Invalid input completion request/);
  await assert.rejects(() => ui.bridge.autocompleteInput("@doc", Number.MAX_SAFE_INTEGER + 1), /Invalid input completion request/);
  assert.deepEqual(calls, [["@doc", 4]]);

  const handler = ui.handlers.get(INPUT_AUTOCOMPLETE_CHANNEL)!;
  await assert.rejects(handler({ ...ui.event, sender: {} } as IpcMainInvokeEvent, "@doc", 4), /Unauthorized settings request/);
  const invalidIpcReply = await handler(ui.event, 4, 4);
  assert.equal((invalidIpcReply as { ok: boolean }).ok, false);
  assert.match((invalidIpcReply as { error: { message: string } }).error.message, /Invalid input completion request/);
  assert.deepEqual(calls, [["@doc", 4]]);
  ui.window.emit("closed");
});

test("model picker selection carries the provider through the product settings route", async () => {
  const calls: unknown[][] = [];
  const ui = setup(async () => completion, async (scope, key, value) => { calls.push([scope, key, value]); return settings; });
  const adapter = createElectronAdapter(ui.bridge);
  assert.equal(await adapter.request("switch_model", { modelId: "model-a", provider: "provider-a" }), true);
  assert.deepEqual(calls, [["global", "model", { id: "model-a", provider: "provider-a" }]]);
  ui.window.emit("closed");
});

test("malformed input completion replies are reported as protocol failures", async () => {
  const ui = setup(async () => ({ prefix: "@doc", items: [{ value: "@doc", label: "doc" }], validMatch: false, highlightRanges: [] }));
  const client = new ProductClient(ui.bridge);
  await assert.rejects(() => client.autocompleteInput("@doc", 4), /Invalid input completion response/);
  ui.window.emit("closed");
});
