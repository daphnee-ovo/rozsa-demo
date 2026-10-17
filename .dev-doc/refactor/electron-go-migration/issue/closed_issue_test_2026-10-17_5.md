---
source: test
nums: 1
---

- [x] ISSUE-I005：Test TASK-T017 fail:(node:76692) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
  - severity: P1
  - location：backend/internal/app/session/manager_test.go
  - description：(node:76692) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
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
  - fix：TASK-T081 fixed the shared packaged-app check: its damaged .app copy now lives under project tmp/migration/install-tmp, framework symlinks stay relative (verbatimSymlinks), and --enable-logging=stderr exposes the expected ENOENT startup diagnostic. The elevated test.ci gate passed.
  - files_modify: [backend/internal/app/session/manager_test.go]
  - files_create: []
