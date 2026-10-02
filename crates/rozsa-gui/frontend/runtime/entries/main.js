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

window.RozsaUIHost = createUIHost(createTauriAdapter());
window.addEventListener("pagehide", () => window.RozsaUIHost.port.dispose(), { once: true });
window.RozsaUIReady = mountConversation(document.getElementById("mainContentScene")).then(() => mountShell([
  { url: new URL("composites/popover.html", templateRoot), selector: "[data-template-mount=popover]", children: true },
])).then(async () => {
  await tools.loadToolTemplate();
  window.RozsaTools = Object.freeze({ ...tools, toolView, activityView });
  bindTools({ boundModules: new Set(), byId: id => document.getElementById(id), toggleToolCall, toggleTurnDiff });
  const interruption = document.querySelector("[data-template-mount=interruption-panel]");
  interruption.replaceWith(document.getElementById("runtimeInterruptionTemplate").content.cloneNode(true));
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
