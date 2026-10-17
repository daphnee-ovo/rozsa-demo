import { app, dialog, ipcMain } from "electron";
import { join, resolve } from "node:path";
import { mkdirSync } from "node:fs";
import { BackendService } from "./backend-service.js";
import { createWindow, pageURL } from "./window.js";
import { installSessionBridge } from "./session-bridge.js";
import { installMessageBridge } from "./message-bridge.js";

function startupMark(stage: string): void {
  if (process.env.ROZSA_PERF_TRACE === "1") {
    console.info(`ROZSA_PERF_STARTUP ${JSON.stringify({ stage, epochMs: Date.now() })}`);
  }
}

startupMark("main-entry");
// Development runtime storage is isolated as well as the catalog inputs.
app.setName("Rózsa");
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
backend = new BackendService(join(app.isPackaged ? process.resourcesPath : join(app.getAppPath(), "../dist"), process.platform === "win32" ? "rozsa-backend.exe" : "rozsa-backend"), directories(process.argv));
startupMark("backend-process-started");
startupMark("backend-initialize-started");
const backendReady = backend.initialize().then(() => startupMark("backend-ready"));
void backendReady.catch(() => {});
void backendReady.then(async () => {
  await backend!.waitForRuntime();
  startupMark("runtime-ready");
}).catch(async error => {
  await app.whenReady();
  if (!quitting) dialog.showErrorBox("Agent runtime initialization failed", error instanceof Error ? error.message : String(error));
});
void backendReady.then(async () => {
  startupMark("catalog-prefetch-started");
  const catalog = await backend!.prefetchSessions();
  startupMark("catalog-prefetch-ready");
  startupMark("latest-history-prefetch-started");
  await backend!.prefetchLatestSession(catalog);
  startupMark("latest-history-prefetch-ready");
  startupMark("startup-session-ready");
})
  .catch(error => console.error("Initial session catalog prefetch:", error));
void app.whenReady().then(async () => {
  startupMark("electron-ready");
  app.setName("Rózsa");
  if (quitting) return;
  const window = createWindow();
  startupMark("window-created");
  window.once("focus", () => startupMark("window-focused"));
  window.once("blur", () => startupMark("window-blurred"));
  installSessionBridge(window, ipcMain, backend, pageURL);
  installMessageBridge(window, ipcMain, backend, pageURL);
  window.show();
  startupMark("window-shown");
  const pageReady = window.loadURL(pageURL);
  await pageReady;
  startupMark("document-loaded");
  if (quitting) return;
  // Renderer boot and Go initialization overlap. BackendService queues product
  // requests until this handshake resolves, so early catalog reads stay safe.
  await backendReady;
}).catch(error => {
  console.error("Rózsa could not start:", error);
  if (quitting) return;
  dialog.showErrorBox("Rózsa could not start", error instanceof Error ? error.message : String(error));
  app.quit();
});
