export function init(api) {
  if (api.boundModules.has("dev-flow")) return;
  api.boundModules.add("dev-flow");

  api.byId("devFlowDetailClose")?.addEventListener("click", api.closeDevFlowDetail);
  document.addEventListener("pointerdown", event => {
    if (api.byId("devFlowDetail")?.hidden || event.target.closest("#devFlowDetail")) return;
    api.closeDevFlowDetail();
  });
  document.addEventListener("keydown", event => {
    if (event.key === "Escape" && !api.byId("devFlowDetail")?.hidden) {
      event.preventDefault();
      api.closeDevFlowDetail();
    }
  });
  document.addEventListener("click", event => {
    if (event.target.closest("#sidebarDevFlowDashboard")) api.openDevFlowDashboardDemo();
    else if (event.target.closest("#devFlowSidebarSummary, #devFlowSidebarMore, [data-dev-flow-trigger]")) api.openDevFlowDetailDemo();
  });
}
