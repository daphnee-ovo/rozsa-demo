// The current candidate consumes one HTML template and two verbatim shared styles.
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
export const resourceFiles = [
  ["desktop/renderer/index.html", "browser/renderer/index.html"],
  ["crates/rozsa-gui/frontend/ui/styles/foundations/tokens.css", "browser/ui/tokens.css"],
  ["crates/rozsa-gui/frontend/ui/styles/features/sidebar.css", "browser/ui/sidebar.css"],
];

export async function generateResources(resourceRoot = projectRoot, outDir = resolve(projectRoot, "desktop/dist")) {
  // Read every required input before emitting anything; a missing source is an error.
  const inputs = await Promise.all(resourceFiles.map(async ([source, target]) =>
    [target, await readFile(resolve(resourceRoot, source))]));
  inputs.push(["node/package.json", Buffer.from('{"type":"commonjs"}\n')]);
  for (const [target, bytes] of inputs) {
    const path = resolve(outDir, target);
    await mkdir(dirname(path), { recursive: true });
    await writeFile(path, bytes);
  }
}

export function options(args, allowWatch = false) {
  const result = { resourceRoot: projectRoot, outDir: resolve(projectRoot, "desktop/dist"), watch: false };
  while (args.length) {
    const arg = args.shift();
    if (allowWatch && arg === "--watch") result.watch = true;
    else if (arg === "--resource-root" || arg === "--out-dir") {
      const value = args.shift();
      if (!value || value.startsWith("--")) throw new Error(`${arg} requires a path`);
      result[arg === "--resource-root" ? "resourceRoot" : "outDir"] = resolve(value);
    } else throw new Error(`Unknown argument: ${arg}`);
  }
  return result;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { resourceRoot, outDir } = options(process.argv.slice(2));
  await generateResources(resourceRoot, outDir);
  console.log(`Resources generated: ${outDir}`);
}
