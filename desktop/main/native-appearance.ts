import type { BrowserWindow } from "electron";

type VibrancyWindow = Pick<BrowserWindow, "isDestroyed" | "setVibrancy">;

export function applySidebarVibrancy(window: VibrancyWindow, enabled: boolean, platform: NodeJS.Platform = process.platform): void {
  if (platform !== "darwin" || window.isDestroyed()) return;
  window.setVibrancy(enabled ? "sidebar" : null);
}
