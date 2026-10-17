import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { test } from "node:test";
import { attachPage, createEnvironment, outputRoot, sampleProcessMemory, startElectron } from "./harness.js";
import { writeConversationFixture } from "./fixtures.js";

const rounds = Math.max(1, Number(process.env.ROZSA_PERF_ROUNDS || 1));
const calibration = process.env.ROZSA_PERF_CALIBRATION === "1";
const requiredRegions = ["main-panel", "conversation", "composer", "message-input", "input-toolbar", "send-button"];
const mainStageNames = ["main-entry", "backend-process-started", "backend-initialize-started", "electron-ready", "backend-ready", "runtime-ready", "catalog-prefetch-started", "catalog-prefetch-ready", "latest-history-prefetch-started", "latest-history-prefetch-ready", "startup-session-ready", "window-created", "document-loaded", "window-shown"];
const backendStageNames = ["process-entry", "session-service-ready", "runtime-initialization-started", "runtime-initialization-complete", "runtime-initialization-failed", "rpc-server-starting", "initialize-request-received", "initialize-response-ready", "session-catalog-started", "session-catalog-complete", "session-history-started", "session-history-complete"];

function percentile(values, fraction) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.max(0, Math.ceil(sorted.length * fraction) - 1)];
}

function summarize(values) {
  return {
    samples: values.length,
    min: Math.min(...values),
    p50: percentile(values, 0.5),
    p95: percentile(values, 0.95),
    max: Math.max(...values),
    mean: values.reduce((sum, value) => sum + value, 0) / values.length,
  };
}

