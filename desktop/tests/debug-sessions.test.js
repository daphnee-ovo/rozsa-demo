import assert from "node:assert/strict";
import test from "node:test";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { mkdir, mkdtemp, readFile, readdir, stat, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { prepareDebugSessions } from "../../devtools/prepare-debug-sessions.mjs";

const root = resolve(import.meta.dirname, "../..");
const execute = promisify(execFile);
async function temporaryDirectory() {
  await mkdir(join(root, "tmp"), { recursive: true });
  return mkdtemp(join(root, "tmp/debug-session-test-"));
}

test("debug fixtures include readable messages, complete diagrams, tool results and long history without overwriting edits", async () => {
  const directory = await temporaryDirectory();
  await prepareDebugSessions(directory);
  const files = await readdir(directory);
  assert.equal(files.length, 3);
  const sessions = await Promise.all(files.map(async file => (await readFile(join(directory, file), "utf8")).trim().split("\n").map(JSON.parse)));
  for (const entries of sessions) {
    assert.equal(entries[0].type, "session");
    let lastId = null;
    for (const entry of entries.slice(1)) {
      assert.equal(entry.parentId, lastId);
      lastId = entry.id;
    }
    assert.ok(entries.some(entry => entry.message?.role === "assistant" && entry.message.api));
    assert.ok(entries.some(entry => entry.message?.role === "toolResult"));
  }
  assert.ok(sessions.some(entries => entries.length > 160));
  assert.ok(JSON.stringify(sessions).includes("sequenceDiagram"));
  const edited = join(directory, "welcome.jsonl");
  await writeFile(edited, "user-edited debug history\n");
  const modified = (await stat(edited)).mtimeMs;
  await prepareDebugSessions(directory);
  assert.equal(await readFile(edited, "utf8"), "user-edited debug history\n");
  assert.equal((await stat(edited)).mtimeMs, modified);
});

test("launcher forwards spaced application arguments unchanged", { skip: process.platform === "win32" ? "Requires the POSIX Bash launcher" : false }, async () => {
  const base = await temporaryDirectory(), bin = join(base, "bin"), result = join(base, "arguments.json");
  await mkdir(bin);
  // Generated command doubles keep this test from starting Forge or changing backend config.
  await writeFile(join(bin, "go"), "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  await writeFile(join(bin, "npm"), `#!${process.execPath}\nrequire('node:fs').writeFileSync(process.env.DEBUG_ARGUMENTS,JSON.stringify({arguments:process.argv.slice(2),configDir:process.env.ROZSA_CONFIG_DIR}));\n`, { mode: 0o755 });
  await writeFile(join(bin, "node"), `#!${process.execPath}\nconst args=process.argv.slice(2);if(args[0].endsWith('update-builtin-skills-config.mjs'))process.exit(0);if(args[0].endsWith('prepare-debug-sessions.mjs'))args.push(process.env.DEBUG_SESSIONS);const child=require('node:child_process').spawnSync(process.execPath,args,{stdio:'inherit'});if(child.error)throw child.error;process.exit(child.status);\n`, { mode: 0o755 });
  const env = { ...process.env, PATH: `${bin}:${process.env.PATH}`, DEBUG_ARGUMENTS: result };
  const script = join(root, "devtools/start-desktop.sh");
  const readInvocation = async () => JSON.parse(await readFile(result, "utf8"));
  const expectedArguments = ["--prefix", "desktop", "start", "--", "--", "--user-data-dir", join(base, "custom profile")];
  await execute("/bin/bash", [script, "--user-data-dir", join(base, "custom profile")], { cwd: base, env });
  assert.deepEqual(await readInvocation(), { arguments: expectedArguments, configDir: join(root, "tmp", "desktop-debug") });

  const override = join(base, "override config");
  await execute("/bin/bash", [script, "--user-data-dir", join(base, "custom profile")], { cwd: base, env: { ...env, ROZSA_CONFIG_DIR: override } });
  assert.deepEqual(await readInvocation(), { arguments: expectedArguments, configDir: override });
});
