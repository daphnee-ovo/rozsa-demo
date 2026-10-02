import { createUIHost } from "../../ui/scripts/core/renderer.js";
import { createTauriAdapter } from "../adapters/tauri.js";
import { mountShell } from "../../ui/scripts/core/shell.js";

const adapter = createTauriAdapter();
window.RozsaUIHost = createUIHost(adapter);
window.RozsaSidebarEmit = adapter.emit;
window.addEventListener("pagehide", () => window.RozsaUIHost.port.dispose(), { once: true });
await mountShell([]);