async function measureLaunch(environment, fixture, mode, index) {
  const app = await startElectron(environment);
  let cdp;
  try {
    await app.waitFor(() => /"stage":"window-shown"/.test(app.logs) ? true : undefined, "native window shown");
    cdp = await attachPage(await app.devToolsURL());
    await cdp.waitFor("performance.getEntriesByName('rozsa:ui-interactive-ready').length > 0", "renderer UI readiness");
    await cdp.waitFor(`[${requiredRegions.map(name => `performance.getEntriesByName('rozsa:visible:${name}').length > 0`).join(",")}].every(Boolean)`, "visible startup regions");
    await cdp.waitFor("performance.getEntriesByName('rozsa:session-catalog-rendered').length > 0", "initial session catalog rendered");
    const sessions = await cdp.evaluate("window.RozsaUIHost.port.request('get_sessions')");
    assert.ok(sessions.some(item => item.id === fixture.id), `startup fixture ${fixture.id} is missing from the catalog`);
    await cdp.waitFor("performance.getEntriesByName('rozsa:initial-history-rendered').length > 0", "initial selected history rendered", 30000);
    await cdp.waitFor("document.querySelector('#chatMessages [data-virtualized=true]')?.dataset.historyEntryCount === '1200'", "full 1200-message history mounted in the virtual list", 30000);
    await cdp.waitFor("document.querySelectorAll('#chatMessages .msg').length > 0", "latest conversation viewport rows", 30000);
    await cdp.waitFor("Array.from(document.querySelectorAll('#chatMessages img')).every(image => image.complete && image.naturalWidth > 0)", "initial image decode", 30000);
    const imageDecodeCompletedMs = Date.now() - app.spawnEpochMs;
    const latestMessageBottomDistancePx = await cdp.evaluate(`(() => {
      const chat=document.querySelector('#chatMessages');
      return chat.scrollHeight-chat.scrollTop-chat.clientHeight;
    })()`);
    const fullyReadyMs = Date.now() - app.spawnEpochMs;
    await cdp.waitFor("window.__ROZSA_PERF__?.markdownWorkerDurations.length > 0", "background Markdown preparation completed", 30000);
    const historyObservation = await cdp.evaluate(`(async()=>{
      const started=performance.now();
      const reply=await window.rozsa.switchSession({path:${JSON.stringify(fixture.path)},catalogId:${JSON.stringify(fixture.id)}});
      const rpcReady=performance.now();
      const snapshot=reply.snapshot;
      return {messageCount:snapshot?.messages.length,payloadBytes:new TextEncoder().encode(JSON.stringify(snapshot)).byteLength,switchMs:rpcReady-started};
    })()`);
    assert.equal(historyObservation.messageCount, fixture.entries, `Electron product bridge did not load fixture history: ${historyObservation.messageCount}`);
    await cdp.evaluate(`window.RozsaUIHost.port.request('switch_session',{path:${JSON.stringify(fixture.path)}})`);
    const renderer = await cdp.evaluate(`(() => {
      const entries = performance.getEntriesByType("mark").filter(entry => entry.name.startsWith("rozsa:"));
      const marks = Object.fromEntries(entries.map(entry => [entry.name.slice(6), performance.timeOrigin + entry.startTime]));
      const navigation = performance.getEntriesByType("navigation")[0];
      const paints = Object.fromEntries(performance.getEntriesByType("paint").map(entry => [entry.name, entry.startTime]));
      return {
        timeOrigin: performance.timeOrigin,
        marks,
        paints,
        navigation: navigation && {
          domInteractive: navigation.domInteractive,
          domContentLoaded: navigation.domContentLoadedEventEnd,
          load: navigation.loadEventEnd,
        },
        historyRows: document.querySelectorAll("#chatMessages .msg").length,
        historyEntryCount: Number(document.querySelector("#chatMessages [data-virtualized=true]")?.dataset.historyEntryCount || 0),
        initialRangeStart: Number(document.querySelector("#chatMessages [data-virtualized=true]")?.dataset.initialRangeStart ?? -1),
        imageCount: document.querySelectorAll("#chatMessages img").length,
        capture: window.__ROZSA_PERF__ && {
          longTaskSupported: window.__ROZSA_PERF__.longTaskSupported,
          markdownWorkerDurations: window.__ROZSA_PERF__.markdownWorkerDurations,
        },
      };
    })()`);
    const mainStages = await app.startupMarks();
    const backendStages = await app.backendStartupMarks();
    const rendererStages = Object.fromEntries(Object.entries(renderer.marks).map(([name, epoch]) => [name, epoch - app.spawnEpochMs]));
    const visibleElementMs = Object.fromEntries(Object.entries(rendererStages)
      .filter(([name]) => name.startsWith("visible:"))
      .map(([name, value]) => [name.slice("visible:".length), value]));
    const visibleTimes = Object.values(visibleElementMs);
    const loadedHistoryReadyMs = await cdp.evaluate("Date.now()") - app.spawnEpochMs;
    const processMemory = await sampleProcessMemory(app.child.pid);
    return {
      round: index,
      mode,
      processToMainEntryMs: mainStages["main-entry"],
      mainStagesMs: mainStages,
      backendStagesMs: backendStages,
      rendererStagesMs: rendererStages,
      paintsMs: renderer.paints,
      navigationMs: renderer.navigation,
      visibleElementMs,
      visibleElementSpreadMs: visibleTimes.length ? Math.max(...visibleTimes) - Math.min(...visibleTimes) : null,
      sessionCatalogRenderedMs: rendererStages["session-catalog-rendered"],
      sessionCatalogRenderDelayMs: rendererStages["session-catalog-rendered"] - rendererStages["session-catalog-response"],
      loadedHistoryReadyMs: rendererStages["initial-history-rendered"],
      historyDataReceivedMs: rendererStages["session-history-received"],
      firstConversationCommitMs: rendererStages["conversation-first-content-commit"],
      imageDecodeCompletedMs,
      latestMessageBottomDistancePx,
      fullyReadyMs,
      historyRows: renderer.historyRows,
      historyEntryCount: renderer.historyEntryCount,
      initialRangeStart: renderer.initialRangeStart,
      imageCount: renderer.imageCount,
      sourceJSONLBytes: fixture.sourceJSONLBytes,
      historySnapshotBytes: historyObservation.payloadBytes,
      historySwitchMs: historyObservation.switchMs,
      memory: processMemory,
      markdownWorkerDurations: renderer.capture?.markdownWorkerDurations || [],
      longTaskSupported: renderer.capture?.longTaskSupported === true,
    };
  } finally {
    cdp?.close();
    await app.stop();
  }
}

