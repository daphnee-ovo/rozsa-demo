import { createUIHost } from "../../ui/scripts/core/renderer.js";
import { createTauriAdapter } from "../adapters/tauri.js";
import { mountShell, templateRoot } from "../../ui/scripts/core/shell.js";
import { init as bindPopover } from "../../ui/scripts/components/popover.js";
import { init as bindComposer, renderSendState } from "../../ui/scripts/components/composer.js";
import { init as bindConversation, emptyConversationMarkup, messageFrame, createMessageFrame, runningPanelMarkup, thinkingMarkup } from "../../ui/scripts/features/conversation.js";
import { mountConversation, createConversationFlow } from "../../ui/scripts/flows/conversation.js";
import { createConversationAdapter } from "../adapters/conversation.js";
import * as tools from '../../ui/scripts/features/tools.js';
import { init as bindTools } from '../../ui/scripts/components/tool-call.js';
import { toolView, activityView } from '../adapters/tools.js';
import * as permissions from '../../ui/scripts/features/permissions.js';
import { init as bindInterruption } from '../../ui/scripts/components/interruption-panel.js';
import { init as bindInterruptionFlow } from '../../ui/scripts/flows/composer-interruption.js';
import { createInterruptionAdapter } from '../adapters/interruption.js';

window.RozsaUIHost = createUIHost(createTauriAdapter());
window.addEventListener("pagehide", () => window.RozsaUIHost.port.dispose(), { once: true });
window.RozsaUIReady = mountConversation(document.getElementById("mainContentScene")).then(() => mountShell([
  { url: new URL("composites/popover.html", templateRoot), selector: "[data-template-mount=popover]", children: true },
  { url: new URL("composites/interruption-panel.html", templateRoot), selector: "[data-template-mount=interruption-panel]", children: true },
])).then(async () => {
  await tools.loadToolTemplate();
  window.RozsaTools = Object.freeze({ ...tools, toolView, activityView });
  bindTools({ boundModules: new Set(), byId: id => document.getElementById(id), toggleToolCall, toggleTurnDiff });
  window.RozsaPermissions = permissions;
  const interruption = {
    ...createInterruptionAdapter({ respondPermission, enterPermissionTrust, enterPermissionHint,
      togglePermissionCommand, showPermissionMainPage, submitPermissionHint, submitUserQuestion,
      normalizePermissionHint, handlePermissionHintKeydown }),
    boundModules: new Set(), byId: id => document.getElementById(id),
  };
  configureInterruptionSuspensions();
  bindInterruption(interruption);
  bindInterruptionFlow(interruption);
  document.querySelector('[data-od-id="chat-input"]').prepend(document.getElementById("runtimeSubagentTemplate").content.cloneNode(true));
  configureConversationSuspensions();
  window.RozsaConversation = Object.freeze({ emptyConversationMarkup, messageFrame, createMessageFrame, runningPanelMarkup, thinkingMarkup });
  bindConversation({ boundModules: new Set(), byId: id => document.getElementById(id), toggleThinking, renderMessages: () => {} });
  window.RozsaComposer = Object.freeze({ renderSendState });
  const flow = createConversationFlow(createConversationAdapter({
    attachFileReference, attachDirectoryReference, insertSlashCommandPrefix, showModelPicker,
    updateAbortButton,
    submit: () => isStreaming ? abortAgent() : sendMessage(),
  }));
  bindComposer({
    ...flow, boundModules: new Set(), byId: id => document.getElementById(id),
    handleInput, handleCompositionStart, handleCompositionUpdate, handleCompositionEnd,
  });
  window.RozsaPopover = bindPopover({
    boundModules: new Set(),
    byId: id => document.getElementById(id),
    beforeOpen: () => { dismissTransientPopups(); renderThinkingEffortPicker(); },
    previewThinkingEffort,
    selectThinkingEffort,
  });
});
