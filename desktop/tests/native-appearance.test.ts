import assert from "node:assert/strict";
import test from "node:test";
import { applySidebarVibrancy } from "../main/native-appearance.ts";

test("sidebar vibrancy applies and clears the macOS sidebar material", () => {
  const calls: Array<string | null> = [];
  const window = { isDestroyed: () => false, setVibrancy: (type: string | null) => calls.push(type) };
  applySidebarVibrancy(window, true, "darwin");
  applySidebarVibrancy(window, false, "darwin");
  assert.deepEqual(calls, ["sidebar", null]);
});

test("sidebar vibrancy is unsupported off macOS and after window destruction", () => {
  const calls: Array<string | null> = [];
  const liveWindow = { isDestroyed: () => false, setVibrancy: (type: string | null) => calls.push(type) };
  applySidebarVibrancy(liveWindow, true, "linux");
  applySidebarVibrancy({ ...liveWindow, isDestroyed: () => true }, true, "darwin");
  assert.deepEqual(calls, []);
});
