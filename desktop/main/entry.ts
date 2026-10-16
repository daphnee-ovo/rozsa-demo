import { app, dialog, ipcMain } from "electron";
import { join, resolve } from "node:path";
import { mkdirSync } from "node:fs";
import { BackendService } from "./backend-service.js";
import { createWindow, pageURL } from "./window.js";
import { installSessionBridge } from "./session-bridge.js";

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
void app.whenReady().then(async () => {
  app.setName("Rózsa");
  backend = new BackendService(join(app.isPackaged ? process.resourcesPath : join(app.getAppPath(), "../dist"), process.platform === "win32" ? "rozsa-backend.exe" : "rozsa-backend"), directories(process.argv));
  await backend.initialize();
  if (quitting) return;
  const window = createWindow();
  installSessionBridge(window, ipcMain, backend, pageURL);
  await window.loadURL(pageURL);
}).catch(error => {
  console.error("Rózsa could not start:", error);
  if (quitting) return;
  dialog.showErrorBox("Rózsa could not start", error instanceof Error ? error.message : String(error));
  app.quit();
});
