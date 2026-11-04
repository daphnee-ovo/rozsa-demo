import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const testDirectoryRoot = join(repositoryRoot, "testdata", "project-test-dirs");
const configRoot = join(repositoryRoot, "testdata", "rozsa-config-dev");
const modelPort = 50805;
const timestamp = "2026-10-23T00:00:00Z";
const explicitSubtests = [
  "TestRuntimeRegistersProductionToolsAndPersistsResults/read",
  "TestRuntimeRegistersProductionToolsAndPersistsResults/write",
  "TestRuntimeRegistersProductionToolsAndPersistsResults/bash",
  "TestDeniedHintCancellationAndFailedResponseRemainSafe/deny_does_not_write",
  "TestDeniedHintCancellationAndFailedResponseRemainSafe/denied_hint_does_not_write",
  "TestDeniedHintCancellationAndFailedResponseRemainSafe/cancel_closes_the_waiter",
  "TestDeniedHintCancellationAndFailedResponseRemainSafe/failed_project_persistence_remains_retryable",
  "TestRuntimeQuestionAndApprovalCleanupUsesOriginSession/approval_cancellation_preserves_the_other_session_request",
  "TestSubagentAbortAndShutdownCancelOwnQuestions/abort",
  "TestSubagentAbortAndShutdownCancelOwnQuestions/shutdown",
];
const runtimeSessionTargets = [
  "TestApprovalWaitsBeforeWriteAndAppliesSessionTrust",
  "TestProjectTrustPersistsThroughLayeredSettings",
  "TestSubagentProductionToolsCheckScopeBeforePermission",
  ...explicitSubtests,
];
const obsoleteSubtestTargets = [
  "TestDeniedHintCancellationAndFailedResponseRemainSafe-TestRuntimeQuestionAndApprovalCleanupUsesOriginSession-approval_cancellation_preserves_the_other_session_request",
  "TestDeniedHintCancellationAndFailedResponseRemainSafe-TestSubagentAbortAndShutdownCancelOwnQuestions-abort",
  "TestDeniedHintCancellationAndFailedResponseRemainSafe-TestSubagentAbortAndShutdownCancelOwnQuestions-shutdown",
];

function targetName(name) {
  return name.replace(/[^a-zA-Z0-9_-]+/g, "-");
}

function projectKey(projectDir) {
  return `-${projectDir.replace(/[\\/]/g, "-").replace(/^-+|-+$/g, "")}-`;
}

async function writeJSON(path, value) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, `${JSON.stringify(value)}\n`);
}

async function writeText(path, content) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, content);
}

async function writeSession(path, id, cwd, entries = []) {
  const records = [{ type: "session", version: 3, id, timestamp, cwd }];
  let parentId = null;
  entries.forEach((entry, index) => {
    const entryId = `fixture-${index + 1}`;
    records.push({ id: entryId, parentId, timestamp, ...entry });
    parentId = entryId;
  });
  await writeText(path, `${records.map((record) => JSON.stringify(record)).join("\n")}\n`);
}

async function testTargets() {
  const files = [
    "backend/cmd/rozsa-backend/main_test.go",
    "backend/cmd/rozsa-backend/process_test.go",
    "backend/internal/app/runtime/subagents_test.go",
    "backend/internal/app/runtime/lifecycle_test.go",
    "backend/internal/app/runtime/runtime_test.go",
    "backend/internal/app/runtime/resources_test.go",
    "backend/internal/app/runtime/question_tool_test.go",
    "backend/internal/app/runtime/prompt_test.go",
    "backend/internal/app/runtime/messages_test.go",
    "backend/internal/app/runtime/model_settings_test.go",
    "backend/internal/app/runtime/production_tools_test.go",
  ];
  const targets = new Set();
  for (const file of files) {
    const source = await readFile(join(repositoryRoot, file), "utf8");
    for (const match of source.matchAll(/^func (Test[A-Za-z0-9_]+)\(t \*testing\.T\)/gm)) {
      targets.add(targetName(match[1]));
    }
  }
  for (const name of explicitSubtests) {
    targets.add(targetName(name));
  }
  return [...targets];
}

async function generateRuntimeSessionFixtures() {
  for (const name of runtimeSessionTargets) {
    const cwd = join(testDirectoryRoot, targetName(name));
    await writeSession(join(cwd, "sessions", "runtime.jsonl"), "runtime", cwd);
  }
}

async function generateProjectDirectories() {
  for (const target of await testTargets()) {
    const targetDirectory = join(testDirectoryRoot, target);
    await rm(targetDirectory, { recursive: true, force: true });
    await mkdir(targetDirectory, { recursive: true });
  }
  for (const target of obsoleteSubtestTargets) {
    await rm(join(testDirectoryRoot, target), { recursive: true, force: true });
  }
}

