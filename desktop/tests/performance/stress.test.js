import assert from "node:assert/strict";
import { createServer } from "node:http";
import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { test } from "node:test";
import { attachPage, createEnvironment, outputRoot, sampleProcessMemory, startElectron } from "./harness.js";
import { writeConversationFixture } from "./fixtures.js";

const rounds = Math.max(1, Number(process.env.ROZSA_PERF_ROUNDS || 1));
const calibration = process.env.ROZSA_PERF_CALIBRATION === "1";
const pairsPerSession = Math.max(1, Number(process.env.ROZSA_PERF_STRESS_PAIRS || 3000));
const expectedRows = pairsPerSession * 2;
const streamTokens = [
  ...Array.from({ length: 32 }, (_, index) => `stress-stream-${String(index).padStart(2, "0")} `),
  "\n```mermaid\nflowchart LR\n  stream_open[Open] --> stream_closed[Close]\n",
  "```",
];

function percentile(values, fraction) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.max(0, Math.ceil(sorted.length * fraction) - 1)];
}

function summarize(values) {
  const sorted = [...values].sort((left, right) => left - right);
  const trimmed = sorted.length >= 5 ? sorted.slice(1, -1) : null;
  return {
    samples: values.length,
    min: Math.min(...values),
    p50: percentile(values, 0.5),
    p95: percentile(values, 0.95),
    max: Math.max(...values),
    mean: values.reduce((sum, value) => sum + value, 0) / values.length,
    trimmedMeanDropOneEach: trimmed
      ? trimmed.reduce((sum, value) => sum + value, 0) / trimmed.length
      : null,
  };
}

async function startProvider() {
  let requests = 0;
  const pendingFences = new Set();
  const server = createServer((request, response) => {
    requests++;
    request.resume();
    request.on("end", () => {
      response.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache", connection: "keep-alive" });
      let index = 0, timer, fenceReleased = false;
      const releaseFence = () => { fenceReleased = true; pendingFences.delete(releaseFence); sendNext(); };
      const sendNext = () => {
        if (index === streamTokens.length - 1 && !fenceReleased) { pendingFences.add(releaseFence); return; }
        if (index < streamTokens.length) {
          response.write(`data: ${JSON.stringify({ choices: [{ delta: { content: streamTokens[index] } }] })}\n\n`);
          index++;
          timer = setTimeout(sendNext, 15);
        } else {
          response.write(`data: ${JSON.stringify({ choices: [{ delta: {}, finish_reason: "stop" }], usage: { prompt_tokens: 30, completion_tokens: 32 } })}\n\n`);
          response.write("data: [DONE]\n\n");
          response.end();
        }
      };
      timer = setTimeout(sendNext, 15);
      response.on("close", () => { clearTimeout(timer); pendingFences.delete(releaseFence); });
    });
  });
  await new Promise((resolve, reject) => server.listen(0, "127.0.0.1", resolve).once("error", reject));
  const address = server.address();
  assert.ok(address && typeof address === "object");
  return { server, releaseDiagramFence: () => { for (const release of pendingFences) release(); }, get requests() { return requests; }, baseURL: `http://127.0.0.1:${address.port}/v1` };
}

async function measureToggle(cdp, controlSelector, targetSelector, className) {
  return cdp.evaluate(`(async () => {
    const target = [...document.querySelectorAll(${JSON.stringify(targetSelector)})].at(-1);
    const control = target?.querySelector(${JSON.stringify(controlSelector)});
    if (!control || !target) throw new Error("Missing performance toggle: ${controlSelector}");
    const containerBounds = document.querySelector("#chatMessages").getBoundingClientRect();
    const targetBounds = target.getBoundingClientRect();
    if (targetBounds.top < containerBounds.top || targetBounds.bottom > containerBounds.bottom) {
      target.scrollIntoView({ block: "center" });
    }
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const wasExpanded = target.classList.contains(${JSON.stringify(className)});
    const started = performance.now();
    control.click();
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    if (target.classList.contains(${JSON.stringify(className)}) === wasExpanded) {
      throw new Error("Toggle did not change state: ${controlSelector}; " + JSON.stringify({
        before: wasExpanded,
        after: target.classList.contains(${JSON.stringify(className)}),
        ariaExpanded: control.getAttribute("aria-expanded"),
        globalToggle: typeof window.toggleThinking,
      }));
    }
    return performance.now() - started;
  })()`, 90000);
}

