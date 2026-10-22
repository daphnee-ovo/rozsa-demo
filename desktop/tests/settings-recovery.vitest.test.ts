import { afterEach, describe, expect, it } from "vitest";
import { access, chmod, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { BackendService } from "../main/backend-service.js";

const root = resolve(import.meta.dirname, "../..");
const capabilities = ["get_sessions", "switch_session", "subscribe_sessions", "next_session_event", "unsubscribe_sessions", "runtime_status", "start_message", "continue_message", "subscribe_message_events", "next_message_event", "unsubscribe_message_events", "cancel_message", "get_interruption_state", "respond_permission", "respond_user_question", "cancel_user_question", "get_settings", "autocomplete_input", "update_setting", "reset_setting", "list_themes", "get_theme", "save_theme"];
let service: BackendService | undefined;
let releaseBarrier: (() => Promise<void>) | undefined;

async function startBackend(options: { busyOnceScope?: string; readCapacity?: number; barrierScope?: string; projectFontSize?: number } = {}): Promise<{ backend: BackendService; waitForCapture: () => Promise<void>; release: () => Promise<void> }> {
  const directory = await mkdtemp(join(tmpdir(), "rozsa-settings-recovery-"));
  const executable = join(directory, "backend.cjs");
  const capturedPath = join(directory, "snapshot-captured");
  const releasePath = join(directory, "release-snapshot");
  const source = [
    `#!${process.execPath}`,
    `const { createMessageConnection, ResponseError } = require(${JSON.stringify(join(root, "desktop/node_modules/vscode-jsonrpc/lib/node/main.js"))});`,
    "const fs=require('node:fs/promises');",
    "const conn = createMessageConnection(process.stdin, process.stdout);",
    `conn.onRequest('initialize',()=>({protocolVersion:1,capabilities:${JSON.stringify(capabilities)}}));`,
    "let activeReads=0,busyOnce=process.env.ROZSA_TEST_BUSY_ONCE_SCOPE||'',snapshotHeld=false;const values={global:{appearance_font_size:14},project:{}},savedThemes=[];",
    "if(process.env.ROZSA_TEST_PROJECT_FONT_SIZE)values.project.appearance_font_size=Number(process.env.ROZSA_TEST_PROJECT_FONT_SIZE);",
    "const theme=(mode)=>({id:mode,name:mode,mode,accent:'#000',background:'#fff',foreground:'#000',uiFont:'sans',codeFont:'mono',variables:{}});",
    "const fontSize=()=>values.project.appearance_font_size??values.global.appearance_font_size;",
    "const snapshot=(scope)=>({scope,settings:{appearance:{fontSize:fontSize()}},appearance:{themeMode:'system',fontSize:fontSize(),translucentSidebar:false,lightTheme:theme('light'),darkTheme:theme('dark')},themes:[{id:'light',name:'Light',mode:'light'},{id:'dark',name:'Dark',mode:'dark'},...savedThemes.map(({id,name,mode})=>({id,name,mode}))],models:[]});",
    "conn.onRequest('watch_models',()=>true);",
    `conn.onRequest('get_settings',async({scope})=>{if(busyOnce===scope){busyOnce='';throw new ResponseError(-32006,'Concurrent request capacity reached');}if(++activeReads>Number(process.env.ROZSA_TEST_SETTINGS_CAPACITY)){activeReads--;throw new ResponseError(-32006,'Concurrent request capacity reached');}const result=snapshot(scope);try{if(!snapshotHeld&&scope===process.env.ROZSA_TEST_BARRIER_SCOPE){snapshotHeld=true;await fs.writeFile(${JSON.stringify(capturedPath)},'captured');for(;;){try{await fs.access(${JSON.stringify(releasePath)});break;}catch{await new Promise(resolve=>setTimeout(resolve,5));}}}return result;}finally{activeReads--;}});`,
    "conn.onRequest('update_setting',({scope,key,value})=>{if(key==='appearance_font_size')values[scope].appearance_font_size=value;return snapshot(scope);});",
    "conn.onRequest('reset_setting',({scope,key})=>{if(key==='appearance_font_size')delete values[scope].appearance_font_size;return snapshot(scope);});",
    "conn.onRequest('save_theme',(value)=>{savedThemes.push(value);return true;});",
    "conn.onRequest('shutdown',()=>{setTimeout(()=>process.exit(0),20);return true;});",
    "conn.listen();",
  ].join("\n");
  await writeFile(executable, source);
  await chmod(executable, 0o700);
  service = new BackendService(executable, { env: { ROZSA_TEST_BUSY_ONCE_SCOPE: options.busyOnceScope ?? "", ROZSA_TEST_SETTINGS_CAPACITY: String(options.readCapacity ?? 1), ROZSA_TEST_BARRIER_SCOPE: options.barrierScope ?? "", ROZSA_TEST_PROJECT_FONT_SIZE: options.projectFontSize ? String(options.projectFontSize) : "" } });
  await service.initialize();
  const release = async (): Promise<void> => { await writeFile(releasePath, "release"); };
  const waitForCapture = async (): Promise<void> => {
    const deadline = Date.now() + 5000;
    while (Date.now() < deadline) {
      try { await access(capturedPath); return; } catch { await new Promise(resolve => setTimeout(resolve, 5)); }
    }
    throw new Error("The settings snapshot did not reach the controlled barrier");
  };
  releaseBarrier = release;
  return { backend: service, waitForCapture, release };
}

afterEach(async () => { await releaseBarrier?.(); await service?.stop(); service = undefined; releaseBarrier = undefined; });

describe("settings reads across renderer recovery", () => {
  it("lets overlapping same-scope reload reads share a result before backend capacity rejects duplicates", async () => {
    const harness = await startBackend({ barrierScope: "global" });
    const resultsPending = Promise.allSettled([harness.backend.getSettings("global"), harness.backend.getSettings("global")]);
    await harness.waitForCapture();
    await harness.release();
    const results = await resultsPending;
    expect(results.map(result => result.status)).toEqual(["fulfilled", "fulfilled"]);
    const snapshots = results.flatMap(result => result.status === "fulfilled" ? [result.value] : []);
    expect(snapshots.map(result => result.scope)).toEqual(["global", "global"]);
    expect(snapshots[0]).toEqual(snapshots[1]);
  });

  it("returns concurrent global and project snapshots with the requested scope and effective settings", async () => {
    const { backend } = await startBackend({ readCapacity: 2 });
    const [global, project] = await Promise.all([backend.getSettings("global"), backend.getSettings("project")]);
    expect([global.scope, project.scope]).toEqual(["global", "project"]);
    expect([global.settings.appearance.fontSize, project.settings.appearance.fontSize]).toEqual([14, 14]);
  });

  it("surfaces Busy and permits a later read to recover", async () => {
    const { backend } = await startBackend({ busyOnceScope: "project" });
    await expect(backend.getSettings("project")).rejects.toMatchObject({
      remote: { kind: "busy", code: -32006, message: "Concurrent request capacity reached" },
    });
    expect((await backend.getSettings("project")).settings.appearance.fontSize).toBe(14);
  });

  it("refreshes a pending project read after a global setting update without changing the old result", async () => {
    const harness = await startBackend({ readCapacity: 2, barrierScope: "project" });
    const oldRead = harness.backend.getSettings("project");
    await harness.waitForCapture();
    await harness.backend.updateSetting("global", "appearance_font_size", 18);
    const newRead = harness.backend.getSettings("project");
    await harness.release();
    const [oldSnapshot, newSnapshot] = await Promise.all([oldRead, newRead]);
    expect(oldSnapshot.settings.appearance.fontSize).toBe(14);
    expect(newSnapshot.settings.appearance.fontSize).toBe(18);
  });

  it("refreshes a pending global read after resetting a project override", async () => {
    const harness = await startBackend({ readCapacity: 2, barrierScope: "global", projectFontSize: 18 });
    const oldRead = harness.backend.getSettings("global");
    await harness.waitForCapture();
    await harness.backend.resetSetting("project", "appearance_font_size");
    const newRead = harness.backend.getSettings("global");
    await harness.release();
    const [oldSnapshot, newSnapshot] = await Promise.all([oldRead, newRead]);
    expect(oldSnapshot.settings.appearance.fontSize).toBe(18);
    expect(newSnapshot.settings.appearance.fontSize).toBe(14);
  });

  it("does not reuse an in-flight snapshot after saving a theme", async () => {
    const harness = await startBackend({ readCapacity: 2, barrierScope: "global" });
    const pending = harness.backend.getSettings("global");
    await harness.waitForCapture();
    await harness.backend.saveTheme({ id: "custom", name: "Custom", mode: "light", accent: "#000", background: "#fff", foreground: "#000", uiFont: "sans", codeFont: "mono", variables: {} });
    const fresh = harness.backend.getSettings("global");
    await harness.release();
    await pending;
    const snapshot = await fresh;
    expect(snapshot.themes.map(item => item.id)).toContain("custom");
  });
});
