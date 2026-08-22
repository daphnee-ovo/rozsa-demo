export function init(api) {
  if (api.boundModules.has("scenario-flow")) return;
  api.boundModules.add("scenario-flow");

  const shell = document.querySelector("[data-application-shell]");
  document.addEventListener("rozsa:scene-ready", event => {
    const scene = event.detail?.scene || "";
    if (shell) {
      shell.dataset.scenarioState = "ready";
      if (scene) shell.dataset.scenarioId = scene;
      else delete shell.dataset.scenarioId;
    }
    document.dispatchEvent(new CustomEvent("rozsa:scenario-ready", { detail: { scene } }));
  }, { once: true });
}
