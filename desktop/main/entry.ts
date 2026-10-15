import { app, dialog, ipcMain } from "electron";
import { join, resolve } from "node:path";
import { mkdirSync } from "node:fs";
import { BackendService } from "./backend-service.js";
import { createWindow, pageURL } from "./window.js";
import { SESSION_LIST_CHANNEL, SESSION_HISTORY_CHANNEL, sessionTarget, bridgeFault, type SessionHistoryReply, type SessionListReply } from "./session-contract.js";

// Candidate runtime storage is isolated as well as the catalog inputs.
app.setName("Rózsa Migration");
const profile = resolve(app.commandLine.getSwitchValue("user-data-dir") || (app.isPackaged ? app.getPath("userData") : join(app.getAppPath(), "../tmp/migration/electron-profile")));
mkdirSync(profile, { recursive: true });
app.setPath("userData", profile);
app.setPath("sessionData", profile);

// Directories are configured by the trusted launcher, never supplied by renderer.
function directories(args: string[]): string[] {
  const result: string[] = [];
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--session-dir") {
      const value = args[++i];
      if (!value || value.startsWith("--")) throw new Error("--session-dir requires a directory");
      result.push(resolve(value));
    } else if (args[i].startsWith("--session-dir=")) {
      const value = args[i].slice("--session-dir=".length);
      if (!value) throw new Error("--session-dir requires a directory");
      result.push(resolve(value));
    }
  }
  return result;
}

let backend: BackendService | undefined;
let quitReady = false;
let quitting = false;
app.on("before-quit", event => {
  if (quitReady) return;
  event.preventDefault();
  if (quitting) return;
  quitting = true;
  void (backend?.stop() ?? Promise.resolve()).finally(() => { quitReady = true; app.quit(); });
});
app.on("window-all-closed", () => app.quit());
// Forge main hot restart sends SIGTERM. Reuse the normal owned-backend shutdown.
process.once("SIGTERM", () => app.quit());
void app.whenReady().then(async () => {
  app.setName("Rózsa Migration");
  backend = new BackendService(join(app.isPackaged ? process.resourcesPath : join(app.getAppPath(), "../dist/migration"), process.platform === "win32" ? "rozsa-backend.exe" : "rozsa-backend"), directories(process.argv));
  await backend.initialize();
  const window = createWindow();
  ipcMain.handle(SESSION_LIST_CHANNEL, async (event, ...args: unknown[]): Promise<SessionListReply> => {
    if (event.sender !== window.webContents || event.senderFrame !== window.webContents.mainFrame || event.senderFrame.url !== pageURL || args.length !== 0) throw new Error("Unauthorized session list request");
    try { return { ok: true, sessions: await backend!.getSessions() }; }
    catch (error) {
      return { ok: false, error: bridgeFault(error) };
    }
  });
  ipcMain.handle(SESSION_HISTORY_CHANNEL, async (event, ...args: unknown[]): Promise<SessionHistoryReply> => {
    if (event.sender !== window.webContents || event.senderFrame !== window.webContents.mainFrame || event.senderFrame.url !== pageURL || args.length !== 1) throw new Error("Unauthorized session history request");
    const target = sessionTarget(args[0]);
    try { return { ok: true, snapshot: await backend!.switchSession(target) }; }
    catch (error) {
      return { ok: false, error: bridgeFault(error) };
    }
  });
  window.on("closed", () => { ipcMain.removeHandler(SESSION_LIST_CHANNEL); ipcMain.removeHandler(SESSION_HISTORY_CHANNEL); });
  await window.loadURL(pageURL);
}).catch(error => {
  dialog.showErrorBox("Rózsa could not start", error instanceof Error ? error.message : String(error));
  app.quit();
});
