export function init(api) {
  if (api.boundModules.has("interruption-panel")) return;
  api.boundModules.add("interruption-panel");

  const panel = api.byId("chat-input");
  panel?.addEventListener("click", event => {
    const action = event.target.closest("[data-permission-action]")?.dataset.permissionAction;
    if (!action) return;
    if (action === "allow") api.respondPermission("allow");
    else if (action === "trust") api.enterPermissionTrust();
    else if (action === "deny") api.respondPermission("deny");
    else if (action === "hint") api.enterPermissionHint();
    else if (action === "toggle-command") api.togglePermissionCommand();
    else if (action === "cancel-hint") api.showPermissionMainPage();
    else if (action === "submit-hint") api.submitPermissionHint();
    else if (action === "submit-question") api.submitQuestionDemo();
  });
  api.byId("permHintInput")?.addEventListener("input", event => api.normalizePermissionHint(event.currentTarget));
  api.byId("permHintInput")?.addEventListener("keydown", api.handlePermissionHintKeydown);
}
