---
source: test
nums: 1
---

- [x] ISSUE-I001：Test TASK-T014 fail:node:internal/modules/run_main:111
  - severity: P1
  - location：desktop/tests/package-resources.test.ts
  - description：node:internal/modules/run_main:111
        triggerUncaughtException(
        ^
    Error [ERR_MODULE_NOT_FOUND]: Cannot find module '/Users/xinyue/ballad/rozsa-demo/{test_files}' imported from /Users/xinyue/ballad/rozsa-demo/
        at finalizeResolution (node:internal/modules/esm/resolve:241:11)
        at moduleResolve (node:internal/modules/esm/resolve:846:10)
        at defaultResolve (node:internal/modules/esm/resolve:973:11)
        at nextResolve (node:internal/modules/esm/hooks:769:28)
        at resolveBase (file:///Users/xinyue/ballad/rozsa-demo/desktop/tests/node_modules/tsx/dist/esm/index.mjs?1792167735570:2:3744)
        at resolveDirectory (file:///Users/xinyue/ballad/rozsa-demo/desktop/tests/node_modules/tsx/dist/esm/index.mjs?1792167735570:2:4243)
        at resolveTsPaths (file:///Users/xinyue/ballad/rozsa-demo/desktop/tests/node_modules/tsx/dist/esm/index.mjs?1792167735570:2:4984)
        at resolve (file:///Users/xinyue/ballad/rozsa-demo/desktop/tests/node_modules/tsx/dist/esm/index.mjs?1792167735570:2:5361)
        at nextResolve (node:internal/modules/esm/hooks:769:28)
        at AsyncLoaderHooksOnLoaderHookWorker.resolve (node:internal/modules/esm/hooks:265:30) {
      code: 'ERR_MODULE_NOT_FOUND',
      url: 'file:///Users/xinyue/ballad/rozsa-demo/%7Btest_files%7D'
    }
    
    Node.js v26.10.0
  - reproduce：npm --prefix desktop/tests exec -- tsx {test_files}
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：Added .dev-doc/test.ci with the direct tsx command for desktop/tests/package-resources.test.ts; verified the task-scoped completion gate passes.
  - files_modify: [desktop/tests/package-resources.test.ts]
  - files_create: []