function modeReport(samples) {
  const mainStages = Object.fromEntries(mainStageNames.map(name => [name, summarize(samples.map(sample => sample.mainStagesMs[name]))]));
  const backendStages = Object.fromEntries(backendStageNames
    .filter(name => samples.some(sample => Number.isFinite(sample.backendStagesMs[name])))
    .map(name => [name, summarize(samples.filter(sample => Number.isFinite(sample.backendStagesMs[name])).map(sample => sample.backendStagesMs[name]))]));
  const visibleRegions = Object.fromEntries(requiredRegions.map(name => [name, summarize(samples.map(sample => sample.visibleElementMs[name]))]));
  return {
    rounds: samples.length,
    mainStages,
    backendStages,
    visibleRegions,
    visibleElementSpreadMs: summarize(samples.map(sample => sample.visibleElementSpreadMs)),
    sessionCatalogRenderedMs: summarize(samples.map(sample => sample.sessionCatalogRenderedMs)),
    sessionCatalogRenderDelayMs: summarize(samples.map(sample => sample.sessionCatalogRenderDelayMs)),
    firstContentfulPaintMs: summarize(samples.map(sample => sample.paintsMs["first-contentful-paint"])),
    markdownWorkerMaxMs: summarize(samples.map(sample => Math.max(0, ...sample.markdownWorkerDurations))),
    loadedHistoryReadyMs: summarize(samples.map(sample => sample.loadedHistoryReadyMs)),
    historyDataReceivedMs: summarize(samples.map(sample => sample.historyDataReceivedMs)),
    firstConversationCommitMs: summarize(samples.map(sample => sample.firstConversationCommitMs)),
    imageDecodeCompletedMs: summarize(samples.map(sample => sample.imageDecodeCompletedMs)),
    latestMessageBottomDistancePx: summarize(samples.map(sample => sample.latestMessageBottomDistancePx)),
    fullyReadyMs: summarize(samples.map(sample => sample.fullyReadyMs)),
    historySwitchMs: summarize(samples.map(sample => sample.historySwitchMs)),
    sourceJSONLBytes: summarize(samples.map(sample => sample.sourceJSONLBytes)),
    historySnapshotBytes: summarize(samples.map(sample => sample.historySnapshotBytes)),
    totalRSSMB: summarize(samples.map(sample => sample.memory.totalRSSMB)),
    rendererRSSMB: summarize(samples.map(sample => sample.memory.rendererRSSMB)),
  };
}

