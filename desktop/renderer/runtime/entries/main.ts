import { createUIHost } from "../../scripts/core/renderer.ts";
import { createElectronAdapter } from "../adapters/electron.ts";
import { bindNativeViewport } from '../adapters/viewport.js';
import { mountShell, resolveTemplateRoot } from "../../scripts/core/shell.ts";
import { init as bindPopover } from "../../scripts/components/popover.js";
import { init as bindComposer, renderSendState } from "../../scripts/components/composer.js";
import { init as bindConversation, emptyConversationMarkup, messageFrame, createMessageFrame, runningPanelMarkup, thinkingMarkup } from "../../scripts/features/conversation.ts";
import { mountConversation, createConversationFlow } from "../../scripts/flows/conversation.ts";
import { createConversationAdapter } from "../adapters/conversation.js";
import { createConversationRenderer } from "../../scripts/features/conversation-view.tsx";
import * as tools from '../../scripts/features/tools.js';
import { init as bindTools } from '../../scripts/components/tool-call.js';
import { toolView, activityView } from '../adapters/tools.js';
import * as permissions from '../../scripts/features/permissions.js';
import { init as bindInterruption } from '../../scripts/components/interruption-panel.js';
import { init as bindInterruptionFlow } from '../../scripts/flows/composer-interruption.js';
import { createInterruptionAdapter } from '../adapters/interruption.js';
import * as settings from '../../scripts/features/settings.js';
import { createSettingsAdapter, bindSettingSwitch } from '../adapters/settings.js';
import * as notifications from '../../scripts/features/notifications.js';
import { bindToast, bindErrorTray } from '../../scripts/components/notification.js';
import { subscribeNotifications, showRuntimeError } from '../adapters/notifications.js';
import * as sidebar from '../../scripts/features/sidebar.js';
import { loadTemplate } from '../../scripts/core/template-loader.ts';
import * as devFlow from '../../scripts/features/dev-flow.js';
import { devFlowSettingsView, devFlowDetailView } from '../adapters/sidebar-dev-flow.js';

document.body.classList.toggle('macos-host', navigator.platform === 'MacIntel');
window.RozsaUIHost = createUIHost(createElectronAdapter(window.rozsa));
window.addEventListener("pagehide", () => window.RozsaUIHost.port.dispose(), { once: true });
let conversationRenderer: ReturnType<typeof createConversationRenderer> | undefined;
window.addEventListener("pagehide", () => conversationRenderer?.dispose(), { once: true });
window.RozsaUIReady = Promise.all([bindNativeViewport(), mountConversation(document.getElementById("mainContentScene"))]).then(() => mountShell([
  { url: new URL("composites/popover.html", resolveTemplateRoot()), selector: "[data-template-mount=popover]", children: true },
  { url: new URL("composites/interruption-panel.html", resolveTemplateRoot()), selector: "[data-template-mount=interruption-panel]", children: true },
  { url: new URL("features/settings.html", resolveTemplateRoot()), selector: "[data-template-mount=settings]", children: true },
  { url: new URL("composites/notification.html", resolveTemplateRoot()), selector: "[data-template-mount=notification]", children: true },
  { url: new URL("features/dev-flow.html", resolveTemplateRoot()), selector: "[data-template-mount=dev-flow]", children: true },
])).then(async () => {
  const conversationRoot = document.getElementById("chatMessages");
  if (!conversationRoot) throw new Error("Conversation React root not found after mounting the conversation template");
  conversationRenderer = createConversationRenderer(conversationRoot);
  const sidebarTemplate = await loadTemplate(new URL('features/sidebar.html', resolveTemplateRoot()));
  document.querySelector<HTMLTemplateElement>('#fallbackSidebarTemplate')!.content.replaceChildren(sidebarTemplate);
  window.RozsaSidebar = Object.freeze({ ...sidebar });
  window.RozsaNotifications = Object.freeze({ ...notifications, bindToast, bindErrorTray, subscribeNotifications, showRuntimeError });
  window.RozsaSettings = Object.freeze({ ...settings, bindSettingSwitch });
  window.RozsaDevFlow = Object.freeze({ ...devFlow, devFlowSettingsView, devFlowDetailView });
  devFlow.init({ boundModules: new Set(), byId: (id: string) => document.getElementById(id), closeDevFlowDetail });
  settings.init({ ...createSettingsAdapter({ switchSettingsTab, closeSettings, selectThemeModeCard,
    previewFontSize, commitFontSize }), boundModules: new Set(), byId: (id: string) => document.getElementById(id) });
  await tools.loadToolTemplate();
  window.RozsaTools = Object.freeze({ ...tools, toolView, activityView });
  bindTools({ boundModules: new Set(), byId: (id: string) => document.getElementById(id), toggleToolCall, toggleTurnDiff });
  window.RozsaPermissions = permissions;
  const interruption = {
    ...createInterruptionAdapter({ respondPermission, enterPermissionTrust, enterPermissionHint,
      togglePermissionCommand, showPermissionMainPage, submitPermissionHint, submitUserQuestion,
      normalizePermissionHint, handlePermissionHintKeydown }),
    boundModules: new Set(), byId: (id: string) => document.getElementById(id),
  };
  configureInterruptionSuspensions();
  bindInterruption(interruption);
  bindInterruptionFlow(interruption);
  document.querySelector('[data-od-id="chat-input"]')!.prepend(document.querySelector<HTMLTemplateElement>('#runtimeSubagentTemplate')!.content.cloneNode(true));
  configureConversationSuspensions();
  window.RozsaConversation = Object.freeze({ emptyConversationMarkup, messageFrame, createMessageFrame, runningPanelMarkup, thinkingMarkup,
    renderView: conversationRenderer.render });
  bindConversation({ boundModules: new Set(), byId: (id: string) => document.getElementById(id), toggleThinking, renderMessages: () => {} });
  window.RozsaComposer = Object.freeze({ renderSendState });
  const flow = createConversationFlow(createConversationAdapter({
    attachFileReference, attachDirectoryReference, insertSlashCommandPrefix, showModelPicker,
    updateAbortButton,
    submit: () => isStreaming ? abortAgent() : sendMessage(),
  }));
  bindComposer({
    ...flow, boundModules: new Set(), byId: (id: string) => document.getElementById(id),
    handleInput, handleCompositionStart, handleCompositionUpdate, handleCompositionEnd,
  });
  window.RozsaPopover = bindPopover({
    boundModules: new Set(),
    byId: (id: string) => document.getElementById(id),
    beforeOpen: () => { dismissTransientPopups(); renderThinkingEffortPicker(); },
    previewThinkingEffort,
    selectThinkingEffort,
  });
});
