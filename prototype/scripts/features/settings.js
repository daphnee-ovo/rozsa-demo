export function init(api) {
  if (api.boundModules.has("settings")) return;
  api.boundModules.add("settings");

  const settings = api.byId("settingsPanel");
  settings?.addEventListener("click", event => {
    const tab = event.target.closest(".settings-tab");
    if (tab) {
      api.switchSettingsTab(tab.dataset.settingsPane, tab);
      return;
    }
    const themeCard = event.target.closest("[data-theme-mode-card]");
    if (themeCard) {
      api.selectThemeModeCard(themeCard.dataset.themeModeCard);
      return;
    }
    const toggle = event.target.closest(".setting-toggle");
    if (toggle) {
      api.toggleSetting(toggle);
      return;
    }
    const action = event.target.closest("[data-action]")?.dataset.action;
    if (action === "close-settings") {
      api.closeSettings();
      return;
    }
    const permissionAction = event.target.closest("[data-permission-action]")?.dataset.permissionAction;
    if (permissionAction) {
      const section = event.target.closest(".permission-rule-section");
      const type = section?.querySelector(".permission-rule-list")?.id.replace("permission", "").replace("List", "").toLowerCase();
      if (permissionAction === "add-rule" && type) api.openPermissionRuleEditor(type);
      else if (permissionAction.startsWith("reset-")) api.resetPermissionRules(permissionAction.slice(6));
      else if (permissionAction === "cancel-editor") api.closePermissionRuleEditor();
      else if (permissionAction === "save-editor") api.savePermissionRule();
    }
  });
  settings.addEventListener("input", event => {
    if (event.target.id === "settingsFontSizeRange" || event.target.id === "settingsFontSizeInput") api.setFontSize(event.target.value);
    if (event.target.id === "shortcutSearch") api.renderKeyBindings(event.target.value);
  });
  api.byId("settingsModelSelect")?.addEventListener("change", event => api.onModelChange(event.currentTarget.value));
}