test("cold and warm Electron startup measure shell, conversation, images and visible-region timing", async () => {
  const byMode = { cold: [], warm: [] };
  const progressPath = join(outputRoot, `startup-${calibration ? "calibration" : "test"}-progress.json`);
  const persistProgress = () => writeFile(progressPath, JSON.stringify({
    platform: process.platform, arch: process.arch, roundsPerMode: rounds,
    completedCold: byMode.cold.length, completedWarm: byMode.warm.length,
    complete: false, cold: byMode.cold, warm: byMode.warm,
  }, null, 2));
  for (let index = 0; index < rounds; index++) {
    const environment = await createEnvironment(`startup-${index}`);
    const fixture = await writeConversationFixture(environment.sessions, `startup-history-${index}`, { pairs: 600, imageEvery: 0, imageAtEnd: true, toolEvery: 40 });
    byMode.cold.push(await measureLaunch(environment, fixture, "cold", index));
    await persistProgress();
    byMode.warm.push(await measureLaunch(environment, fixture, "warm", index));
    await persistProgress();
  }

  for (const samples of Object.values(byMode)) {
    for (const sample of samples) {
      for (const region of requiredRegions) assert.ok(Number.isFinite(sample.visibleElementMs[region]), `startup did not record visible region ${region}`);
      assert.equal(sample.longTaskSupported, true, "Chromium long-task observation is required for responsiveness measurements");
      assert.ok(sample.markdownWorkerDurations.length > 0, "complex startup history must be prepared in a Web Worker");
      assert.equal(sample.historyEntryCount, 1200, `expected full 1200-message history data, saw ${sample.historyEntryCount}`);
      assert.ok(sample.historyRows > 0 && sample.historyRows < sample.historyEntryCount, `expected bounded viewport DOM rows, saw ${sample.historyRows}/${sample.historyEntryCount}`);
      assert.ok(sample.initialRangeStart >= sample.historyEntryCount - 64, `first mounted message range started at ${sample.initialRangeStart}, not near the latest history`);
      assert.ok(sample.sessionCatalogRenderDelayMs <= 100, `session catalog render lagged its response by ${sample.sessionCatalogRenderDelayMs}ms`);
      assert.ok(sample.imageCount > 0, "startup history fixture must render local images");
      assert.ok(sample.latestMessageBottomDistancePx <= 48, `default session did not settle at its latest message after image decode: ${sample.latestMessageBottomDistancePx}px from bottom`);
    }
  }

  const report = {
    platform: process.platform,
    arch: process.arch,
    roundsPerMode: rounds,
    cold: modeReport(byMode.cold),
    warm: modeReport(byMode.warm),
    samples: [...byMode.cold, ...byMode.warm],
  };
  const reportPath = join(outputRoot, `startup-${calibration ? "calibration" : "test"}.json`);
  await writeFile(reportPath, JSON.stringify(report, null, 2));
  console.log(`Startup performance ${JSON.stringify({ rounds, cold: report.cold, warm: report.warm, reportPath })}`);

  for (const mode of ["cold", "warm"]) {
    for (const sample of byMode[mode]) {
      assert.ok(sample.fullyReadyMs <= 3000, `${mode} round ${sample.round} reached window, session catalog, 600-pair history and decoded local images in ${sample.fullyReadyMs}ms; limit is 3000ms`);
    }
    assert.ok(report[mode].fullyReadyMs.p95 <= 3000, `${mode} fully-ready P95 ${report[mode].fullyReadyMs.p95}ms exceeds the fixed 3000ms limit`);
  }

  if (!calibration) {
    const thresholds = JSON.parse(await readFile(join(import.meta.dirname, "thresholds.json"), "utf8"));
    assert.equal(thresholds.status, "fixed", "performance thresholds are awaiting calibration");
    for (const mode of ["cold", "warm"]) {
      for (const [stage, stats] of Object.entries(report[mode].mainStages)) {
        const maximum = thresholds.startup[mode]?.mainStagesMs?.[stage];
        if (maximum !== undefined) assert.ok(stats.p95 <= maximum, `${mode} ${stage} P95 ${stats.p95}ms exceeds ${maximum}ms`);
      }
      const maximumHistory = thresholds.startup[mode]?.loadedHistoryReadyP95Ms;
      if (maximumHistory !== undefined) assert.ok(report[mode].loadedHistoryReadyMs.p95 <= maximumHistory, `${mode} history P95 exceeds ${maximumHistory}ms`);
      const maximumFullyReady = thresholds.startup[mode]?.fullyReadyP95Ms;
      if (maximumFullyReady !== undefined) assert.ok(report[mode].fullyReadyMs.p95 <= maximumFullyReady, `${mode} fully-ready P95 exceeds ${maximumFullyReady}ms`);
      const maximumSwitch = thresholds.startup[mode]?.historySwitchP95Ms;
      if (maximumSwitch !== undefined) assert.ok(report[mode].historySwitchMs.p95 <= maximumSwitch, `${mode} history-switch P95 exceeds ${maximumSwitch}ms`);
    }
  }
});

