---
source: test
nums: 2
---

- [x] ISSUE-I032：Test TASK-T039 fail:FAIL	rozsa/backend/internal/transport/server [build failed]
  - severity: P1
  - location：backend/internal/transport/server/interruption_test.go
  - description：FAIL	rozsa/backend/internal/transport/server [build failed]
    FAIL
    # rozsa/backend/internal/transport/server_test [rozsa/backend/internal/transport/server.test]
    internal/transport/server/server_test.go:109:23: undefined: AppNotification
    internal/transport/server/server_test.go:112:74: undefined: AppNotification
    internal/transport/server/server_test.go:119:29: undefined: AppNotification
    internal/transport/server/server_test.go:132:19: undefined: AppNotification
    internal/transport/server/messages_test.go:40:91: undefined: MessageEventSubscription
    internal/transport/server/messages_test.go:78:59: undefined: testPeer
    internal/transport/server/messages_test.go:82:11: undefined: testPeer
    internal/transport/server/messages_test.go:172:2: undefined: errorKind
    internal/transport/server/messages_test.go:174:2: undefined: errorKind
    internal/transport/server/messages_test.go:197:2: undefined: errorKind
    internal/transport/server/server_test.go:132:19: too many errors
  - reproduce：go test ./internal/transport/server
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：Completed external-package test migration by qualifying exported server types and composing shared testPeer/error helpers; transport package passes.
  - files_modify: [backend/internal/transport/server/interruption_test.go]
  - files_create: []
- [x] ISSUE-I033：Test TASK-T039 fail:RUN  v5.0.5 /Users/xinyue/ballad/rozsa-demo/desktop/tests
  - severity: P1
  - location：desktop/tests/approval-question-lifecycle.test.js
  - description：RUN  v5.0.5 /Users/xinyue/ballad/rozsa-demo/desktop/tests
    
    
     Test Files   (1)
          Tests  4 passed (9)
         Errors  1 error
       Start at  02:52:26
       Duration  104.40s (import 98%, transform 2%)
    ⎯⎯⎯⎯⎯⎯ Unhandled Errors ⎯⎯⎯⎯⎯⎯
    
    Vitest caught 1 unhandled error during the test run.
    This might cause false positive tests. Resolve unhandled errors to make sure your tests are not affected.
    
    ⎯⎯⎯⎯⎯⎯ Unhandled Error ⎯⎯⎯⎯⎯⎯⎯
    Error: [vitest-pool]: Worker forks emitted error.
     ❯ EventEmitter.onTaskError node_modules/vitest/dist/chunks/index.1TqnEahn.js:5915:21
     ❯ EventEmitter.emit node:events:514:20
     ❯ ChildProcess.emitUnexpectedExit node_modules/vitest/dist/chunks/index.1TqnEahn.js:5390:22
     ❯ ChildProcess.emit node:events:514:20
     ❯ ChildProcess._handle.onexit node:internal/child_process:323:12
    
    Caused by: Error: Worker exited unexpectedly with signal SIGKILL during started state while running test file /Users/xinyue/ballad/rozsa-demo/desktop/tests/approval-question-lifecycle.test.js
     ❯ ChildProcess.emitUnexpectedExit node_modules/vitest/dist/chunks/index.1TqnEahn.js:5388:33
     ❯ ChildProcess.emit node:events:514:20
     ❯ ChildProcess._handle.onexit node:internal/child_process:323:12
    
    ⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯
  - reproduce：npm exec -- vitest run 'approval-question-lifecycle.test.js'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：Reproduced with the owning desktop/tests Vitest runner; approval-question-lifecycle passes 9/9.
  - files_modify: [desktop/tests/approval-question-lifecycle.test.js]
  - files_create: []
