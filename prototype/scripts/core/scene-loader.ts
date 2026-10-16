import { getSceneDefinition, resolveSceneId } from "../../scenarios/registry.ts";
import type { createUIHost } from "../../../desktop/renderer/scripts/core/renderer.ts";

declare global {
  interface Window { RozsaUIHost: ReturnType<typeof createUIHost> }
}

export async function loadPrototypeScene(search = window.location.search) {
  const requested = new URLSearchParams(search).get("scene") || document.documentElement.dataset.rozsaScene;
  const id = resolveSceneId(requested);
  const definition = getSceneDefinition(id);
  return window.RozsaUIHost.port.request("load-scene", definition);
}

export function applyPrototypeScene(api: { applySceneFixture(scene: unknown): void }, scene: unknown) {
  if (!scene || typeof scene !== "object" || !("definition" in scene)) {
    throw new Error("Prototype scene data is unavailable");
  }
  api.applySceneFixture(scene);
}
