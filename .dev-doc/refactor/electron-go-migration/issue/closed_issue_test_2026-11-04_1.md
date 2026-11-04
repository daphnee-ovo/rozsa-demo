---
source: test
nums: 1
---

- [x] ISSUE-I035：Test TASK-T195 fail:RUN  v5.0.5 /Users/xinyue/ballad/rozsa-demo/desktop/tests
  - severity: P1
  - location：desktop/tests/approval-question-lifecycle.native.test.js
  - description：RUN  v5.0.5 /Users/xinyue/ballad/rozsa-demo/desktop/tests
    No test files found, exiting with code 1
    
    filter: approval-question-lifecycle.native.test.js
    include: agent-session-avatar.test.js, approval-question-lifecycle.test.js, code-highlighter.test.ts, conversation-components.test.js, message-states.test.js, notification-center.test.js, session-ui.test.ts, shared-interruption.test.js, shared-suspension.test.js, shared-tools.test.js, notification-position.test.js, error-notification-routing.test.js, dev-flow-settings.test.js, dev-flow-sidebar.test.js, dev-flow-tool-presentation.test.js, shared-dev-flow.test.js, shared-sidebar.test.js, shared-viewport.test.js, session-message-routing.test.ts, electron-product-entry.test.js, native-appearance.test.ts, prototype-contract.test.js, prototype-gallery.test.js, shared-ui-host-contract.test.js, input-model.test.js, ime-input.test.js, ime-runtime.test.js, general-settings.test.js, shared-settings.test.js, **/*.vitest.test.{js,ts,tsx}
    exclude:  **/node_modules/**, **/.git/**
  - reproduce：npm exec -- vitest run 'approval-question-lifecycle.native.test.js'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：Removed the native node:test file from T195's Vitest test list; the native check remains a separate command.
  - files_modify: [desktop/tests/approval-question-lifecycle.native.test.js]
  - files_create: []
