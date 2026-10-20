import { defineConfig } from "vitest/config";

export default defineConfig({
  root: import.meta.dirname,
  test: {
    include: ["notification-center.test.js", "session-ui.test.ts", "**/*.vitest.test.{js,ts,tsx}"],
    environment: "node",
    globals: false,
    fileParallelism: false,
    maxWorkers: 1,
  },
});
