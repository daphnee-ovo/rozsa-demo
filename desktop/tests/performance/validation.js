const startupStages = new Set([
  "main-entry", "backend-process-started", "backend-initialize-started",
  "backend-initialize-response-received", "electron-ready", "backend-ready",
  "runtime-ready", "catalog-prefetch-started", "catalog-prefetch-ready",
  "latest-history-prefetch-started", "latest-history-prefetch-ready",
  "startup-session-ready", "window-created", "document-loaded", "window-shown",
  "window-focused",
]);
const startupRegions = new Set([
  "main-panel", "conversation", "composer", "message-input", "input-toolbar", "send-button",
]);
const stressStatistics = Object.freeze({
  loadedHistoryMs: "p95",
  historyReachabilityMs: "p95",
  markdownRenderCommitP95Ms: "p95",
  peleMountP95Ms: "p95",
  peleAdaptP95Ms: "p95",
  switchOutMs: "p95",
  switchOutPreparationMs: "p95",
  switchOutRpcMs: "p95",
  switchOutRenderAfterRpcMs: "p95",
  switchBackMs: "p95",
  switchBackPreparationMs: "p95",
  switchBackRpcMs: "p95",
  switchBackRenderAfterRpcMs: "p95",
  toolToggleP95Ms: "p95",
  thinkingToggleP95Ms: "p95",
  scrollMs: "p95",
  typingMs: "p95",
  firstDeltaMs: "p95",
  jsHeapMB: "p95",
  maxRSSMB: "max",
  maxRendererRSSMB: "max",
  longTaskP95Ms: "p95",
  frameGapP95Ms: "p95",
  frameGapMaxMs: "max",
  frameGapsOver50Ms: "max",
  longestTaskMs: "max",
});

function invalid(message) {
  throw new Error(`Invalid performance thresholds: ${message}`);
}

function requireObject(value, path) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) invalid(`${path} must be an object`);
}

function requireLimit(value, path, allowCalibrationNull) {
  if (value === null && allowCalibrationNull) return;
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) {
    invalid(`${path} must be a positive finite number${allowCalibrationNull ? " or null while calibrating" : ""}`);
  }
}

function requireMappedEntries(value, known, path, status) {
  requireObject(value, path);
  for (const [name, limit] of Object.entries(value)) {
    if (!known.has(name)) invalid(`${path}.${name} has no measured metric mapping`);
    requireLimit(limit, `${path}.${name}`, status === "calibrating");
  }
}

export function validateRounds(value, minimum) {
  const rounds = typeof value === "string" && value.trim() !== "" ? Number(value) : value;
  if (!Number.isSafeInteger(rounds) || rounds < minimum) {
    throw new Error(`ROZSA_PERF_ROUNDS must be an integer of at least ${minimum}; received ${String(value)}`);
  }
  return rounds;
}

export function validateThresholds(thresholds) {
  requireObject(thresholds, "root");
  if (thresholds.schemaVersion !== 1) invalid("schemaVersion must be 1");
  if (!Number.isSafeInteger(thresholds.roundsRequired) || thresholds.roundsRequired < 30) {
    invalid("roundsRequired must be an integer of at least 30");
  }
  if (!["calibrating", "fixed"].includes(thresholds.status)) invalid("status must be calibrating or fixed");

  const { startup, stress } = thresholds;
  requireObject(startup, "startup");
  const startupKeys = new Set([
    "fullyReadyMaxMs", "mainStagesMs", "visibleRegionsMs", "firstContentfulPaintMs",
    "loadedHistoryReadyMs", "visibleElementSpreadMs", "memoryPeakMB",
  ]);
  for (const key of Object.keys(startup)) if (!startupKeys.has(key)) invalid(`startup.${key} has no measured metric mapping`);
  requireObject(startup.fullyReadyMaxMs, "startup.fullyReadyMaxMs");
  for (const mode of ["cold", "warm"]) requireLimit(startup.fullyReadyMaxMs[mode], `startup.fullyReadyMaxMs.${mode}`, false);
  for (const mode of Object.keys(startup.fullyReadyMaxMs)) {
    if (mode !== "cold" && mode !== "warm") invalid(`startup.fullyReadyMaxMs.${mode} is an unknown mode`);
  }
  if ("mainStagesMs" in startup) requireMappedEntries(startup.mainStagesMs, startupStages, "startup.mainStagesMs", thresholds.status);
  if ("visibleRegionsMs" in startup) requireMappedEntries(startup.visibleRegionsMs, startupRegions, "startup.visibleRegionsMs", thresholds.status);
  for (const key of ["firstContentfulPaintMs", "loadedHistoryReadyMs", "visibleElementSpreadMs", "memoryPeakMB"]) {
    if (key in startup) requireLimit(startup[key], `startup.${key}`, thresholds.status === "calibrating");
  }

  requireObject(stress, "stress");
  requireMappedEntries(stress, new Set(Object.keys(stressStatistics)), "stress", thresholds.status);
  return thresholds;
}

