import { loadTemplate } from "./template-loader.js";
import { mountTemplate } from "./renderer.js";

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
  ["templates/primitives/input.html", "[data-template-mount=\"input\"]"],
];

async function bootstrapPrototypeShell() {
  for (const [relativePath, selector] of mounts) {
    const fragment = await loadTemplate(new URL(relativePath, templateRoot));
    mountTemplate(fragment, selector);
  }

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