async function generateSharedConfiguration() {
  const syntheticModel = JSON.parse(
    await readFile(
      join(repositoryRoot, "testdata", "fixtures", "backend", "synthetic.json"),
      "utf8",
    ),
  );
  syntheticModel.providers.synthetic.baseUrl = `http://127.0.0.1:${modelPort}/v1`;
  await rm(join(configRoot, "models", "test.json"), { force: true });
  await rm(join(configRoot, "models", "title-model.json"), { force: true });
  await writeJSON(join(configRoot, "settings.json"), {
    defaultProvider: "synthetic",
    defaultModel: "test",
  });
  await writeJSON(join(configRoot, "models", "synthetic.json"), syntheticModel);
  await writeText(
    join(configRoot, "skills", ".builtin", "rozsa-docs", "SKILL.md"),
    "---\nname: rozsa-docs\ndescription: process test fixture\n---\n",
  );
}

async function generateProcessFixtures() {
  const cases = [
    ["TestProcessPublishesInitializationFailureWithoutMessageRequest", "missing"],
    ["TestProductRPCShutdownDrainsTwoSessionWritesBeforeAcknowledging", "test"],
    ["TestStdioProcessProtocolAndDiagnostics", "test"],
  ];
  for (const [name, model] of cases) {
    const cwd = join(testDirectoryRoot, name);
    if (model !== "test") {
      await writeJSON(join(cwd, ".rozsa", "settings.json"), {
        defaultProvider: "synthetic",
        defaultModel: model,
      });
    }
    const sessions = join(configRoot, "sessions", projectKey(cwd));
    if (name === "TestProductRPCShutdownDrainsTwoSessionWritesBeforeAcknowledging") {
      await writeSession(join(sessions, "a.jsonl"), "a", cwd);
      await writeSession(join(sessions, "b.jsonl"), "b", cwd);
    } else {
      await mkdir(sessions, { recursive: true });
    }
  }
}

async function generateCoordinatorFixture() {
  const cwd = join(
    testDirectoryRoot,
    "TestRuntimeCoordinatorServesSessionsAndAdoptsSelectionBeforeReady",
  );
  const sessions = join(cwd, "sessions");
  await writeSession(join(sessions, "selected.jsonl"), "selected", cwd);
  await writeSession(join(sessions, "other.jsonl"), "other", cwd);
}

async function generateLayeredPromptFixture() {
  const root = join(
    testDirectoryRoot,
    "TestProductionRuntimeBuildsLayeredPromptAndReloadsPerSession",
  );
  const globalRoot = join(root, "global");
  const home = join(root, "home");
  const cwdA = join(root, "project-a", "work");
  const cwdB = join(root, "project-b", "work");

  await writeText(join(globalRoot, "AGENTS.md"), "global resource");
  await writeText(join(cwdA, ".rozsa", "AGENTS.md"), "project A resource");
  await writeText(join(cwdB, ".rozsa", "AGENTS.md"), "project B resource");
  await writeText(join(root, "project-a", "AGENTS.md"), "project A ancestor");
  await writeText(join(cwdA, "AGENTS.md"), "project A cwd");
  await writeText(join(cwdB, "AGENTS.md"), "project B cwd");
  await writeText(
    join(globalRoot, "skills", "shared", "SKILL.md"),
    "---\ndescription: global shared\n---\nglobal shared body",
  );
  await writeText(
    join(home, ".agents", "skills", "shared", "SKILL.md"),
    "---\ndescription: agents shared\n---\nagents shared body",
  );
  await writeText(
    join(cwdA, ".rozsa", "skills", "shared", "SKILL.md"),
    "---\ndescription: project shared\n---\nproject shared body",
  );
  await writeText(
    join(globalRoot, "skills", "global-only", "SKILL.md"),
    "---\ndescription: global only\n---\nglobal only body",
  );
  await writeText(
    join(cwdB, ".rozsa", "skills", "b-only", "SKILL.md"),
    "---\ndescription: project B only\n---\nproject B body",
  );
  await writeJSON(join(globalRoot, "settings.json"), { skills: { "global-only": false } });
  await writeSession(join(root, "sessions-a", "a.jsonl"), "session-a", cwdA);
  await writeSession(join(root, "sessions-b", "b.jsonl"), "session-b", cwdB, [
    {
      type: "message",
      message: {
        role: "user",
        content: [{ type: "text", text: "existing B history" }],
        timestamp: 1,
      },
    },
  ]);
}

