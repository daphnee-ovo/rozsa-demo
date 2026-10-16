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
  });
  api.byId("runningSendMode")?.addEventListener("change", api.updateAbortButton);
}

export function renderSendState(button, action) {
  const labels = { send: "Send message", stop: "Stop", queue: "Queue message", steer: "Steer" };
  if (!Object.hasOwn(labels, action)) throw new TypeError("Unsupported composer action");
  button.dataset.sendAction = action;
  button.title = labels[action];
  button.setAttribute("aria-label", labels[action]);
  button.innerHTML = action === "stop"
    ? '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="6" y="6" width="12" height="12" rx="2" fill="currentColor"/></svg>'
    : '<svg class="lucide" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></svg>';
}
