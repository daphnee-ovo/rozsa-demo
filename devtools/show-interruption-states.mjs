// Launch an isolated Electron + Go app with a permission request and a user question in separate sessions.
import { createServer } from "node:http";
import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { attachPage } from "../desktop/tests/performance/harness.js";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const runsRoot = join(projectRoot, "tmp/interruption-state-showcase");
await mkdir(runsRoot, { recursive: true });
const runRoot = await mkdtemp(join(runsRoot, "run-"));
const config = join(runRoot, "config");
const projectConfig = join(runRoot, "project-config");
const sessions = join(runRoot, "sessions");
const profile = join(runRoot, "profile");
for (const directory of [join(config, "models"), projectConfig, sessions, profile]) {
  await mkdir(directory, { recursive: true });
}

function toolChunk(id, name, args) {
  return [
    { choices: [{ delta: { tool_calls: [{ index: 0, id, type: "function", function: { name, arguments: JSON.stringify(args) } }] } }] },
    { choices: [{ delta: {}, finish_reason: "tool_calls" }], usage: { prompt_tokens: 24, completion_tokens: 5 } },
  ];
}

function textChunk(text) {
  return [
    { choices: [{ delta: { content: text } }] },
    { choices: [{ delta: {}, finish_reason: "stop" }], usage: { prompt_tokens: 18, completion_tokens: 4 } },
  ];
}

function sendCompletion(response, chunks) {
  response.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache", connection: "keep-alive" });
  for (const chunk of chunks) response.write(`data: ${JSON.stringify(chunk)}\n\n`);
  response.write("data: [DONE]\n\n");
  response.end();
}

const shownRequests = new Set();
const permissionTarget = join(runRoot, "permission-demo.txt");
const provider = createServer((request, response) => {
  let body = "";
  request.setEncoding("utf8");
  request.on("data", chunk => { body += chunk; });
  request.on("end", () => {
    try {
      const payload = JSON.parse(body);
      const latestUser = [...(payload.messages || [])].reverse().find(message => message.role === "user");
      const prompt = typeof latestUser?.content === "string" ? latestUser.content
        : Array.isArray(latestUser?.content) ? latestUser.content.map(part => typeof part?.text === "string" ? part.text : "").join("\n") : "";
      if (prompt.includes("SHOW_PERMISSION_DEMO") && !shownRequests.has("permission")) {
        shownRequests.add("permission");
        sendCompletion(response, toolChunk("demo-permission-write", "write", {
          file_path: permissionTarget,
          content: "This file is written only if the permission request is approved.",
        }));
        return;
      }
      if (prompt.includes("SHOW_QUESTION_DEMO") && !shownRequests.has("question")) {
        shownRequests.add("question");
        sendCompletion(response, toolChunk("demo-user-question", "askUserQuestion", { questions: [
          { header: "Mode", question: "Which mode should this session use?", options: [
            { label: "Careful", description: "Review each change" },
            { label: "Fast", description: "Make the change directly" },
          ] },
          { header: "Checks", question: "Which checks should run?", multiSelect: true, options: [
            { label: "Tests", description: "Run focused tests" },
            { label: "Docs", description: "Review documentation" },
          ] },
        ] }));
        return;
      }
      sendCompletion(response, textChunk("Demo provider is ready. The pending interaction remains available in its session."));
    } catch (error) {
      response.writeHead(400, { "content-type": "text/plain" });
      response.end(error instanceof Error ? error.message : String(error));
    }
  });
});

await new Promise((resolveListen, rejectListen) => {
  provider.once("error", rejectListen);
  provider.listen(0, "127.0.0.1", resolveListen);
});
const address = provider.address();
if (!address || typeof address === "string") throw new Error("The demo provider did not bind to a TCP port");

await writeFile(join(config, "settings.json"), JSON.stringify({ defaultProvider: "synthetic", defaultModel: "stream" }));
await writeFile(join(config, "models", "synthetic.json"), JSON.stringify({ providers: { synthetic: {
  api: "openai-completions", apiKey: "fixture-key", baseUrl: `http://127.0.0.1:${address.port}/v1`,
  models: [{ id: "stream", contextWindow: 16384, maxTokens: 512 }],
} } }));
await writeFile(join(projectConfig, "settings.json"), "{}\n");

const timestamp = new Date().toISOString();
for (const [id, name] of [
  ["permission-demo", "Permission Request Demo"],
  ["question-demo", "Ask User Question Demo"],
]) {
  const header = { type: "session", version: 3, id, timestamp, cwd: runRoot };
  const info = { type: "session_info", timestamp, name };
  await writeFile(join(sessions, `${id}.jsonl`), `${JSON.stringify(header)}\n${JSON.stringify(info)}\n`);
}

const app = spawn("bash", [
  "devtools/start-desktop.sh",
  "--remote-debugging-port=0",
  `--user-data-dir=${profile}`,
  "--session-dir", sessions,
], {
  cwd: projectRoot,
  detached: process.platform !== "win32",
  env: { ...process.env, ROZSA_CONFIG_DIR: config, ROZSA_PROJECT_CONFIG_DIR: projectConfig },
  stdio: ["ignore", "pipe", "pipe"],
});

