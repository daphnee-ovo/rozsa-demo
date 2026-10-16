import { loadTemplate } from "./template-loader.ts";
import { mountTemplateChildren } from "./renderer.ts";
import { mountShell, resolveTemplateRoot } from "../../../desktop/renderer/scripts/core/shell.ts";
import { resolveSceneId } from "../../scenarios/registry.ts";
import { createUIHost } from "../../../desktop/renderer/scripts/core/renderer.ts";
import { createMockAdapter } from "../adapters/mock-adapter.ts";
import { bindViewport } from '../../../desktop/renderer/scripts/core/viewport.ts';

window.RozsaUIHost = createUIHost(createMockAdapter());
window.addEventListener("pagehide", () => window.RozsaUIHost.port.dispose(), { once: true });

const mounts: [string, string][] = [
  ["templates/features/sidebar.html", "[data-template-mount=\"sidebar\"]"],
  ["templates/features/conversation.html", "[data-template-mount=\"conversation\"]"],
  ["templates/features/settings.html", "[data-template-mount=\"settings\"]"],
  ["templates/composites/popover.html", "[data-template-mount=\"popover\"]"],
  ["templates/composites/notification.html", "[data-template-mount=\"notification\"]"],
  ["templates/features/dev-flow.html", "[data-template-mount=\"dev-flow\"]"],
  ["templates/composites/composer.html", "[data-template-mount=\"composer\"]"],
];

async function bootstrapPrototypeShell() {
  await bindViewport(window);
  const sceneId = resolveSceneId(new URLSearchParams(window.location.search).get("scene"));
  const templateRoot = resolveTemplateRoot();
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

window.RozsaUIReady = bootstrapPrototypeShell();
window.RozsaUIReady.catch(error => {
  console.error("Prototype application shell failed to load.", error);
  const root = document.querySelector("[data-application-shell]");
  if (root) {
    (root as HTMLElement).dataset.shellError = "template-load-failed";
    const message = document.createElement("p");
    message.setAttribute("role", "alert");
    message.textContent = error instanceof Error ? error.message : String(error);
    root.appendChild(message);
  }
});
