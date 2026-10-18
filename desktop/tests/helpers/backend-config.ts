import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const base = join(root, "tmp/backend-test-config");

export async function createIsolatedBackendEnvironment(): Promise<NodeJS.ProcessEnv> {
  await mkdir(base, { recursive: true });
  const configRoot = await mkdtemp(join(base, "run-"));
  const global = join(configRoot, "global"), project = join(configRoot, "project");
  await mkdir(join(global, "models"), { recursive: true });
  await mkdir(project, { recursive: true });
  await writeFile(join(global, "settings.json"), JSON.stringify({ defaultProvider: "synthetic", defaultModel: "test" }));
  await writeFile(join(global, "models", "synthetic.json"), JSON.stringify({ providers: { synthetic: {
    api: "openai-completions", apiKey: "fixture-key", baseUrl: "http://127.0.0.1:1/v1",
    models: [{ id: "test", contextWindow: 4096, maxTokens: 256 }],
  } } }));
  await writeFile(join(project, "settings.json"), "{}\n");
  return { ROZSA_CONFIG_DIR: global, ROZSA_PROJECT_CONFIG_DIR: project };
}