async function generateModelAndSessionFixtures() {
  const imageRoot = join(
    testDirectoryRoot,
    "TestBlockImagesRemovesImageBlocksFromProviderInputAndKeepsSessionImage",
  );
  await writeSession(join(imageRoot, "sessions", "images.jsonl"), "images", imageRoot, [
    {
      type: "message",
      message: {
        role: "user",
        content: [
          { type: "text", text: "keep this" },
          { type: "image", data: "image-data", mimeType: "image/png" },
        ],
        timestamp: 0,
      },
    },
  ]);

  const manualRoot = join(
    testDirectoryRoot,
    "TestGenerateSessionNameUsesSmallModelAndPreservesManualRename",
  );
  await writeSession(join(manualRoot, "sessions", "manual", "manual.jsonl"), "manual", manualRoot, [
    { type: "session_info", name: "manual rename" },
  ]);

  const compactionRoot = join(
    testDirectoryRoot,
    "TestAutoCompactionUsesConfiguredRatiosAndRestoresSummaryContext",
  );
  const history = [];
  for (let index = 0; index < 6; index++) {
    history.push({
      type: "message",
      message: {
        role: "user",
        content: [{ type: "text", text: "earlier detail ".repeat(8) }],
        timestamp: 0,
      },
    });
  }
  history.push({
    type: "message",
    message: {
      role: "assistant",
      content: [],
      api: "",
      provider: "provider",
      model: "model",
      timestamp: 0,
      stopReason: "stop",
      usage: {
        input: 90,
        output: 2,
        cacheRead: 0,
        cacheWrite: 0,
        totalTokens: 92,
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
      },
    },
  });
  history.push({
    type: "message",
    message: {
      role: "user",
      content: [{ type: "text", text: "continue after the long context" }],
      timestamp: 0,
    },
  });
  await writeSession(
    join(compactionRoot, "sessions", "compaction.jsonl"),
    "compaction",
    compactionRoot,
    history,
  );

  const forkRoot = join(testDirectoryRoot, "TestForkCopiesOnlyPrefixThroughUserAndRestoresContext");
  const forkMessages = [
    { role: "user", content: [{ type: "text", text: "first" }], timestamp: 0 },
    {
      role: "assistant",
      content: [{ type: "text", text: "answer" }],
      api: "",
      provider: "",
      model: "",
      timestamp: 0,
      stopReason: "stop",
      usage: {
        input: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite: 0,
        totalTokens: 0,
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
      },
    },
    { role: "user", content: [{ type: "text", text: "fork here" }], timestamp: 0 },
    {
      role: "assistant",
      content: [{ type: "text", text: "later" }],
      api: "",
      provider: "",
      model: "",
      timestamp: 0,
      stopReason: "stop",
      usage: {
        input: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite: 0,
        totalTokens: 0,
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
      },
    },
  ];
  await writeSession(
    join(forkRoot, "sessions", "source.jsonl"),
    "source",
    forkRoot,
    forkMessages.map((message) => ({ type: "message", message })),
  );
}

async function generateReadToolFixture() {
  const cwd = join(testDirectoryRoot, "TestRuntimeRegistersProductionToolsAndPersistsResults-read");
  await writeText(join(cwd, "input.txt"), "read fixture\n");
}

async function generateSwitchedSessionFixtures() {
  const switchCWD = join(testDirectoryRoot, "TestSwitchSessionChangesTheRuntimeMessageOwner");
  await writeSession(join(switchCWD, "sessions-b", "selected.jsonl"), "selected", switchCWD, [
    {
      type: "message",
      message: { role: "user", content: [{ type: "text", text: "existing" }], timestamp: 1 },
    },
  ]);

  const fixtures = [
    ["TestConcurrentSessionsKeepEventsCancellationAndSelectionIsolated", "session-b", "session-b"],
    ["TestRuntimeQuestionAndApprovalCleanupUsesOriginSession", "b", "header-b"],
    ["TestRuntimeShutdownRejectsNewWorkWithoutChangingSelection", "selected", "selected"],
    ["TestRuntimeReportsWriteFailureWithoutContaminatingOtherSession", "b", "b"],
    [
      "TestRuntimeQuestionAndApprovalCleanupUsesOriginSession/approval_cancellation_preserves_the_other_session_request",
      "approval-b",
      "approval-b",
    ],
  ];
  for (const [target, filename, sessionID] of fixtures) {
    const cwd = join(testDirectoryRoot, targetName(target));
    await writeSession(join(cwd, "sessions", `${filename}.jsonl`), sessionID, "/workspace");
  }
}

async function main() {
  await mkdir(testDirectoryRoot, { recursive: true });
  await generateProjectDirectories();
  await generateSharedConfiguration();
  await generateProcessFixtures();
  await generateCoordinatorFixture();
  await generateLayeredPromptFixture();
  await generateModelAndSessionFixtures();
  await generateReadToolFixture();
  await generateSwitchedSessionFixtures();
  await generateRuntimeSessionFixtures();
  process.stdout.write(`Generated T212 fixtures under ${testDirectoryRoot}\n`);
}

await main();