export function requireFormalThresholds(thresholds, calibration) {
  if (!calibration && thresholds.status !== "fixed") {
    throw new Error(`Performance thresholds are ${thresholds.status}; calibrate and approve thresholds before formal acceptance`);
  }
}

function requireStats(stats, metric) {
  if (!stats || typeof stats !== "object") throw new Error(`No performance samples were recorded for ${metric}`);
  return stats;
}

function requireFinite(value, metric) {
  if (typeof value !== "number" || !Number.isFinite(value)) throw new Error(`No finite performance measurement was recorded for ${metric}`);
  return value;
}

function compareLimit(stats, limit, metric, statistic = "p95") {
  const measured = requireFinite(requireStats(stats, metric)[statistic], `${metric}.${statistic}`);
  if (measured > limit) throw new Error(`${metric} ${statistic.toUpperCase()} ${measured} exceeds ${limit}`);
}

export function checkStartupThresholds(thresholds, mode, report) {
  const startup = thresholds.startup;
  const modeReport = report[mode];
  compareLimit(modeReport.fullyReadyMs, startup.fullyReadyMaxMs[mode], `${mode}.fullyReadyMs`, "max");
  if (modeReport.fullyReadyMs.p95 > startup.fullyReadyMaxMs[mode]) {
    throw new Error(`${mode}.fullyReadyMs P95 ${modeReport.fullyReadyMs.p95} exceeds ${startup.fullyReadyMaxMs[mode]}`);
  }
  for (const [stage, limit] of Object.entries(startup.mainStagesMs ?? {})) {
    compareLimit(modeReport.mainStages?.[stage], limit, `${mode}.mainStagesMs.${stage}`);
  }
  for (const [region, limit] of Object.entries(startup.visibleRegionsMs ?? {})) {
    compareLimit(modeReport.visibleRegions?.[region], limit, `${mode}.visibleRegionsMs.${region}`);
  }
  const metricStats = {
    firstContentfulPaintMs: modeReport.firstContentfulPaintMs,
    loadedHistoryReadyMs: modeReport.loadedHistoryReadyMs,
    visibleElementSpreadMs: modeReport.visibleElementSpreadMs,
    memoryPeakMB: modeReport.totalRSSMB,
  };
  for (const [metric, stats] of Object.entries(metricStats)) {
    const limit = startup[metric];
    if (limit !== undefined && limit !== null) {
      compareLimit(stats, limit, `${mode}.${metric}`, metric === "memoryPeakMB" ? "max" : "p95");
    }
  }
}

export function checkStressThresholds(thresholds, summary) {
  for (const [metric, limit] of Object.entries(thresholds.stress)) {
    if (limit === null) continue;
    const statistic = stressStatistics[metric];
    if (!statistic) invalid(`stress.${metric} has no statistic mapping`);
    compareLimit(summary[metric], limit, `stress.${metric}`, statistic);
  }
}
