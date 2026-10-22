---
source: test
nums: 1
---

- [x] ISSUE-I031：Test TASK-T039 fail:FAIL	rozsa/backend/internal/transport/server [build failed]
  - severity: P1
  - location：backend/internal/transport/server/interruption_test.go
  - description：FAIL	rozsa/backend/internal/transport/server [build failed]
    FAIL
    # rozsa/backend/internal/transport/server [rozsa/backend/internal/transport/server.test]
    internal/transport/server/concurrency_test.go:415:31: undefined: rawFrames
    internal/transport/server/interruption_test.go:142:40: undefined: rawFrames
  - reproduce：go test ./internal/transport/server
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：Restored the rawFrames test-side view of jsonrpc2 native frame methods; transport package compiles and passes.
  - files_modify: [backend/internal/transport/server/interruption_test.go]
  - files_create: []
