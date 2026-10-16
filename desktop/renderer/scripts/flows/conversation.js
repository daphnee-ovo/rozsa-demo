import { loadTemplate } from "../core/template-loader.js";
import { mountShell, templateRoot } from "../core/shell.js";

export async function mountConversation(root) {
  const template = await loadTemplate(new URL("features/conversation.html", templateRoot));
  root.replaceChildren(...template.firstElementChild.childNodes);
  await mountShell([
    { url: new URL("composites/composer.html", templateRoot), selector: "[data-template-mount=composer]", children: true },
    { url: new URL("primitives/input.html", templateRoot), selector: "[data-template-mount=input]", children: true },
  ]);
}

export function createConversationFlow(adapter) {
  return Object.freeze({
    attachFileReference: () => adapter.attachFile(),
    attachDirectoryReference: () => adapter.attachDirectory(),
    insertSlashCommandPrefix: () => adapter.slashCommand(),
    showModelPicker: () => adapter.chooseModel(),
    addMessage: () => adapter.submit(),
    updateAbortButton: () => adapter.refreshSendState(),
  });
}
