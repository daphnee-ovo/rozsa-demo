import { VitePlugin } from "@electron-forge/plugin-vite";
import { resolve } from "node:path";
import { readFileSync } from "node:fs";
import { backendPath, prepareCandidate, projectRoot } from "./scripts/backend.mjs";

export default {
  packagerConfig: {
    name: "Rozsa Migration", executableName: "rozsa-migration",
    asar: true, extraResource: [backendPath],
    // Packager scratch files stay in the project; tool caches use defaults.
    tmpdir: resolve(projectRoot, "tmp/migration/packager"),
    // The pinned npm Electron package supplies official release hashes. Keep
    // verification enabled without fetching mutable checksum metadata each run.
    download: { checksums: JSON.parse(readFileSync(new URL("./node_modules/electron/checksums.json", import.meta.url), "utf8")) },
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