let appLog = "";
app.stdout.on("data", chunk => { appLog += chunk.toString(); });
app.stderr.on("data", chunk => { appLog += chunk.toString(); });
let appExit;
const exited = new Promise(resolveExit => app.once("exit", (code, signal) => {
  appExit = { code, signal };
  resolveExit(appExit);
}));
const waitFor = async (read, label, timeoutMs = 30000) => {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (appExit) throw new Error(`Desktop app exited during ${label} (${appExit.code ?? appExit.signal}):\n${appLog}`);
    const value = read();
    if (value !== undefined) return value;
    await new Promise(resolveWait => setTimeout(resolveWait, 50));
  }
  throw new Error(`Timed out during ${label}:\n${appLog}`);
};

let page;
let stage = "DevTools startup";
try {
  const debugURL = await waitFor(() => /DevTools listening on (ws:\/\/\S+)/.exec(appLog)?.[1], "DevTools startup");
  stage = "attaching to the renderer";
  page = await attachPage(debugURL);
  await page.waitFor("window.RozsaUIReady && window.RozsaUIHost", "product UI ready");
  stage = "loading showcase sessions";
  const entries = await page.evaluate("window.RozsaUIHost.port.request('get_sessions').then(items=>items.map(({id,path})=>({id,path})))");
  const permissionSession = entries.find(entry => entry.id === "permission-demo");
  const questionSession = entries.find(entry => entry.id === "question-demo");
  if (!permissionSession || !questionSession) throw new Error(`Showcase sessions were not catalogued: ${JSON.stringify(entries)}`);

  stage = "creating the permission request";
  await page.evaluate(`window.RozsaUIHost.port.request('switch_session',{path:${JSON.stringify(permissionSession.path)}}).then(()=>true)`);
  await page.waitFor("window.RozsaUIHost.port.request('get_state').then(state=>state.sessionId==='permission-demo')", "permission session selection");
  await page.evaluate("void window.RozsaUIHost.port.request('send_message',{message:'SHOW_PERMISSION_DEMO'}); true");
  await page.waitFor("document.querySelector('#permPanel.visible')?.dataset.requestId", "permission request");
  const permissionState = await page.evaluate("window.rozsa.getInterruptionState('permission-demo')");
  const pendingApproval = permissionState.approvals.find(({ status }) => status === "pending");
  if (!pendingApproval) throw new Error("The permission session has no pending approval");
  const relativeTarget = relative(projectRoot, permissionTarget).split(sep).join("/");
  const parts = relativeTarget.split("/");
  const expectedTrustLabels = [relativeTarget];
  for (let index = parts.length - 1; index > 0; index--) expectedTrustLabels.push(`${parts.slice(0, index).join("/")}/`);
  const actualTrustLabels = pendingApproval.trustGroups[0]?.levels.map(({ label }) => label) || [];
  if (JSON.stringify(actualTrustLabels) !== JSON.stringify(expectedTrustLabels)) {
    throw new Error(`Permission trust paths differ from the launch project root: ${JSON.stringify(actualTrustLabels)}; expected ${JSON.stringify(expectedTrustLabels)}`);
  }

  stage = "creating the question in the second session";
  stage = "switching to the question session";
  await page.evaluate(`window.RozsaUIHost.port.request('switch_session',{path:${JSON.stringify(questionSession.path)}}).then(()=>true)`);
  stage = "confirming the question session";
  await page.waitFor("window.RozsaUIHost.port.request('get_state').then(state=>state.sessionId==='question-demo')", "question session selection");
  stage = "starting the question request";
  await page.evaluate("void window.RozsaUIHost.port.request('send_message',{message:'SHOW_QUESTION_DEMO'}); true");
  stage = "waiting for the question panel";
  await page.waitFor("document.querySelector('#questionPanel.visible')?.dataset.requestId", "AskUserQuestion request");

  console.log(`Permission Request and AskUserQuestion are pending in separate sessions.\nSwitch sessions in the sidebar:\n- Permission Request Demo (${permissionSession.id})\n- Ask User Question Demo (${questionSession.id})\nBoth states share one renderer interruption mount.\nRelative trust paths: ${actualTrustLabels.join(" | ")}\nTemporary profile and fixtures: ${runRoot}\nPress Ctrl+C to close this isolated app.`);
  await Promise.race([
    exited,
    new Promise(resolveSignal => {
      const finish = signal => resolveSignal({ signal });
      process.once("SIGINT", finish);
      process.once("SIGTERM", finish);
    }),
  ]);
} catch (error) {
  throw new Error(`Showcase failed while ${stage}: ${error instanceof Error ? error.message : String(error)}\n${appLog}`);
} finally {
  page?.close();
  if (!appExit) {
    if (process.platform === "win32") app.kill("SIGTERM");
    else if (app.pid) {
      try { process.kill(-app.pid, "SIGTERM"); } catch {}
    }
    await Promise.race([exited, new Promise(resolveTimeout => setTimeout(resolveTimeout, 5000))]);
    if (!appExit && process.platform === "win32") app.kill("SIGKILL");
    else if (!appExit && app.pid && process.platform !== "win32") {
      try { process.kill(-app.pid, "SIGKILL"); } catch {}
    }
  }
  await new Promise(resolveClose => provider.close(resolveClose));
}
