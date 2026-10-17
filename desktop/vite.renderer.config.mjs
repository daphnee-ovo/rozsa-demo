import { readFileSync, readdirSync, statSync } from "node:fs";
import { readFile as readFileAsync } from "node:fs/promises";
import { join, relative, resolve, sep } from "node:path";
import { defineConfig } from "vite";

const rendererRoot = resolve(import.meta.dirname, "renderer");
const appScriptNames = readFileSync(join(rendererRoot, "runtime/app/order.txt"), "utf8").trim().split("\n");

function appSource() {
  return appScriptNames.map(name => readFileSync(join(rendererRoot, "runtime/app", name), "utf8")).join("\n");
}

function emitRuntimeAssets() {
  const appRoot = resolve(rendererRoot, "runtime/app");
  return {
    name: "emit-existing-renderer-runtime-assets",
    apply: "build",
    buildStart() {
      for (const name of ["gui_shared.js"]) {
        this.emitFile({ type: "asset", fileName: name, source: readFileSync(join(rendererRoot, name)) });
      }
      this.emitFile({ type: "asset", fileName: "runtime/app.js", source: appSource() });
      const visit = directory => {
        for (const name of readdirSync(directory)) {
          const path = join(directory, name);
          if (statSync(path).isDirectory()) visit(path);
          else if (path.endsWith(".html")) {
            this.emitFile({ type: "asset", fileName: relative(rendererRoot, path).split(sep).join("/"), source: readFileSync(path) });
          }
        }
      };
      visit(join(rendererRoot, "templates"));
    },
  };
}

function serveAppScripts() {
  const appRoot = resolve(rendererRoot, "runtime/app");
  return {
    name: "serve-classic-app-scripts",
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        if (request.method !== "GET" || !request.url) return next();
        let pathname;
        try { pathname = decodeURIComponent(new URL(request.url, "http://localhost").pathname); }
        catch (error) { return next(error); }
        if (pathname !== "/runtime/app.js") return next();
        void Promise.all(appScriptNames.map(name => readFileAsync(join(appRoot, name), "utf8"))).then(sources => {
          response.setHeader("Content-Type", "text/javascript; charset=utf-8");
          response.end(sources.join("\n"));
        }, next);
      });
    },
  };
}

function serveSharedTemplates() {
  const templateRoot = resolve(rendererRoot, "templates");
  return {
    name: "serve-shared-ui-templates",
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        if (request.method !== "GET" || !request.url) return next();
        let pathname;
        try { pathname = decodeURIComponent(new URL(request.url, "http://localhost").pathname); }
        catch (error) { return next(error); }
        if (!pathname.startsWith("/templates/") || !pathname.endsWith(".html")) return next();
        const file = resolve(templateRoot, pathname.slice("/templates/".length));
        if (file !== templateRoot && !file.startsWith(`${templateRoot}${sep}`)) {
          response.statusCode = 400;
          response.end("Invalid template path");
          return;
        }
        void readFileAsync(file).then(source => {
          response.setHeader("Content-Type", "text/html; charset=utf-8");
          response.end(source);
        }, next);
      });
    },
  };
}

export default defineConfig({
  root: rendererRoot,
  base: "./",
  // Electron and its prototype own these sources; the Rust GUI embeds its own frontend.
  server: { host: "127.0.0.1", fs: { allow: [resolve(import.meta.dirname, "..")] } },
  build: {
    rollupOptions: { input: { index: resolve(rendererRoot, "index.html") } },
    outDir: resolve(import.meta.dirname, ".vite/renderer/sessions"),
    emptyOutDir: true,
    sourcemap: true,
  },
  plugins: [emitRuntimeAssets(), serveAppScripts(), serveSharedTemplates(), {
    name: "candidate-dev-csp",
    apply: "serve",
    transformIndexHtml(html) {
      return html.replace("connect-src 'none'", "connect-src 'self' ws://127.0.0.1:*");
    },
  }],
});
