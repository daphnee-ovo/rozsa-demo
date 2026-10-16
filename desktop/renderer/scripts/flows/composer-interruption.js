export function init(api) {
  if (api.boundModules.has("composer-interruption")) return;
  api.boundModules.add("composer-interruption");

  const shell = document.querySelector("[data-application-shell]");
  const setState = (state, kind) => {
    if (!shell) return;
    shell.dataset.interruptionState = state;
    if (kind) shell.dataset.interruptionKind = kind;
    else delete shell.dataset.interruptionKind;
  };
  const isVisible = id => api.byId(id)?.classList.contains("visible");
  const restoreComposerFocus = kind => {
    if (isVisible("permPanel") || isVisible("questionPanel")) {
      setState("open", kind);
      return;
    }
    setState("closed", kind);
    const messageInput = api.byId("msgInput");
    if (messageInput?.getClientRects().length) {
      messageInput.focus();
    } else {
      const activeElement = document.activeElement;
      if (activeElement instanceof HTMLElement && activeElement !== document.body) {
        activeElement.blur();
      }
    }
  };

  document.addEventListener("rozsa:interruption-open", event => {
    setState("open", event.detail?.kind);
  });
  document.addEventListener("rozsa:interruption-close", event => {
    restoreComposerFocus(event.detail?.kind);
  });
  document.addEventListener("rozsa:scenario-ready", () => {
    if (isVisible("permPanel")) setState("open", "permission");
    else if (isVisible("questionPanel")) setState("open", "question");
    else setState("closed");
  });
}
