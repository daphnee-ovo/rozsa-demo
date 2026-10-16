---
source: test
nums: 1
---

- [x] ISSUE-I003：Test TASK-T015 fail:(node:46298) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
  - severity: P1
  - location：backend/internal/app/settings/settings_test.go
  - description：(node:46298) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
    (Use `node --trace-deprecation ...` to show where the warning was created)
    /Users/xinyue/ballad/rozsa-demo/desktop/tests/package-resources.test.ts:64
        if (!diagnosticSeen) { clearTimeout(timeout); reject(new Error(`Packaged app exited before startup diagnostic (${code}):\n${logs}`)); }
                                                             ^
    
    Error: Packaged app exited before startup diagnostic (null):
    
        at ChildProcess.<anonymous> (/Users/xinyue/ballad/rozsa-demo/desktop/tests/package-resources.test.ts:64:58)
        at ChildProcess.wrapper (node:events:639:12)
        at ChildProcess.emit (node:events:514:20)
        at ChildProcess._handle.onexit (node:internal/child_process:323:12)
    
    Node.js v26.10.0
  - reproduce：npm --prefix desktop/tests exec -- tsx desktop/tests/package-resources.test.ts
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：提权重跑 npm --prefix desktop/tests exec -- tsx desktop/tests/package-resources.test.ts，输出 Forge packaged resources PASS；提权执行 dow task done TASK-T015 -v 后 test.ci run 1 PASS。
  - files_modify: [backend/internal/app/settings/settings_test.go, backend/internal/platform/configpaths/configpaths_test.go]
  - files_create: []
