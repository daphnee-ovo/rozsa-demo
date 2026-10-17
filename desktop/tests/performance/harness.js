import { spawn } from "node:child_process";
import { execFile } from "node:child_process";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { promisify } from "node:util";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const execFileAsync = promisify(execFile);
export const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
export const outputRoot = join(projectRoot, "tmp/migration-message-performance");

export async function createEnvironment(label, { baseURL = "http://127.0.0.1:9/v1", sessionsDirectory } = {}) {
  await mkdir(outputRoot, { recursive: true });
  const base = await mkdtemp(join(outputRoot, `${label}-`));
  const config = join(base, "config");
  const projectConfig = join(base, "project-config");
  const sessions = sessionsDirectory || join(base, "sessions");
  const profile = join(base, "profile");
  for (const directory of [join(config, "models"), projectConfig, sessions, profile]) {
    await mkdir(directory, { recursive: true });
  }
  await writeFile(join(config, "settings.json"), JSON.stringify({ defaultProvider: "synthetic", defaultModel: "stream" }));
  await writeFile(join(config, "models", "synthetic.json"), JSON.stringify({ providers: { synthetic: {
    api: "openai-completions", apiKey: "fixture-key", baseUrl: baseURL,
    models: [{ id: "stream", contextWindow: 16384, maxTokens: 512 }],
  } } }));
  await writeFile(join(projectConfig, "settings.json"), "{}\n");
  return { base, config, projectConfig, sessions, profile, cwd: base };
}

