// Launch an isolated Electron + Go app with Write, Bash, and question requests in separate sessions.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { attachPage } from "../desktop/tests/performance/harness.js";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const cwd = join(projectRoot, "testdata/project-test-dirs/interruption-state-showcase");
const config = join(projectRoot, "testdata/rozsa-config-dev");
const profile = join(projectRoot, "tmp/electron-profiles/interruption-state-showcase");
const showcase = JSON.parse(
  await readFile(
    join(projectRoot, "testdata/fixtures/desktop/interruption-state-showcase.json"),
    "utf8",
  ),
);
const bashCommand = JSON.parse(
  showcase.requests.bash[0].choices[0].delta.tool_calls[0].function.arguments,
).command;
const providerPort = 50805;
const permissionTarget = join(cwd, "tmp/interruption-state-showcase/permission-demo.txt");

function sendCompletion(response, chunks) {
  response.writeHead(200, {
    "content-type": "text/event-stream",
    "cache-control": "no-cache",
    connection: "keep-alive",
  });
  for (const chunk of chunks) response.write(`data: ${JSON.stringify(chunk)}\n\n`);
  response.write("data: [DONE]\n\n");
  response.end();
}

const shownRequests = new Set();
const provider = createServer((request, response) => {
  let body = "";
  request.setEncoding("utf8");
  request.on("data", (chunk) => {
    body += chunk;
  });
  request.on("end", () => {
    try {
      const payload = JSON.parse(body);
      const latestUser = [...(payload.messages || [])]
        .reverse()
        .find((message) => message.role === "user");
      const prompt =
        typeof latestUser?.content === "string"
          ? latestUser.content
          : Array.isArray(latestUser?.content)
            ? latestUser.content
                .map((part) => (typeof part?.text === "string" ? part.text : ""))
                .join("\n")
            : "";
      const scenario = [
        ["SHOW_PERMISSION_DEMO", "permission"],
        ["SHOW_BASH_PERMISSION_DEMO", "bash"],
        ["SHOW_QUESTION_DEMO", "question"],
      ].find(([marker]) => prompt.includes(marker))?.[1];
      if (!scenario) {
        response.writeHead(400, { "content-type": "text/plain" });
        response.end("The showcase provider received an unknown prompt.");
        return;
      }
      if (shownRequests.has(scenario)) {
        sendCompletion(response, showcase.followup);
        return;
      }
      shownRequests.add(scenario);
      sendCompletion(response, showcase.requests[scenario]);
    } catch (error) {
      response.writeHead(400, { "content-type": "text/plain" });
      response.end(error instanceof Error ? error.message : String(error));
    }
  });
});

await new Promise((resolveListen, rejectListen) => {
  provider.once("error", rejectListen);
  provider.listen(providerPort, "127.0.0.1", resolveListen);
});

const appEnvironment = {
  ...process.env,
  ROZSA_CONFIG_DIR: config,
  ROZSA_PROJECT_ROOT: cwd,
};
delete appEnvironment.ROZSA_PROJECT_CONFIG_DIR;

const app = spawn(
  "bash",
  ["devtools/start-desktop.sh", "--remote-debugging-port=0", `--user-data-dir=${profile}`],
  {
    cwd: projectRoot,
    detached: process.platform !== "win32",
    env: appEnvironment,
    stdio: ["ignore", "pipe", "pipe"],
  },
);

let appLog = "";
app.stdout.on("data", (chunk) => {
  appLog += chunk.toString();
});
app.stderr.on("data", (chunk) => {
  appLog += chunk.toString();
});
let appExit;
const exited = new Promise((resolveExit) =>
  app.once("exit", (code, signal) => {
    appExit = { code, signal };
    resolveExit(appExit);
  }),
);
const waitFor = async (read, label, timeoutMs = 30000) => {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (appExit)
      throw new Error(
        `Desktop app exited during ${label} (${appExit.code ?? appExit.signal}):\n${appLog}`,
      );
    const value = read();
    if (value !== undefined) return value;
    await new Promise((resolveWait) => setTimeout(resolveWait, 50));
  }
  throw new Error(`Timed out during ${label}:\n${appLog}`);
};

