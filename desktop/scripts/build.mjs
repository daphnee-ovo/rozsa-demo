import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { build as bundle } from "esbuild";
import { generateResources, options, projectRoot, resourceFiles } from "./resources.mjs";

const args = process.argv.slice(2);
const typecheck = args.includes("--typecheck");
const { resourceRoot, outDir, watch } = options(args.filter(arg => arg !== "--typecheck"), true);
if (typecheck && watch) throw new Error("--typecheck cannot be combined with --watch");
const desktop = resolve(projectRoot, "desktop");
const compilerPath = resolve(desktop, "node_modules/typescript/bin/tsc");
const expectedVersion = JSON.parse(await readFile(resolve(desktop, "package.json"), "utf8")).devDependencies.typescript;
const installedVersion = JSON.parse(await readFile(resolve(desktop, "node_modules/typescript/package.json"), "utf8")).version;
if (!expectedVersion.startsWith("7.") || installedVersion !== expectedVersion) {
  throw new Error(`TS7 ${expectedVersion} required; installed ${installedVersion}. Run npm ci in desktop.`);
}
let stopping = false;
let compiler;
let typescriptReady = false;
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => {
  stopping = true;
  compiler?.kill(signal);
});

async function compile(config, flags) {
  if (stopping) return;
  await new Promise((accept, reject) => {
    const child = spawn(process.execPath, [compilerPath, "-p", config, ...flags], { cwd: desktop, stdio: "inherit" });
    compiler = child;
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      compiler = undefined;
      if (stopping) accept();
      else if (code === 0) accept();
      else reject(new Error(`${config} failed (${code ?? signal}); outputs are not ready`));
    });
  });
}

async function build(typescript = true) {
  if (typescript) {
    typescriptReady = false;
    await compile("tsconfig.main.json", ["--outDir", resolve(outDir, "node")]);
    await compile("tsconfig.preload.json", ["--outDir", resolve(outDir, "preload")]);
    await compile("tsconfig.renderer.json", ["--outDir", resolve(outDir, "browser")]);
    // TS7 removed outFile: combine its already checked/emitted JS, not TS sources.
    // esbuild consumes TS7's maps to retain the original embedded preload sources.
    await bundle({ entryPoints: [resolve(outDir, "preload/preload/entry.js")], outfile: resolve(outDir, "preload.cjs"),
      bundle: true, platform: "node", format: "cjs", target: "es2022", external: ["electron"],
      sourcemap: true, sourcesContent: true });
    if (!stopping) typescriptReady = true;
  }
  if (stopping) return;
  await generateResources(resourceRoot, outDir);
  console.log(`Candidate ${typescript ? "build" : "resources"} ready: ${outDir} (TS7 ${installedVersion})`);
}

async function tsFiles(directory) {
  const files = [];
  for (const item of await readdir(directory, { withFileTypes: true })) {
    const path = resolve(directory, item.name);
    if (item.isDirectory()) files.push(...await tsFiles(path));
    else if (item.name.endsWith(".ts")) files.push(path);
  }
  return files;
}

async function fingerprint(paths) {
  const hash = createHash("sha256");
  for (const path of paths.sort()) {
    hash.update(path);
    try { hash.update(await readFile(path)); }
    catch (error) { if (error.code !== "ENOENT") throw error; hash.update("missing"); }
  }
  return hash.digest("hex");
}

async function snapshot() {
  const typescript = [...await tsFiles(resolve(desktop, "main")), ...await tsFiles(resolve(desktop, "preload")), ...await tsFiles(resolve(desktop, "renderer")),
    ...["shared", "main", "preload", "renderer"].map(name => resolve(desktop, `tsconfig.${name}.json`))];
  return {
    typescript: await fingerprint(typescript),
    resources: await fingerprint(resourceFiles.map(([source]) => resolve(resourceRoot, source))),
  };
}

if (typecheck) {
  for (const name of ["main", "preload", "renderer"]) await compile(`tsconfig.${name}.json`, ["--noEmit"]);
  console.log(`TS7 ${installedVersion} typecheck passed`);
} else if (!watch) await build();
else {
  // Content polling is portable and detects atomic saves, removals and new TS files.
  // One build at a time; edits made during compilation are detected on the next pass.
  let previous;
  while (!stopping) {
    try {
      const current = await snapshot();
      if (!previous || current.typescript !== previous.typescript || current.resources !== previous.resources) {
        const compileTS = !typescriptReady || !previous || current.typescript !== previous.typescript;
        previous = current;
        await build(compileTS);
        if (!stopping) console.log("Watch ready; reload renderer for HTML/CSS/renderer changes; restart app for main/preload changes.");
      }
    } catch (error) {
      console.error(`Watch build failed: ${error.message}`);
      // Keep watching for an explicit source correction, without reporting readiness.
    }
    if (!stopping) await new Promise(accept => setTimeout(accept, 300));
  }
}