async function measureSessionSwitch(cdp, sessionIndex, sessionId, expectedRows) {
  return cdp.evaluate(`(async () => {
    const item = document.querySelector('.session-item[data-session-index="${sessionIndex}"]');
    if (!item) throw new Error("Session switch control missing for index ${sessionIndex}");
    const started = performance.now();
    item.click();
    const deadline = performance.now() + 60000;
    while (document.querySelector("#chatMessages").dataset.sessionId !== ${JSON.stringify(sessionId)}
      || Number(document.querySelector("#chatMessages [data-virtualized=true]")?.dataset.historyEntryCount) !== ${expectedRows}) {
      if (performance.now() > deadline) throw new Error("Session history did not reach expected row count ${expectedRows}");
      await new Promise(resolve => requestAnimationFrame(resolve));
    }
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const trace = window.__ROZSA_SESSION_SWITCH_TRACE__;
    if (!trace || trace.sessionId !== ${JSON.stringify(sessionId)} || !trace.preparationCompletedAt || !trace.rpcCompletedAt || !trace.renderCommittedAt) {
      throw new Error("Session switch did not record setup RPC and React commit phases");
    }
    return {
      clickToRenderMs: performance.now() - started,
      preparationMs: trace.preparationCompletedAt - trace.startedAt,
      rpcResponseMs: trace.rpcCompletedAt - trace.preparationCompletedAt,
      renderAfterRpcMs: trace.renderCommittedAt - trace.rpcCompletedAt,
    };
  })()`, 90000);
}

