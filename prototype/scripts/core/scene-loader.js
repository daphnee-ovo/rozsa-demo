import { getSceneDefinition, resolveSceneId } from "../../scenarios/registry.js";

export async function loadPrototypeScene(search = window.location.search) {
  const requested = new URLSearchParams(search).get("scene") || document.documentElement.dataset.rozsaScene;
  const id = resolveSceneId(requested);
  const definition = getSceneDefinition(id);
  return window.RozsaUIHost.port.request("load-scene", definition);
}

export function applyPrototypeScene(api, scene) {
  if (!scene || !scene.definition) throw new Error("Prototype scene data is unavailable");
  api.applySceneFixture(scene);
}
