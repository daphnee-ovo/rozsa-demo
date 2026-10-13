import { BrowserWindow } from "electron";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

export const page = join(import.meta.dirname, "../../browser/renderer/index.html");
export const pageURL = pathToFileURL(page).href;
export function createWindow(): BrowserWindow {
  const window = new BrowserWindow({
    width: 920, height: 680, minWidth: 560, minHeight: 400,
    title: "Rózsa · Sessions", show: false,
    webPreferences: { preload: join(import.meta.dirname, "../../preload.cjs"), contextIsolation: true, sandbox: true, nodeIntegration: false, webSecurity: true },
  });
  window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
  window.webContents.on("will-navigate", event => event.preventDefault());
  window.webContents.on("will-attach-webview", event => event.preventDefault());
  window.once("ready-to-show", () => window.show());
  window.once("show", () => console.info("Session window shown"));
  return window;
}
