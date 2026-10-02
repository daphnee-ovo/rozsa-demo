import { loadTemplate } from "./template-loader.js";
import { mountTemplateChildren } from "./renderer.js";
import { mountShell } from "../../../crates/rozsa-gui/frontend/ui/scripts/core/shell.js";
import { resolveSceneId } from "../../scenarios/registry.js";
import { createUIHost } from "../../../crates/rozsa-gui/frontend/ui/scripts/core/renderer.js";
import { createMockAdapter } from "../adapters/mock-adapter.js";

window.RozsaUIHost = createUIHost(createMockAdapter());
window.addEventListener("pagehide", () => window.RozsaUIHost.port.dispose(), { once: true });

const templateRoot = new URL("../../", import.meta.url);
const mounts = [
  ["templates/features/sidebar.html", "[data-template-mount=\"sidebar\"]"],
  ["templates/features/conversation.html", "[data-template-mount=\"conversation\"]"],
  ["templates/features/settings.html", "[data-template-mount=\"settings\"]"],
  ["templates/composites/popover.html", "[data-template-mount=\"popover\"]"],
  ["templates/composites/notification.html", "[data-template-mount=\"notification\"]"],
  ["templates/features/dev-flow.html", "[data-template-mount=\"dev-flow\"]"],
  ["templates/composites/composer.html", "[data-template-mount=\"composer\"]"],
];

async function bootstrapPrototypeShell() {
  const sceneId = resolveSceneId(new URLSearchParams(window.location.search).get("scene"));
  await mountShell(mounts.map(([relativePath, selector]) => ({ url: new URL(relativePath, templateRoot), selector })));
  const interruptionPanelFragment = await loadTemplate(
    new URL("templates/composites/interruption-panel.html", templateRoot),
  );
  mountTemplateChildren(interruptionPanelFragment, "[data-template-mount=\"interruption-panel\"]");
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
