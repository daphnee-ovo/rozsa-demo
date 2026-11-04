// Generate local Electron test configuration and sessions from committed fixtures.
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const config = join(projectRoot, "testdata/rozsa-config-dev");
const fixtureDirectory = join(projectRoot, "testdata/fixtures/desktop");
const timestamp = "2026-10-23T00:00:00Z";

async function readJSON(path) {
  return JSON.parse(await readFile(path, "utf8"));
}

async function writeJSON(path, value) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`);
}

function projectKey(projectDir) {
  const encoded = projectDir.replace(/[\\/]/g, "-").replace(/^-+|-+$/g, "");
  return `-${encoded}-`;
}

const [approval, showcase] = await Promise.all([
  readJSON(join(fixtureDirectory, "approval-question-lifecycle.json")),
  readJSON(join(fixtureDirectory, "interruption-state-showcase.json")),
]);

const targets = [
  {
    name: "approval-question-lifecycle",
    cwd: join(projectRoot, "testdata/project-test-dirs/approval-question-lifecycle"),
    sessions: approval.sessions,
  },
  {
    name: "interruption-state-showcase",
    cwd: join(projectRoot, "testdata/project-test-dirs/interruption-state-showcase"),
    sessions: showcase.sessions,
  },
];

const generatedSessions = {};
for (const { name, cwd, sessions } of targets) {
  await mkdir(cwd, { recursive: true });

  const sessionDirectory = join(config, "sessions", projectKey(cwd));
  await mkdir(sessionDirectory, { recursive: true });
  for (const { id, name: sessionName } of sessions) {
    const records = [
      { type: "session", version: 3, id, timestamp, cwd },
      { type: "session_info", timestamp, name: sessionName },
    ];
    const path = join(sessionDirectory, `${id}.jsonl`);
    await writeFile(path, `${records.map((record) => JSON.stringify(record)).join("\n")}\n`);
    generatedSessions[id] = { path, directory: sessionDirectory };
  }
  process.stdout.write(`${name}: cwd=${cwd} sessions=${sessionDirectory}\n`);
}

await writeJSON(join(config, "generated/desktop-test-environment.json"), {
  sessions: generatedSessions,
});
process.stdout.write(`ROZSA_CONFIG_DIR=${config}\n`);
