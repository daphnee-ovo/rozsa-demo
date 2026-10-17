---
source: test
nums: 1
---

- [x] ISSUE-I002：Test TASK-T076 fail:(node:25293) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
  - severity: P1
  - location：test.ci run 1
  - description：(node:25293) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
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
  - fix：The package resource test fails to launch a copied macOS app under the sandbox; the same failure reproduced from T014 commit 6ed2c25. Running the required TASK-T076 completion check with host GUI permissions passed.
