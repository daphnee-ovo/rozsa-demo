import { resolve } from "node:path";
import { defineConfig } from "vite";

export default defineConfig({
  root: resolve(import.meta.dirname, "renderer"),
  // Candidate resources are owned by desktop; prototype and Tauri share this source.
  server: { host: "127.0.0.1", fs: { allow: [resolve(import.meta.dirname, "..")] } },
  build: { outDir: resolve(import.meta.dirname, ".vite/renderer/sessions"), sourcemap: true },
  plugins: [{
    name: "candidate-dev-csp",
    apply: "serve",
    transformIndexHtml(html) {
      return html.replace("connect-src 'none'", "connect-src 'self' ws://127.0.0.1:*");
    },
  }],
});
