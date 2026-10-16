import { createUIHost } from "../../scripts/core/renderer.js";
import { createTauriAdapter } from "../adapters/tauri.js";
import { bindNativeViewport } from '../adapters/viewport.js';
import { mountShell, templateRoot } from "../../scripts/core/shell.js";
import { loadTemplate } from '../../scripts/core/template-loader.js';
import { configureSettingsSuspensions, isSettingsPaneAvailable, bindSettingsNavigation } from '../../scripts/features/settings.js';
import { createSidebarFeedback } from '../adapters/notifications.js';

import * as sidebar from '../../scripts/features/sidebar.js';
import { claimedMarkup } from '../../scripts/features/dev-flow.js';

const adapter = createTauriAdapter();
window.RozsaUIHost = createUIHost(adapter);
window.RozsaSidebarEmit = adapter.emit;
window.addEventListener("pagehide", () => window.RozsaUIHost.port.dispose(), { once: true });
window.RozsaSettings = Object.freeze({ isSettingsPaneAvailable });
window.RozsaUIReady = (async () => {
  await Promise.all([bindNativeViewport(), mountShell([{ url: new URL('features/sidebar.html', templateRoot), selector: '[data-template-mount=sidebar]', children: true }])]);
  window.RozsaSidebar = Object.freeze({ ...sidebar, claimedMarkup });
  sidebar.suspendQuotaTooltip();
  sidebar.init({ boundModules: new Set(), byId: id => document.getElementById(id),
    activateSession: index => switchSidebarSession(sidebarSessions[index].path),
    newSession: newSidebarSession, toggleSettings: openSidebarSettings, openDevFlowDashboard,
  });
  window.RozsaSidebarFeedback = createSidebarFeedback(adapter);
  const template = await loadTemplate(new URL('features/settings.html', templateRoot));
  const navigation = template.querySelector('#fallbackSettingsNavigationTemplate').content.cloneNode(true);
  const root = document.getElementById('settingsSidebarScene');
  root.replaceChildren(navigation);
  configureSettingsSuspensions();
  bindSettingsNavigation(root, { switchSettingsTab: selectSidebarSettingsPane, closeSettings: closeSidebarSettings });
})();
