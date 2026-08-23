import { getSceneDefinition, resolveSceneId } from "../../scenarios/registry.js";
import { adaptSceneData, loadMockSet } from "../adapters/mock-adapter.js";

export async function loadPrototypeScene(search = window.location.search) {
  const requested = new URLSearchParams(search).get("scene") || document.documentElement.dataset.rozsaScene;
  const id = resolveSceneId(requested);
  const definition = getSceneDefinition(id);
  const mocks = await loadMockSet(definition.mocks);
  return adaptSceneData(definition, mocks);
}

export function applyPrototypeScene(api, scene) {
  if (!scene || !scene.definition) throw new Error("Prototype scene data is unavailable");
  api.applySceneFixture(scene);
}
