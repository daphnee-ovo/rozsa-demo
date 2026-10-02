import { createUIHost } from "../../ui/scripts/core/renderer.js";
import { createTauriAdapter } from "../adapters/tauri.js";
import { mountShell, templateRoot } from "../../ui/scripts/core/shell.js";
import { loadTemplate } from '../../ui/scripts/core/template-loader.js';
import { configureSettingsSuspensions, isSettingsPaneAvailable, bindSettingsNavigation } from '../../ui/scripts/features/settings.js';
import { createSidebarFeedback } from '../adapters/notifications.js';

const adapter = createTauriAdapter();
window.RozsaUIHost = createUIHost(adapter);
window.RozsaSidebarEmit = adapter.emit;
window.addEventListener("pagehide", () => window.RozsaUIHost.port.dispose(), { once: true });
window.RozsaSettings = Object.freeze({ isSettingsPaneAvailable });
window.RozsaUIReady = (async () => {
  await mountShell([{ url: new URL('composites/notification.html', templateRoot), selector: '[data-template-mount=notification]', children: true }]);
  window.RozsaSidebarFeedback = createSidebarFeedback();
  const template = await loadTemplate(new URL('features/settings.html', templateRoot));
  const navigation = template.querySelector('#fallbackSettingsNavigationTemplate').content.cloneNode(true);
  const root = document.getElementById('settingsSidebarScene');
  root.replaceChildren(navigation);
  configureSettingsSuspensions();
  bindSettingsNavigation(root, { switchSettingsTab: selectSidebarSettingsPane, closeSettings: closeSidebarSettings });
})();
