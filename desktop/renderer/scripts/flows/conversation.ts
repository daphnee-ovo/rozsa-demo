import { loadTemplate } from "../core/template-loader.ts";
import { mountShell, resolveTemplateRoot } from "../core/shell.ts";

export interface ConversationFlowAdapter {
  attachFile(): void;
  attachDirectory(): void;
  slashCommand(): void;
  chooseModel(): void;
  submit(): void;
  refreshSendState(): void;
}

export async function mountConversation(root: Element | null): Promise<void> {
  if (!root) throw new Error("Conversation root not found");
  const templateRoot = resolveTemplateRoot();
  const template = await loadTemplate(new URL("features/conversation.html", templateRoot));
  root.replaceChildren(...template.firstElementChild!.childNodes);
  await mountShell([
    { url: new URL("composites/composer.html", templateRoot), selector: "[data-template-mount=composer]", children: true },
    { url: new URL("primitives/input.html", templateRoot), selector: "[data-template-mount=input]", children: true },
  ]);
}

export function createConversationFlow(adapter: ConversationFlowAdapter) {
  return Object.freeze({
    attachFileReference: () => adapter.attachFile(),
    attachDirectoryReference: () => adapter.attachDirectory(),
    insertSlashCommandPrefix: () => adapter.slashCommand(),
    showModelPicker: () => adapter.chooseModel(),
    addMessage: () => adapter.submit(),
    updateAbortButton: () => adapter.refreshSendState(),
  });
}
