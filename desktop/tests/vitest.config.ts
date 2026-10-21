import { defineConfig } from "vitest/config";

export default defineConfig({
  root: import.meta.dirname,
  test: {
    include: [
      "agent-session-avatar.test.js",
      "approval-question-lifecycle.test.js",
      "code-highlighter.test.ts",
      "conversation-components.test.js",
      "message-states.test.js",
      "notification-center.test.js",
      "session-ui.test.ts",
      "shared-interruption.test.js",
      "shared-suspension.test.js",
      "shared-tools.test.js",
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
