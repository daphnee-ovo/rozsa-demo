import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";

export async function checkResourceLoading(outDir: string): Promise<{ html: string; styles: string[]; scripts: string[]; maps: string[] }> {
  const html = resolve(outDir, "browser/renderer/index.html");
  const text = await readFile(html, "utf8");
  const styles = [...text.matchAll(/<link\b[^>]*href="([^"]+)"/g)].map(match => resolve(dirname(html), match[1]));
  const scripts = [...text.matchAll(/<script\b[^>]*src="([^"]+)"/g)].map(match => resolve(dirname(html), match[1]));
  assert.equal(styles.length, 2);
  assert.equal(scripts.length, 1);
  for (const path of [...styles, ...scripts]) assert.ok((await readFile(path)).length > 0, path);
  assert.equal(JSON.parse(await readFile(resolve(outDir, "node/package.json"), "utf8")).type, "module");
  const maps: string[] = [];
  async function walk(directory: string): Promise<void> {
    for (const item of await readdir(directory, { withFileTypes: true })) {
      const path = resolve(directory, item.name);
      if (item.isDirectory()) await walk(path);
      else if (item.name.endsWith(".js") || item.name.endsWith(".cjs")) {
        const code = await readFile(path, "utf8");
        const reference = /\/\/# sourceMappingURL=(\S+)/.exec(code);
        assert.ok(reference, `Missing source map: ${path}`);
        const mapPath = resolve(dirname(path), reference[1]);
        const map = JSON.parse(await readFile(mapPath, "utf8")) as { version: number; sources: string[]; sourcesContent: string[]; mappings: string };
        assert.equal(map.version, 3); assert.ok(map.mappings.length > 0);
        assert.equal(map.sources.length, map.sourcesContent.length);
        for (const [i, source] of map.sources.entries()) {
          assert.equal(map.sourcesContent[i], await readFile(resolve(dirname(mapPath), source), "utf8"), `Map source: ${source}`);
        }
        maps.push(mapPath);
        // Generated renderer imports must resolve too, including shared contract types/runtime.
        if (path.startsWith(resolve(outDir, "browser"))) {
          for (const match of code.matchAll(/(?:from\s+|import\s*)["'](\.[^"']+)["']/g)) await readFile(resolve(dirname(path), match[1]));
        }
      }
    }
  }
  await walk(outDir);
  assert.ok(maps.some(path => path.endsWith("preload.cjs.map")));
  assert.ok(maps.includes(resolve(outDir, "node/main/entry.js.map")));
  assert.ok(maps.includes(resolve(outDir, "browser/renderer/entry.js.map")));
  return { html, styles, scripts, maps };
}
