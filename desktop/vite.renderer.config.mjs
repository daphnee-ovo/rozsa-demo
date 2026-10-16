import { resolve } from "node:path";
import { defineConfig } from "vite";

const rendererRoot = resolve(import.meta.dirname, "renderer");

export default defineConfig({
  root: rendererRoot,
  base: "./",
  // Electron and its prototype own these sources; the Rust GUI embeds its own frontend.
  server: { host: "127.0.0.1", fs: { allow: [resolve(import.meta.dirname, "..")] } },
  build: {
    outDir: resolve(import.meta.dirname, ".vite/renderer/sessions"),
    emptyOutDir: true,
    sourcemap: true,
  },
  plugins: [{
    name: "candidate-dev-csp",
    apply: "serve",
    transformIndexHtml(html) {
      return html.replace("connect-src 'none'", "connect-src 'self' ws://127.0.0.1:*");
    },
  }],
});
