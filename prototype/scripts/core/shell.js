import { loadTemplate } from "./template-loader.js";
import { mountTemplate, mountTemplateChildren } from "./renderer.js";
import { getSceneDefinition, resolveSceneId } from "../../scenarios/registry.js";

const templateRoot = new URL("../../", import.meta.url);
const mounts = [
  ["templates/features/sidebar.html", "[data-template-mount=\"sidebar\"]"],
  ["templates/features/conversation.html", "[data-template-mount=\"conversation\"]"],
  ["templates/features/settings.html", "[data-template-mount=\"settings\"]"],
  ["templates/composites/popover.html", "[data-template-mount=\"popover\"]"],
  ["templates/composites/notification.html", "[data-template-mount=\"notification\"]"],
  ["templates/features/dev-flow.html", "[data-template-mount=\"dev-flow\"]"],
  ["templates/composites/composer.html", "[data-template-mount=\"composer\"]"],
  ["templates/composites/interruption-panel.html", "[data-template-mount=\"interruption-panel\"]"],
];

async function loadSceneStylesheet(sceneId) {
  const stylesheet = getSceneDefinition(sceneId).stylesheet;
  if (!stylesheet) return;
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = stylesheet;
  link.dataset.prototypeSceneStylesheet = sceneId;
  await new Promise((resolve, reject) => {
    link.addEventListener("load", resolve, { once: true });
    link.addEventListener("error", () => reject(new Error("Unable to load prototype scene stylesheet: " + stylesheet)), { once: true });
    document.head.appendChild(link);
  });
}

async function bootstrapPrototypeShell() {
  const sceneId = resolveSceneId(new URLSearchParams(window.location.search).get("scene"));
  await loadSceneStylesheet(sceneId);
  for (const [relativePath, selector] of mounts) {
    const fragment = await loadTemplate(new URL(relativePath, templateRoot));
    mountTemplate(fragment, selector);
  }
  const inputFragment = await loadTemplate(new URL("templates/primitives/input.html", templateRoot));
  mountTemplateChildren(inputFragment, "[data-template-mount=\"input\"]");

  const scene = new URLSearchParams(window.location.search).get("scene");
  if (scene && !document.documentElement.dataset.rozsaScene) {
    document.documentElement.dataset.rozsaScene = scene;
  }

  const script = document.createElement("script");
  script.src = "rozsa-gui.js";
  script.async = false;
  script.addEventListener("load", () => {
    if (document.readyState === "complete") {
      document.dispatchEvent(new Event("DOMContentLoaded"));
    }
  });
  document.body.appendChild(script);
}

bootstrapPrototypeShell().catch(error => {
  console.error("Prototype application shell failed to load.", error);
  const root = document.querySelector("[data-application-shell]");
  if (root) {
    root.dataset.shellError = "template-load-failed";
    const message = document.createElement("p");
    message.setAttribute("role", "alert");
    message.textContent = error.message;
    root.appendChild(message);
  }
});
