import { app, dialog, ipcMain, nativeTheme, shell } from "electron";
import { join, resolve } from "node:path";
import { mkdirSync } from "node:fs";
import { BackendService } from "./backend-service.js";
import { createWindow, pageURL } from "./window.js";
import { installSessionBridge } from "./session-bridge.js";
import { installMessageBridge } from "./message-bridge.js";
import { installInterruptionBridge } from "./interruption-bridge.js";
import { RUNTIME_INITIALIZATION_ERROR_CHANNEL } from "./session-contract.js";

function startupMark(stage: string): void {
  if (process.env.ROZSA_PERF_TRACE === "1") {
    console.info(`ROZSA_PERF_STARTUP ${JSON.stringify({ stage, epochMs: Date.now() })}`);
  }
}

startupMark("main-entry");
// Capture the directory used to launch this project once. Session cwd changes
// must not move the permission display or trust boundary.
const launchProjectRoot = resolve(process.env.ROZSA_PROJECT_ROOT || process.env.INIT_CWD || process.cwd());
// Development runtime storage is isolated as well as the catalog inputs.
app.setName("Rózsa");
const profile = resolve(app.commandLine.getSwitchValue("user-data-dir") || (app.isPackaged ? app.getPath("userData") : join(app.getAppPath(), "../tmp/electron-profile")));
mkdirSync(profile, { recursive: true });
app.setPath("userData", profile);
app.setPath("sessionData", profile);

let backend: BackendService | undefined;
let quitReady = false;
let quitting = false;
let runtimeErrorWindow: ReturnType<typeof createWindow> | undefined;
let runtimeErrorUiReady = false;
let pendingRuntimeInitializationError: string | undefined;
function deliverRuntimeInitializationError(message: string): void {
  if (quitting) return;
  if (!runtimeErrorUiReady || !runtimeErrorWindow || runtimeErrorWindow.isDestroyed()) {
    pendingRuntimeInitializationError = message;
    return;
  }
  runtimeErrorWindow.webContents.send(RUNTIME_INITIALIZATION_ERROR_CHANNEL, message);
}
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
backend = new BackendService(
  join(app.isPackaged ? process.resourcesPath : join(app.getAppPath(), "../dist"), process.platform === "win32" ? "rozsa-backend.exe" : "rozsa-backend"),
  { projectRoot: launchProjectRoot },
);
backend.onRuntimeInitializationError(deliverRuntimeInitializationError);
startupMark("backend-process-started");
startupMark("backend-initialize-started");
const backendReady = backend.initialize().then(() => startupMark("backend-ready"));
void backendReady.catch(() => { });
void backendReady.then(async () => {
  await backend!.waitForRuntime();
  startupMark("runtime-ready");
}).catch(async error => {
  console.error("Agent runtime initialization failed:", error);
});
void backendReady.then(async () => {
  startupMark("catalog-prefetch-started");
  const catalog = await backend!.prefetchSessions();
  startupMark("catalog-prefetch-ready");
  startupMark("latest-history-prefetch-started");
  await backend!.prefetchLatestSession(catalog);
  startupMark("latest-history-prefetch-ready");
  startupMark("startup-session-ready");
  if (catalog.length > 0) {
    startupMark("autocomplete-prewarm-started");
    void backend!.autocompleteInput("/", 1)
      .then(() => startupMark("autocomplete-prewarm-ready"))
      .catch(error => console.error("Autocomplete prewarm:", error));
  }
})
  .catch(error => console.error("Initial session catalog prefetch:", error));
void app.whenReady().then(async () => {
  startupMark("electron-ready");
  app.setName("Rózsa");
  if (quitting) return;
  const window = createWindow();
  runtimeErrorWindow = window;
  startupMark("window-created");
  window.once("focus", () => startupMark("window-focused"));
  window.once("blur", () => startupMark("window-blurred"));
  installSessionBridge(window, ipcMain, backend, pageURL, url => shell.openExternal(url), mode => { nativeTheme.themeSource = mode; });
  installMessageBridge(window, ipcMain, backend, pageURL);
  installInterruptionBridge(window, ipcMain, backend, pageURL);

  const firstPaint = new Promise<void>(resolve => {
    window.once("ready-to-show", () => resolve());
  });

  await window.loadURL(pageURL);
  startupMark("document-loaded");

  await window.webContents.executeJavaScript(`
    (async () => {
    if (!window.RozsaUIReady) {
      throw new Error("Renderer initialization not started");
    }
    await window.RozsaUIReady;
    await new Promise(resolve => {
      requestAnimationFrame(() => requestAnimationFrame(resolve));
    });
  })()
  `);

  await firstPaint

  if (quitting || window.isDestroyed()) return;
  runtimeErrorUiReady = true;
  if (pendingRuntimeInitializationError !== undefined) {
    const message = pendingRuntimeInitializationError;
    pendingRuntimeInitializationError = undefined;
    window.webContents.send(RUNTIME_INITIALIZATION_ERROR_CHANNEL, message);
  }
  window.show();
  startupMark("window-shown");

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
