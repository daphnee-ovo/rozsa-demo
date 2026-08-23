export function init(api) {
  if (api.boundModules.has("composer")) return;
  api.boundModules.add("composer");

  const input = api.byId("msgInput");
  input?.addEventListener("input", event => api.handleInput(input, event));
  input?.addEventListener("compositionstart", api.handleCompositionStart);
  input?.addEventListener("compositionupdate", event => api.handleCompositionUpdate(input, event));
  input?.addEventListener("compositionend", event => api.handleCompositionEnd(input, event));

  const actions = {
    "attach-file": api.attachFileReference,
    "attach-directory": api.attachDirectoryReference,
    "slash-command": api.insertSlashCommandPrefix,
    "model-picker": api.showModelPicker,
    send: api.addMessage,
  };
  const composer = api.byId("composer") || api.byId("chat-input");
  composer?.addEventListener("click", event => {
    const action = event.target.closest("[data-action]")?.dataset.action;
    const handler = actions[action];
    if (handler) handler(event);
    if (action === "thinking-effort") api.toggleThinkingEffortPicker(event);
  });
  api.byId("runningSendMode")?.addEventListener("change", api.updateAbortButton);
}