let page;
let stage = "DevTools startup";
try {
  const debugURL = await waitFor(
    () => /DevTools listening on (ws:\/\/\S+)/.exec(appLog)?.[1],
    "DevTools startup",
  );
  stage = "attaching to the renderer";
  page = await attachPage(debugURL);
  await page.waitFor("window.RozsaUIReady && window.RozsaUIHost", "product UI ready");
  stage = "loading showcase sessions";
  const entries = await page.evaluate(
    "window.RozsaUIHost.port.request('get_sessions').then(items=>items.map(({id,path})=>({id,path})))",
  );
  const permissionSession = entries.find((entry) => entry.id === "permission-demo");
  const bashSession = entries.find((entry) => entry.id === "bash-permission-demo");
  const questionSession = entries.find((entry) => entry.id === "question-demo");
  if (!permissionSession || !bashSession || !questionSession)
    throw new Error(`Showcase sessions were not catalogued: ${JSON.stringify(entries)}`);

  stage = "creating the permission request";
  await page.evaluate(
    `window.RozsaUIHost.port.request('switch_session',{path:${JSON.stringify(permissionSession.path)}}).then(()=>true)`,
  );
  await page.waitFor(
    "window.RozsaUIHost.port.request('get_state').then(state=>state.sessionId==='permission-demo')",
    "permission session selection",
  );
  await page.evaluate(
    "void window.RozsaUIHost.port.request('send_message',{message:'SHOW_PERMISSION_DEMO'}); true",
  );
  await page.waitFor(
    "document.querySelector('#permPanel.visible')?.dataset.requestId",
    "permission request",
  );
  const permissionState = await page.evaluate(
    "window.rozsa.getInterruptionState('permission-demo')",
  );
  const pendingApproval = permissionState.approvals.find(({ status }) => status === "pending");
  if (!pendingApproval) throw new Error("The permission session has no pending approval");
  const relativeTarget = relative(cwd, permissionTarget).split(sep).join("/");
  const parts = relativeTarget.split("/");
  const expectedTrustLabels = [relativeTarget];
  for (let index = parts.length - 1; index > 0; index--)
    expectedTrustLabels.push(`${parts.slice(0, index).join("/")}/`);
  const actualTrustLabels = pendingApproval.trustGroups[0]?.levels.map(({ label }) => label) || [];
  if (JSON.stringify(actualTrustLabels) !== JSON.stringify(expectedTrustLabels)) {
    throw new Error(
      `Permission trust paths differ from the launch project root: ${JSON.stringify(actualTrustLabels)}; expected ${JSON.stringify(expectedTrustLabels)}`,
    );
  }

  stage = "switching to the Bash permission session";
  await page.evaluate(
    `window.RozsaUIHost.port.request('switch_session',{path:${JSON.stringify(bashSession.path)}}).then(()=>true)`,
  );
  await page.waitFor(
    "window.RozsaUIHost.port.request('get_state').then(state=>state.sessionId==='bash-permission-demo')",
    "Bash permission session selection",
  );
  await page.evaluate(
    "void window.RozsaUIHost.port.request('send_message',{message:'SHOW_BASH_PERMISSION_DEMO'}); true",
  );
  await page.waitFor(
    "document.querySelector('#permPanel.visible') && document.querySelector('#permTool')?.textContent==='bash'",
    "Bash permission request",
  );
  await page.waitFor(
    `(() => { const code = document.querySelector('#permPanel .perm-panel-cmd .fold-full code.md-highlighted-code.language-bash'); return code?.textContent === ${JSON.stringify(bashCommand)} && code.previousElementSibling?.textContent === "$ "; })()`,
    "highlighted Bash source with a separate prompt",
  );

  stage = "switching to the question session";
  await page.evaluate(
    `window.RozsaUIHost.port.request('switch_session',{path:${JSON.stringify(questionSession.path)}}).then(()=>true)`,
  );
  stage = "confirming the question session";
  await page.waitFor(
    "window.RozsaUIHost.port.request('get_state').then(state=>state.sessionId==='question-demo')",
    "question session selection",
  );
  stage = "starting the question request";
  await page.evaluate(
    "void window.RozsaUIHost.port.request('send_message',{message:'SHOW_QUESTION_DEMO'}); true",
  );
  stage = "waiting for the question panel";
  await page.waitFor(
    "document.querySelector('#questionPanel.visible')?.dataset.requestId",
    "AskUserQuestion request",
  );

  console.log(
    `Write, Bash, and AskUserQuestion requests are pending in separate sessions.\nSwitch sessions in the sidebar:\n- Permission Request Demo (${permissionSession.id})\n- Bash Permission Request Demo (${bashSession.id})\n- Ask User Question Demo (${questionSession.id})\nAll states share one renderer interruption mount.\nRelative trust paths: ${actualTrustLabels.join(" | ")}\nTest cwd and profile: ${cwd}\nPress Ctrl+C to close this isolated app.`,
  );
  await Promise.race([
    exited,
    new Promise((resolveSignal) => {
      const finish = (signal) => resolveSignal({ signal });
      process.once("SIGINT", finish);
      process.once("SIGTERM", finish);
    }),
  ]);
} catch (error) {
  throw new Error(
    `Showcase failed while ${stage}: ${error instanceof Error ? error.message : String(error)}\n${appLog}`,
  );
} finally {
  page?.close();
  if (!appExit) {
    if (process.platform === "win32") app.kill("SIGTERM");
    else if (app.pid) {
      try {
        process.kill(-app.pid, "SIGTERM");
      } catch {}
    }
    await Promise.race([exited, new Promise((resolveTimeout) => setTimeout(resolveTimeout, 5000))]);
    if (!appExit && process.platform === "win32") app.kill("SIGKILL");
    else if (!appExit && app.pid && process.platform !== "win32") {
      try {
        process.kill(-app.pid, "SIGKILL");
      } catch {}
    }
  }
  await new Promise((resolveClose) => provider.close(resolveClose));
}
