export function init(api) {
  if (api.boundModules.has("sidebar")) return;
  api.boundModules.add("sidebar");

  const sidebar = api.byId("sidebar");
  sidebar?.addEventListener("click", event => {
    const item = event.target.closest(".session-item");
    if (item) activateSession(api, item);
    const action = event.target.closest("[data-action]")?.dataset.action;
    if (action === "new-session") api.newSession();
    if (action === "toggle-settings") api.toggleSettings();
  });
  sidebar?.addEventListener("keydown", event => {
    if (event.key !== "Enter" && event.key !== " ") return;
    const item = event.target.closest(".session-item");
    if (!item) return;
    event.preventDefault();
    activateSession(api, item);
  });
}

function activateSession(api, item) {
  api.state.activeSession = Number(item.dataset.sessionIndex);
  api.renderSessionList();
  api.renderMessages();
  api.saveState();
}

