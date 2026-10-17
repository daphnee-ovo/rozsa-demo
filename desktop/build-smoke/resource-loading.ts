import assert from "node:assert/strict";
import { readFile, readdir, access } from "node:fs/promises";
import { dirname, resolve } from "node:path";

export async function checkResourceLoading(outDir: string) {
  const html = resolve(outDir, "renderer/sessions/index.html");
  const text = await readFile(html, "utf8");
  const paths = (tag: string, attribute: string) => [...text.matchAll(new RegExp(`<${tag}\\b[^>]*${attribute}="([^"]+)"`, "g"))].map(m => resolve(dirname(html), m[1]));
  const styles = paths("link", "href").filter(p => p.endsWith(".css"));
  const scripts = paths("script", "src");
  assert.ok(styles.length > 0);
  assert.ok(scripts.some(path => path.endsWith('/runtime/app.js')), 'combined app runtime resource is referenced');
  assert.match(text, /connect-src 'self'/); assert.doesNotMatch(text, /entry\.ts|@vite\/client/);
  for (const path of [...styles, ...scripts]) assert.ok((await readFile(path)).length > 0, path);
  const css = (await Promise.all(styles.map(path => readFile(path, "utf8")))).join("\n");
  assert.match(css, /--font-ui/); assert.match(css, /session-item/);
  const maps: string[] = [];
  for (const directory of ["build", "renderer/sessions/assets"]) {
    for (const name of await readdir(resolve(outDir, directory))) {
      if (!name.endsWith(".map")) continue;
      const path = resolve(outDir, directory, name);
      const map = JSON.parse(await readFile(path, "utf8"));
      assert.equal(map.version, 3); assert.ok(map.mappings.length > 0);
      assert.ok(map.sources.length > 0); assert.equal(map.sources.length, map.sourcesContent.length);
      for (const [i, source] of map.sources.entries()) {
        assert.equal(map.sourcesContent[i], await readFile(resolve(dirname(path), source), "utf8"), source);
      }
      maps.push(path);
    }
  }
  for (const name of ["main", "preload"]) {
    await access(resolve(outDir, `build/${name}.cjs`));
    assert.ok(maps.includes(resolve(outDir, `build/${name}.cjs.map`)));
  }
  assert.ok(maps.some(path => path.includes("/assets/")));
  const preload = await readFile(resolve(outDir, "build/preload.cjs"), "utf8");
  assert.match(preload, /require\(["']electron["']\)/);
  assert.doesNotMatch(preload, /require\(["']\.\//); // Sandbox has no filesystem module loader.
  return { html, styles, scripts, maps };
}