export async function startElectron(environment, { performanceTrace = true } = {}) {
  const electronBinary = join(projectRoot, "desktop/node_modules/electron/dist", (await readFile(join(projectRoot, "desktop/node_modules/electron/path.txt"), "utf8")).trim());
  const executable = process.env.ROZSA_NATIVE_EXECUTABLE || electronBinary;
  const args = [
    ...(process.env.ROZSA_NATIVE_EXECUTABLE ? [] : [join(projectRoot, "desktop")]),
    "--remote-debugging-port=0",
    `--user-data-dir=${environment.profile}`,
    "--session-dir", environment.sessions,
  ];
  const spawnEpochMs = Date.now();
  const child = spawn(executable, args, {
    cwd: environment.cwd,
    env: {
      ...process.env,
      ROZSA_CONFIG_DIR: environment.config,
      ROZSA_PROJECT_CONFIG_DIR: environment.projectConfig,
      ROZSA_PERF_TRACE: performanceTrace ? "1" : "0",
      TMPDIR: join(projectRoot, "tmp/migration/install-tmp"),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let logs = "";
  child.stdout.on("data", chunk => { logs += chunk.toString(); });
  child.stderr.on("data", chunk => { logs += chunk.toString(); });
  const exited = new Promise((resolveExit, rejectExit) => {
    child.once("error", rejectExit);
    child.once("exit", (code, signal) => resolveExit({ code, signal }));
  });
  const closed = new Promise(resolveClose => child.once("close", (code, signal) => resolveClose({ code, signal })));

  async function waitFor(read, label, timeoutMs = 20000) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const value = read();
      if (value !== undefined) return value;
      if (child.exitCode !== null || child.signalCode !== null) {
        await closed;
        throw new Error(`Electron exited during ${label} (${child.exitCode ?? child.signalCode}):\n${logs}`);
      }
      await new Promise(resolveWait => setTimeout(resolveWait, 25));
    }
    throw new Error(`Timed out during ${label}:\n${logs}`);
  }

  return {
    child,
    get logs() { return logs; },
    spawnEpochMs,
    waitFor,
    async devToolsURL() {
      return waitFor(() => /DevTools listening on (ws:\/\/\S+)/.exec(logs)?.[1], "DevTools startup");
    },
    async startupMarks() {
      const entries = [...logs.matchAll(/ROZSA_PERF_STARTUP (\{[^\n]+\})/g)];
      return Object.fromEntries(entries.map(([, json]) => {
        const item = JSON.parse(json);
        return [item.stage, item.epochMs - spawnEpochMs];
      }));
    },
    async backendStartupMarks() {
      return Object.fromEntries([...logs.matchAll(/ROZSA_PERF_BACKEND ([^\s]+) (\d+)/g)]
        .map(([, stage, epoch]) => [stage, Number(epoch) - spawnEpochMs]));
    },
    async stop() {
      if (child.exitCode === null && child.signalCode === null) {
        child.kill("SIGTERM");
        await Promise.race([exited, new Promise(resolveTimeout => setTimeout(resolveTimeout, 3000))]);
      }
      if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
      return closed;
    },
    exited,
  };
}

export async function attachPage(debugURL) {
  const socket = new WebSocket(debugURL);
  await new Promise((resolveOpen, rejectOpen) => {
    socket.addEventListener("open", resolveOpen, { once: true });
    socket.addEventListener("error", rejectOpen, { once: true });
  });
  let requestId = 0;
  const pending = new Map();
  const failures = [];
  socket.addEventListener("message", event => {
    const message = JSON.parse(String(event.data));
    if (message.method === "Runtime.exceptionThrown" || message.method === "Log.entryAdded") failures.push(message.params);
    const request = pending.get(message.id);
    if (!request) return;
    pending.delete(message.id);
    clearTimeout(request.timer);
    if (message.error) request.reject(new Error(JSON.stringify(message.error)));
    else request.resolve(message.result);
  });
  const send = (method, params = {}, sessionId, timeoutMs = 15000) => new Promise((resolveRequest, rejectRequest) => {
    const id = ++requestId;
    const timer = setTimeout(() => {
      pending.delete(id);
      rejectRequest(new Error(`DevTools timeout: ${method}`));
    }, timeoutMs);
    pending.set(id, { resolve: resolveRequest, reject: rejectRequest, timer });
    socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
  const targets = await send("Target.getTargets");
  const target = targets.targetInfos.find(item => item.type === "page" &&
    (item.url.endsWith("/index.html?rozsa_perf=1") || item.url.endsWith("/index.html") || /^http:\/\/127\.0\.0\.1:\d+\/$/.test(item.url)));
  if (!target) throw new Error(`Electron renderer target not found: ${JSON.stringify(targets.targetInfos)}`);
  const attached = await send("Target.attachToTarget", { targetId: target.targetId, flatten: true });
  await send("Runtime.enable", {}, attached.sessionId);
  await send("Log.enable", {}, attached.sessionId);
  await send("Performance.enable", {}, attached.sessionId);
  const evaluate = async (expression, timeoutMs = 15000) => {
    const result = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, attached.sessionId, timeoutMs);
    if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  return {
    socket,
    failures,
    send: (method, params) => send(method, params, attached.sessionId),
    evaluate,
    async performanceMetrics() { return (await send("Performance.getMetrics", {}, attached.sessionId)).metrics; },
    async waitFor(expression, label, timeoutMs = 20000) {
      const deadline = Date.now() + timeoutMs;
      while (Date.now() < deadline) {
        const result = await evaluate(expression);
        if (result) return result;
        await new Promise(resolveWait => setTimeout(resolveWait, 25));
      }
      throw new Error(`Timed out during ${label}`);
    },
    close() { socket.close(); },
  };
}

export async function sampleProcessMemory(rootPid) {
  const { stdout } = await execFileAsync("ps", ["-axo", "pid=,ppid=,rss=,command="]);
  const processes = stdout.split("\n").map(line => {
    const match = /^\s*(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/.exec(line);
    return match ? { pid: Number(match[1]), ppid: Number(match[2]), rssKB: Number(match[3]), command: match[4] } : null;
  }).filter(Boolean);
  const root = processes.find(item => item.pid === rootPid);
  if (!root) throw new Error(`Electron process ${rootPid} disappeared during memory sample`);
  const descendants = new Map([[rootPid, [root]]]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const item of processes) {
      if (descendants.has(item.pid)) continue;
      const parent = descendants.get(item.ppid);
      if (!parent) continue;
      descendants.set(item.pid, [item]);
      changed = true;
    }
  }
  const tree = [...descendants.values()].flat();
  const renderer = tree.filter(item => item.command.includes("--type=renderer"));
  return {
    mainRSSMB: root.rssKB / 1024,
    rendererRSSMB: renderer.reduce((sum, item) => sum + item.rssKB, 0) / 1024,
    totalRSSMB: tree.reduce((sum, item) => sum + item.rssKB, 0) / 1024,
    processCount: tree.length,
  };
}

