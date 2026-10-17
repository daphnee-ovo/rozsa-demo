import { BrowserWindow } from "electron";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

export const page = join(__dirname, `../renderer/${SESSIONS_VITE_NAME}/index.html`);
export const pageURL = SESSIONS_VITE_DEV_SERVER_URL ? new URL("/", SESSIONS_VITE_DEV_SERVER_URL).href : pathToFileURL(page).href;
export function createWindow(): BrowserWindow {
  const window = new BrowserWindow({
    width: 920, height: 680, minWidth: 560, minHeight: 400,
    title: "Rózsa", show: false,
    ...(process.platform === "darwin" ? {
      titleBarStyle: "hiddenInset" as const,
      vibrancy: "sidebar" as const,
      visualEffectState: "followWindow" as const,
    } : {}),
    webPreferences: {
      preload: join(__dirname, "preload.cjs"),
      contextIsolation: true, sandbox: true, nodeIntegration: false, webSecurity: true,
    },
  });
  window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
  // Vite's automatic full reload navigates to the same trusted page. Keep that
  // reload path available while denying every other destination.
  window.webContents.on("will-navigate", (event, url) => { if (url !== pageURL) event.preventDefault(); });
  window.webContents.on("will-attach-webview", event => event.preventDefault());
  window.once("show", () => console.info("Session window shown"));
  return window;
}
