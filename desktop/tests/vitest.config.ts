import { defineConfig } from "vitest/config";

export default defineConfig({
  root: import.meta.dirname,
  test: {
    include: [
      "notification-center.test.js",
      "session-ui.test.ts",
      "input-model.test.js",
      "ime-input.test.js",
      "ime-runtime.test.js",
      "general-settings.test.js",
      "shared-settings.test.js",
      "**/*.vitest.test.{js,ts,tsx}",
    ],
    environment: "node",
    globals: false,
    fileParallelism: false,
    maxWorkers: 1,
  },
});
