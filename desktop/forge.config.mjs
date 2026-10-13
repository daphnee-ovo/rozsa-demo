import { VitePlugin } from "@electron-forge/plugin-vite";
import { resolve } from "node:path";
import { backendPath, prepareCandidate, projectRoot } from "./scripts/backend.mjs";

export default {
  packagerConfig: {
    name: "Rozsa Migration", executableName: "rozsa-migration",
    asar: true, extraResource: [backendPath],
    // Packager scratch files, as well as tool caches, stay inside the project.
    tmpdir: resolve(projectRoot, "tmp/migration/packager"),
    download: { cacheRoot: resolve(projectRoot, "tmp/migration/electron-cache") },
  },
  hooks: {
    preStart: () => prepareCandidate(),
    prePackage: (_config, platform, arch) => prepareCandidate(platform, arch),
  },
  plugins: [new VitePlugin({
    hotRestart: true,
    build: [
      { entry: { main: "main/entry.ts" }, config: "vite.main.config.mjs", target: "main" },
      { entry: { preload: "preload/entry.ts" }, config: "vite.preload.config.mjs", target: "preload" },
    ],
    renderer: [{ name: "sessions", config: "vite.renderer.config.mjs" }],
  })],
};
