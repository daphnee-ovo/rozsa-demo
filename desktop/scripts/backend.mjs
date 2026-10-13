import { spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { resolve } from "node:path";

export const projectRoot = resolve(import.meta.dirname, "../..");
export const backendName = process.platform === "win32" ? "rozsa-backend.exe" : "rozsa-backend";
export const backendPath = resolve(projectRoot, "dist/migration", backendName);

export function prepareCandidate(platform = process.platform, arch = process.arch) {
  // This task packages the current host. Cross-builds must not embed a host Go binary.
  if (platform !== process.platform || arch !== process.arch) throw new Error(`Candidate package requires host ${process.platform}/${process.arch}; requested ${platform}/${arch}`);
  const check = spawnSync(process.execPath, [resolve(projectRoot, "desktop/scripts/typecheck.mjs")], { stdio: "inherit" });
  if (check.error) throw check.error;
  if (check.status !== 0) throw new Error("Candidate typecheck failed; no build/package started");
  const env = { ...process.env };
  for (const [name, directory] of Object.entries({ GOCACHE: "go-cache", GOTMPDIR: "go-tmp", GOMODCACHE: "go-mod-cache", TMPDIR: "install-tmp" })) {
    const path = resolve(projectRoot, "tmp/migration", directory);
    mkdirSync(path, { recursive: true }); env[name] = path;
  }
  // Explicit target prevents ambient GOOS/GOARCH from producing an unusable binary.
  env.GOOS = platform === "win32" ? "windows" : platform;
  env.GOARCH = arch === "x64" ? "amd64" : arch;
  mkdirSync(resolve(projectRoot, "dist/migration"), { recursive: true });
  const build = spawnSync("go", ["-C", resolve(projectRoot, "backend"), "build", "-o", backendPath, "./cmd/rozsa-backend"], { env, stdio: "inherit" });
  if (build.error) throw build.error;
  if (build.status !== 0) throw new Error("Go backend build failed");
}
