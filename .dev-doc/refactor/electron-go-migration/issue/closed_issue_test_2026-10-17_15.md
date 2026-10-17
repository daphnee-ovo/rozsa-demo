---
source: test
nums: 1
---

- [x] ISSUE-I015：Test TASK-T023 fail:(node:64026) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
  - severity: P1
  - location：backend/internal/model/bedrock/provider_test.go
  - description：(node:64026) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
    (Use `node --trace-deprecation ...` to show where the warning was created)
    /Users/xinyue/ballad/rozsa-demo/desktop/tests/package-resources.test.ts:59
        reject(new Error(`Packaged startup diagnostic timed out:\n${logs}`));
               ^
    
    Error: Packaged startup diagnostic timed out:
    
        at Timeout._onTimeout (/Users/xinyue/ballad/rozsa-demo/desktop/tests/package-resources.test.ts:59:12)
        at listOnTimeout (node:internal/timers:685:17)
        at process.processTimers (node:internal/timers:618:7)
    
    Node.js v26.10.0
  - reproduce：npm --prefix desktop/tests exec -- tsx desktop/tests/package-resources.test.ts
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：提权重跑记录的复现命令后通过：Forge packaged resources PASS。未改产品代码；该验收运行需提权执行。
  - files_modify: [backend/internal/model/bedrock/provider_test.go, backend/internal/app/runtime/runtime_test.go]
  - files_create: []
