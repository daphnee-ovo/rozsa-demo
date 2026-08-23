export function init(api) {
  if (api.boundModules.has("popover")) return;
  api.boundModules.add("popover");

  api.byId("thinkingEffort")?.addEventListener("click", api.toggleThinkingEffortPicker);
  const slider = api.byId("thinkingEffortSlider");
  slider?.addEventListener("input", event => api.previewThinkingEffort(event.currentTarget.value));
  slider?.addEventListener("change", event => api.selectThinkingEffort(event.currentTarget.value));
}