async function measureRound(index, longFixture, provider) {
  const environment = await createEnvironment(`stress-${index}`, { baseURL: provider.baseURL });
  const short = await writeConversationFixture(environment.sessions, `short-${index}`, { pairs: 5, imageEvery: 0, toolEvery: 0 });
  const longPath = join(environment.sessions, "long-session.jsonl");
  await copyFile(longFixture.path, longPath);
  const app = await startElectron(environment);
  let cdp;
  let keepSampling = true;
  let phase = "startup";
  const memory = [];
  const memorySampler = (async () => {
    while (keepSampling) {
      try { memory.push({ elapsedMs: Date.now() - app.spawnEpochMs, ...(await sampleProcessMemory(app.child.pid)) }); }
      catch { /* process termination can race the final memory sample */ }
      await new Promise(resolve => setTimeout(resolve, 250));
    }
  })();
  try {
    phase = "window shown";
    await app.waitFor(() => /"stage":"window-shown"/.test(app.logs) ? true : undefined, "stress window shown");
    cdp = await attachPage(await app.devToolsURL());
    phase = "initial history render";
    await cdp.waitFor("performance.getEntriesByName('rozsa:ui-interactive-ready').length > 0", "stress renderer UI readiness");
    await cdp.waitFor(`Number(document.querySelector('#chatMessages [data-virtualized=true]')?.dataset.historyEntryCount) === ${expectedRows}`, `complete complex ${expectedRows}-message history available`, 90000);
    await cdp.waitFor("document.querySelectorAll('#chatMessages .msg').length > 0", "latest complex-session viewport rendered", 90000);
    await cdp.waitFor("document.querySelectorAll('#chatMessages .md-mermaid[data-pele-ready=true]').length > 0", "visible complete Mermaid diagrams mounted with Pele", 90000);
    await cdp.waitFor("Array.from(document.querySelectorAll('#chatMessages img')).every(image => image.complete && image.naturalWidth > 0)", "stress image decode", 90000);
    const initial = await cdp.evaluate(`({
      rows: document.querySelectorAll("#chatMessages .msg").length,
      historyEntries: Number(document.querySelector("#chatMessages [data-virtualized=true]")?.dataset.historyEntryCount || 0),
      images: document.querySelectorAll("#chatMessages img").length,
      tools: document.querySelectorAll("#chatMessages .tool-call").length,
      thinking: document.querySelectorAll("#chatMessages .thinking-block").length,
      expandedTools: document.querySelectorAll("#chatMessages .tool-call.expanded").length,
      expandedThinking: document.querySelectorAll("#chatMessages .thinking-block.expanded").length,
      mermaidCharts: document.querySelectorAll("#chatMessages .md-mermaid[data-pele-ready=true]").length,
      renderReadyEpochMs: Date.now(),
    })`);
    const loadedHistoryMs = initial.renderReadyEpochMs - app.spawnEpochMs;
    assert.equal(initial.historyEntries, expectedRows, "complete long history must remain available while virtualized");
    assert.ok(initial.rows > 0 && initial.rows < expectedRows, `expected bounded viewport DOM rows, saw ${initial.rows}/${expectedRows}`);
    assert.ok(initial.tools > 0, `expected visible historical tool cards, saw ${initial.tools}`);
    assert.ok(initial.thinking > 0, `expected visible thinking blocks, saw ${initial.thinking}`);
    assert.ok(initial.mermaidCharts > 0, "complex Markdown fixture must mount visible Mermaid diagrams");
    assert.equal(initial.expandedTools, 0);
    assert.equal(initial.expandedThinking, 0);

    phase = "virtual history reachability";
    const historyReachabilityMs = await cdp.evaluate(`(async () => {
      const chat = document.querySelector("#chatMessages");
      const started = performance.now();
      chat.scrollTop = 0;
      const deadline = performance.now() + 15000;
      while (!document.querySelector('.conversation-virtual-row [data-od-id="message-0"]')) {
        if (performance.now() > deadline) throw new Error("oldest history row did not become available after scrolling to the start");
        await new Promise(resolve => requestAnimationFrame(resolve));
      }
      await new Promise(resolve => requestAnimationFrame(resolve));
      const firstImage = document.querySelector('.conversation-virtual-row img');
      if (!firstImage) throw new Error("oldest history viewport did not mount its local image");
      while (!firstImage.complete || firstImage.naturalWidth === 0) {
        if (performance.now() > deadline) throw new Error("scrolled-to history image did not decode");
        await new Promise(resolve => requestAnimationFrame(resolve));
      }
      chat.scrollTop = chat.scrollHeight;
      while (!document.querySelector('.conversation-virtual-row [data-od-id="message-${expectedRows - 1}"]')) {
        if (performance.now() > deadline) throw new Error("latest history row did not return after scrolling to the end");
        await new Promise(resolve => requestAnimationFrame(resolve));
      }
      return performance.now() - started;
    })()`, 30000);

    phase = "responsive Mermaid measurement";
    const beforeResize = await cdp.evaluate(`({
      width: document.querySelector("#chatMessages .md-mermaid")?.clientWidth || 0,
      viewportWidth: window.innerWidth,
      adaptations: window.__ROZSA_PERF__?.markdownStages?.peleAdapt?.length || 0,
    })`);
    assert.ok(beforeResize.width > 0, "visible Mermaid container width is unavailable");
    const narrowContainerWidth = Math.max(240, beforeResize.width - 280);
    await cdp.evaluate(`(() => {
      const diagram = document.querySelector("#chatMessages .md-mermaid");
      diagram.style.width = "${narrowContainerWidth}px";
    })()`);
    await cdp.evaluate("new Promise(resolve => setTimeout(resolve, 500))", 3000);
    const afterResize = await cdp.evaluate(`({
      width: document.querySelector("#chatMessages .md-mermaid")?.clientWidth || 0,
      viewportWidth: window.innerWidth,
      mainPanelRight: document.querySelector('[data-od-id="main-panel"]').getBoundingClientRect().right,
      documentWidth: document.documentElement.clientWidth,
      adaptations: window.__ROZSA_PERF__?.markdownStages?.peleAdapt?.length || 0,
      viewBox: document.querySelector("#chatMessages .md-mermaid svg")?.getAttribute("viewBox") || "",
    })`);
    assert.ok(afterResize.width > 0 && afterResize.width < beforeResize.width,
      `Mermaid container did not narrow: ${JSON.stringify({ beforeResize, afterResize })}`);
    assert.ok(afterResize.adaptations > beforeResize.adaptations,
      `Pele adaptation was not timed after width change: ${JSON.stringify({ beforeResize, afterResize })}`);
    const narrowDiagramWidth = afterResize.width;
    await cdp.evaluate(`(() => {
      document.querySelector("#chatMessages .md-mermaid").style.removeProperty("width");
    })()`);
    await cdp.waitFor(`window.__ROZSA_PERF__?.markdownStages?.peleAdapt?.length > ${afterResize.adaptations}`,
      "Pele re-adapts when the full conversation container width is restored", 10000);
    const restoredLayout = await cdp.evaluate(`({
      viewportWidth: window.innerWidth,
      documentWidth: document.documentElement.clientWidth,
      mainPanelRight: document.querySelector('[data-od-id="main-panel"]').getBoundingClientRect().right,
      diagramWidth: document.querySelector("#chatMessages .md-mermaid")?.clientWidth || 0,
    })`);
    assert.ok(Math.abs(restoredLayout.mainPanelRight - restoredLayout.documentWidth) <= 1,
      `main panel did not fill the Electron viewport after container reflow: ${JSON.stringify(restoredLayout)}`);
    assert.ok(restoredLayout.diagramWidth > narrowContainerWidth,
      `Mermaid container did not return to full width: ${JSON.stringify({ narrowContainerWidth, restoredLayout })}`);
    if (process.platform === "darwin") {
      const collapsed = await cdp.evaluate(`(() => {
        const toggle = document.querySelector("body > .sidebar-toggle-button");
        if (!toggle) throw new Error("macOS sidebar toggle is unavailable");
        if (!document.body.classList.contains("sidebar-collapsed")) toggle.click();
        const panel = document.querySelector('[data-od-id="main-panel"]').getBoundingClientRect();
        return {
          collapsed: document.body.classList.contains("sidebar-collapsed"),
          viewportWidth: window.innerWidth,
          outerWidth: window.outerWidth,
          panelRight: panel.right,
          documentWidth: document.documentElement.clientWidth,
        };
      })()`);
      assert.equal(collapsed.collapsed, true, `sidebar did not collapse: ${JSON.stringify(collapsed)}`);
      assert.ok(Math.abs(collapsed.viewportWidth - collapsed.outerWidth) <= 1,
        `renderer viewport did not match the actual Electron window: ${JSON.stringify(collapsed)}`);
      assert.ok(Math.abs(collapsed.panelRight - collapsed.documentWidth) <= 1,
        `main panel left a blank strip after hiding the sidebar: ${JSON.stringify(collapsed)}`);
      await cdp.evaluate("new Promise(resolve => setTimeout(resolve, 220))", 3000);
    }
    const screenshot = await cdp.send("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
    const screenshotPath = join(outputRoot, `stress-${index}-markdown.png`);
    await writeFile(screenshotPath, Buffer.from(screenshot.data, "base64"));

    phase = "session switching";
    phase = "session catalog";
    const sessions = await cdp.evaluate("window.RozsaUIHost.port.request('get_sessions')");
    const shortIndex = sessions.findIndex(item => item.id === short.id);
    const shortSession = sessions[shortIndex];
    assert.ok(shortSession && shortIndex >= 0, "short history session missing from isolated catalog");
    const longIndex = sessions.findIndex(item => item.path === longPath);
    const longSession = sessions[longIndex];
    assert.ok(longSession && longIndex >= 0, "long history session missing from isolated catalog");
    phase = "switch to short history";
    const switchOut = await measureSessionSwitch(cdp, shortIndex, shortSession.id, 10);
    await cdp.waitFor("window.__ROZSA_PERF__?.activePeleMounts === 0", "Pele observers released after virtual rows are removed", 10000);
    phase = "switch back to long history";
    const switchBack = await measureSessionSwitch(cdp, longIndex, longSession.id, expectedRows);
    await cdp.waitFor("window.__ROZSA_PERF__?.activePeleMounts > 0", "visible Mermaid diagrams remount after session restoration", 30000);

    phase = "tool and thinking expansion";
    const toolToggleMs = [];
    const thinkingToggleMs = [];
    for (let iteration = 0; iteration < 10; iteration++) {
      toolToggleMs.push(await measureToggle(cdp, ".tool-header", ".tool-call", "expanded"));
      thinkingToggleMs.push(await measureToggle(cdp, ".thinking-header", ".thinking-block", "expanded"));
    }

    phase = "scroll responsiveness";
    const scrollMs = await cdp.evaluate(`(async () => {
      const chat = document.querySelector("#chatMessages");
      const started = performance.now();
      for (let index = 0; index < 10; index++) {
        chat.scrollTop = (chat.scrollHeight - chat.clientHeight) * ((index % 2) ? 0.35 : 0.7);
        await new Promise(resolve => requestAnimationFrame(resolve));
      }
      await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      return performance.now() - started;
    })()`, 90000);

    phase = "typing responsiveness";
    await cdp.evaluate("document.querySelector('#msgInput').focus()");
    const typingStarted = Date.now();
    for (const character of "Keep typing responsive while rendering a long conversation. ") {
      await cdp.send("Input.insertText", { text: character });
    }
    const typingMs = Date.now() - typingStarted;
    const inputText = await cdp.evaluate("document.querySelector('#msgInput').textContent");
    assert.equal(inputText, "Keep typing responsive while rendering a long conversation. ");

    phase = "streaming response";
    const sendStarted = Date.now();
    await cdp.evaluate("window.RozsaUIHost.port.request('send_message',{message:'Stream during complex history interaction'})");
    await cdp.waitFor(`window.RozsaUIHost.port.request('get_state').then(state => state.messages.at(-1)?.message?.content?.map(block => block.text || '').join(' ') || '').then(text => text.includes(${JSON.stringify(streamTokens[0])}))`, "first stream delta", 15000);
    const firstDeltaMs = Date.now() - sendStarted;
    await cdp.waitFor(`window.RozsaUIHost.port.request('get_state').then(state => state.messages.at(-1)?.message?.content?.map(block => block.text || '').join(' ') || '').then(text => text.includes("flowchart LR"))`, "streaming Mermaid source received before its closing fence", 15000);
    await cdp.evaluate("(() => { const chat = document.querySelector('#chatMessages'); chat.scrollTop = chat.scrollHeight; })()");
    await cdp.waitFor(`(() => [...document.querySelectorAll("#chatMessages .conversation-virtual-row .msg")].some(item => item.textContent.includes("stress-stream-31") && item.textContent.includes("flowchart LR")))()`,
      "streaming message row mounted at the conversation end", 15000);
    const openDiagram = await cdp.evaluate(`(() => {
      const row = [...document.querySelectorAll("#chatMessages .conversation-virtual-row .msg")].find(item => item.textContent.includes("stress-stream-31"));
      return { sourceVisible: row?.textContent.includes("stream_closed"), rendered: Boolean(row?.querySelector(".md-mermaid-wrap")) };
    })()`);
    assert.equal(openDiagram.sourceVisible, true, `streaming Mermaid source must remain readable before closure: ${JSON.stringify(openDiagram)}`);
    assert.equal(openDiagram.rendered, false, "incomplete stream must not mount a diagram from Streamdown's temporary completion");
    provider.releaseDiagramFence();
    await cdp.waitFor(`window.RozsaUIHost.port.request('get_state').then(state => state.messages.at(-1)?.message?.content?.map(block => block.text || '').join(' ') || '').then(text => text.includes(${JSON.stringify(streamTokens.at(-1).trim())}))`, "stream completion", 30000);
    await cdp.waitFor(`(() => { const row = [...document.querySelectorAll("#chatMessages .conversation-virtual-row .msg")].find(item => item.textContent.includes("stress-stream-31")); return Boolean(row?.querySelector(".md-mermaid[data-pele-ready=true]")); })()`, "Pele mounts the stream diagram after the source fence closes", 30000);
    const streamRows = await cdp.evaluate("Number(document.querySelector('#chatMessages [data-virtualized=true]')?.dataset.historyEntryCount)");
    assert.equal(streamRows, expectedRows + 2);

    phase = "browser and process metrics";
    const browserMetrics = await cdp.performanceMetrics();
    const heap = browserMetrics.find(metric => metric.name === "JSHeapUsedSize")?.value;
    const nodeCount = browserMetrics.find(metric => metric.name === "Nodes")?.value;
    const browser = await cdp.evaluate(`({
      longTaskSupported: window.__ROZSA_PERF__?.longTaskSupported,
      longTasks: window.__ROZSA_PERF__?.longTasks || [],
      frameGaps: window.__ROZSA_PERF__?.frameGaps || [],
      markdownStages: window.__ROZSA_PERF__?.markdownStages || {},
      scrollTop: document.querySelector("#chatMessages").scrollTop,
    })`);
    assert.equal(browser.longTaskSupported, true);
    const longTaskDurations = browser.longTasks.map(task => task.duration);
    const frameGapsOver50Ms = browser.frameGaps.filter(gap => gap >= 50).length;
    assert.ok(browser.markdownStages.renderCommit?.length > 0, `Markdown render-to-commit durations were not measured: ${JSON.stringify(browser.markdownStages)}`);
    assert.ok(browser.markdownStages.peleMount?.length > 0, "Pele mount durations were not measured");
    assert.ok(browser.markdownStages.peleAdapt?.length > 0, "Pele width adaptation was not measured");
    assert.ok(browser.frameGaps.length >= 30, "animation-frame samples were not collected");
    const frameGapP95Ms = percentile(browser.frameGaps, 0.95);
    return {
      round: index,
      historyRows: initial.rows,
      imageCount: initial.images,
      toolCount: initial.tools,
      thinkingCount: initial.thinking,
      loadedHistoryMs,
      historyReachabilityMs,
      oldestImageDecoded: true,
      switchOutMs: switchOut.clickToRenderMs,
      switchOutPreparationMs: switchOut.preparationMs,
      switchOutRpcMs: switchOut.rpcResponseMs,
      switchOutRenderAfterRpcMs: switchOut.renderAfterRpcMs,
      switchBackMs: switchBack.clickToRenderMs,
      switchBackPreparationMs: switchBack.preparationMs,
      switchBackRpcMs: switchBack.rpcResponseMs,
      switchBackRenderAfterRpcMs: switchBack.renderAfterRpcMs,
      toolToggleP50Ms: percentile(toolToggleMs, 0.5),
      toolToggleP95Ms: percentile(toolToggleMs, 0.95),
      thinkingToggleP50Ms: percentile(thinkingToggleMs, 0.5),
      thinkingToggleP95Ms: percentile(thinkingToggleMs, 0.95),
      scrollMs,
      typingMs,
      firstDeltaMs,
      markdownRenderCommitP95Ms: percentile(browser.markdownStages.renderCommit, 0.95),
      peleMountP95Ms: percentile(browser.markdownStages.peleMount, 0.95),
      peleAdaptP95Ms: percentile(browser.markdownStages.peleAdapt, 0.95),
      mermaidCharts: initial.mermaidCharts,
      narrowDiagramWidth,
      screenshotPath,
      jsHeapMB: heap / (1024 * 1024),
      domNodes: nodeCount,
      longTaskCount: longTaskDurations.length,
      longestTaskMs: longTaskDurations.length ? Math.max(...longTaskDurations) : 0,
      longTaskP95Ms: longTaskDurations.length ? percentile(longTaskDurations, 0.95) : 0,
      frameGapP95Ms,
      frameGapMaxMs: Math.max(...browser.frameGaps),
      frameGapsOver50Ms,
      maxRSSMB: Math.max(...memory.map(sample => sample.totalRSSMB)),
      maxRendererRSSMB: Math.max(...memory.map(sample => sample.rendererRSSMB)),
      memorySamples: memory.length,
      memoryTimeline: memory,
      longTaskDurationsMs: longTaskDurations,
      frameGapsMs: browser.frameGaps,
      providerRequests: provider.requests,
    };
  } catch (error) {
    const failure = {
      round: index,
      phase,
      elapsedMs: Date.now() - app.spawnEpochMs,
      error: String(error),
      memoryTimeline: memory,
    };
    throw Object.assign(new Error(`Complex-session stress failed during ${phase}: ${String(error)}`), { performanceFailure: failure });
  } finally {
    keepSampling = false;
    await memorySampler;
    cdp?.close();
    await app.stop();
  }
}

test("complex session stress measures rendering, images, expansion, input, streaming and resource pressure", async () => {
  const fixtureDirectory = join(outputRoot, "stress-fixture");
  await mkdir(fixtureDirectory, { recursive: true });
  const longFixture = await writeConversationFixture(fixtureDirectory, "long-session", { pairs: pairsPerSession, imageEvery: 24, toolEvery: 20, toolAtEnd: true, complexMarkdown: true });
  const provider = await startProvider();
  const samples = [];
  const failedAttempts = [];
  const progressPath = join(outputRoot, `stress-${calibration ? "calibration" : "test"}-progress.json`);
  let attempts = 0;
  const maxAttempts = rounds + Math.max(5, Math.ceil(rounds * 0.5));
  try {
    while (samples.length < rounds && attempts < maxAttempts) {
      const index = attempts++;
      try {
        samples.push(await measureRound(index, longFixture, provider));
      } catch (error) {
        failedAttempts.push(error.performanceFailure || { round: index, phase: "startup", error: String(error) });
      } finally {
        provider.releaseDiagramFence();
      }
      await writeFile(progressPath, JSON.stringify({
        platform: process.platform, arch: process.arch, requestedRounds: rounds,
        completedRounds: samples.length, attempts, maxAttempts, failedAttempts,
        complete: samples.length === rounds, fixture: longFixture, samples,
      }, null, 2));
    }
  } finally {
    await new Promise(resolve => provider.server.close(resolve));
  }

  const metrics = [
    "loadedHistoryMs", "historyReachabilityMs", "markdownRenderCommitP95Ms", "peleMountP95Ms", "peleAdaptP95Ms", "switchOutMs", "switchOutPreparationMs", "switchOutRpcMs", "switchOutRenderAfterRpcMs",
    "switchBackMs", "switchBackPreparationMs", "switchBackRpcMs", "switchBackRenderAfterRpcMs", "toolToggleP95Ms", "thinkingToggleP95Ms",
    "scrollMs", "typingMs", "firstDeltaMs", "jsHeapMB", "maxRSSMB", "maxRendererRSSMB",
    "longTaskP95Ms", "frameGapP95Ms", "frameGapMaxMs", "frameGapsOver50Ms", "longestTaskMs",
  ];
  const summary = Object.fromEntries(metrics.map(name => [name, summarize(samples.map(sample => sample[name]))]));
  const report = { platform: process.platform, arch: process.arch, rounds, attempts, failedAttempts, fixture: longFixture, summary, samples };
  const reportPath = join(outputRoot, `stress-${calibration ? "calibration" : "test"}.json`);
  await writeFile(reportPath, JSON.stringify(report, null, 2));
  console.log(`Complex-session performance ${JSON.stringify({ rounds, fixture: longFixture, summary, reportPath })}`);
  assert.equal(samples.length, rounds, `collected ${samples.length} valid stress samples after ${attempts} attempts`);

  if (!calibration) {
    assert.equal(failedAttempts.length, 0, `${failedAttempts.length} failed attempts occurred during stress sampling`);
    const thresholds = JSON.parse(await readFile(join(import.meta.dirname, "thresholds.json"), "utf8"));
    assert.equal(thresholds.status, "fixed", "performance thresholds are awaiting calibration");
    for (const [metric, measured] of Object.entries(summary)) {
      const maximum = thresholds.stress[metric];
      if (maximum !== undefined) assert.ok(measured.p95 <= maximum, `${metric} P95 ${measured.p95} exceeds ${maximum}`);
    }
  }
});

