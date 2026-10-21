import { describe, expect, it } from "vitest";
import {
  checkStartupThresholds,
  checkStressThresholds,
  requireFormalThresholds,
  validateRounds,
  validateThresholds,
} from "./performance/validation.js";

const copyThresholds = () => ({
  schemaVersion: 1,
  status: "calibrating",
  roundsRequired: 30,
  startup: {
    fullyReadyMaxMs: { cold: 4000, warm: 3000 },
    firstContentfulPaintMs: null,
    loadedHistoryReadyMs: null,
  },
  stress: { maxRSSMB: null, frameGapMaxMs: null },
});

describe("performance acceptance configuration", () => {
  it("requires at least the configured 30 rounds instead of silently accepting a short run", () => {
    expect(validateRounds("30", 30)).toBe(30);
    expect(() => validateRounds("29", 30)).toThrow("at least 30");
  });

  it("rejects non-integer and non-numeric round counts before a benchmark starts", () => {
    for (const value of ["1.5", "many", ""]) {
      expect(() => validateRounds(value, 30)).toThrow("must be an integer of at least 30");
    }
  });

  it("rejects a threshold name that has no matching measured metric", () => {
    const thresholds = copyThresholds();
    thresholds.stress.maxRSSMB = null;
    thresholds.stress.memoryPeakMB = 512;
    expect(() => validateThresholds(thresholds)).toThrow("stress.memoryPeakMB has no measured metric mapping");
  });

  it("keeps explicit null thresholds calibration-only and rejects them for fixed acceptance", () => {
    expect(validateThresholds(copyThresholds()).status).toBe("calibrating");
    const thresholds = copyThresholds();
    thresholds.status = "fixed";
    expect(() => validateThresholds(thresholds)).toThrow("startup.firstContentfulPaintMs must be a positive finite number");
  });

  it("maps a configured startup history limit to the measured P95 and fails when that measurement is absent", () => {
    const thresholds = copyThresholds();
    thresholds.startup.loadedHistoryReadyMs = 1200;
    const report = {
      cold: {
        fullyReadyMs: { p95: 3500, max: 3900 },
        loadedHistoryReadyMs: { p95: 1300 },
      },
    };
    expect(() => checkStartupThresholds(thresholds, "cold", report)).toThrow("cold.loadedHistoryReadyMs P95 1300 exceeds 1200");
    expect(() => checkStartupThresholds(thresholds, "cold", { cold: { fullyReadyMs: { p95: 3500, max: 3900 } } }))
      .toThrow("No performance samples were recorded for cold.loadedHistoryReadyMs");
  });

  it("uses the maximum RSS sample instead of allowing a high peak outside P95", () => {
    const thresholds = copyThresholds();
    thresholds.stress.maxRSSMB = 800;
    expect(() => checkStressThresholds(thresholds, { maxRSSMB: { p95: 700, max: 820 } }))
      .toThrow("stress.maxRSSMB MAX 820 exceeds 800");
  });

  it("uses the maximum frame gap when the configured threshold names a maximum", () => {
    const thresholds = copyThresholds();
    thresholds.stress.frameGapMaxMs = 60;
    expect(() => checkStressThresholds(thresholds, { frameGapMaxMs: { p95: 55, max: 70 } }))
      .toThrow("stress.frameGapMaxMs MAX 70 exceeds 60");
  });

  it("rejects a configured stress threshold when no samples were recorded", () => {
    const thresholds = copyThresholds();
    thresholds.stress.maxRSSMB = 800;
    expect(() => checkStressThresholds(thresholds, {}))
      .toThrow("No performance samples were recorded for stress.maxRSSMB");
  });

  it("does not allow formal acceptance while thresholds remain calibrating", () => {
    expect(() => requireFormalThresholds(copyThresholds(), false))
      .toThrow("Performance thresholds are calibrating");
    expect(() => requireFormalThresholds(copyThresholds(), true)).not.toThrow();
  });
});
