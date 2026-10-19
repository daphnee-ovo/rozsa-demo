---
source: test
nums: 1
---

- [x] ISSUE-I022：Test TASK-T100 fail:node:internal/modules/esm/get_format:243
  - severity: P1
  - location：desktop/tests/message-states.test.js
  - description：node:internal/modules/esm/get_format:243
      throw new ERR_UNKNOWN_FILE_EXTENSION(ext, filepath);
            ^
    
    TypeError [ERR_UNKNOWN_FILE_EXTENSION]: Unknown file extension ".tsx" for /Users/xinyue/ballad/rozsa-demo/desktop/renderer/scripts/features/conversation-view.tsx
        at Object.getFileProtocolModuleFormat [as file:] (node:internal/modules/esm/get_format:243:9)
        at defaultGetFormat (node:internal/modules/esm/get_format:283:36)
        at defaultLoadSync (node:internal/modules/esm/load:159:16)
        at #loadAndMaybeBlockOnLoaderThread (node:internal/modules/esm/loader:809:12)
        at #loadSync (node:internal/modules/esm/loader:841:53)
        at ModuleLoader.load (node:internal/modules/esm/loader:790:26)
        at ModuleLoader.loadAndTranslate (node:internal/modules/esm/loader:501:31)
        at #getOrCreateModuleJobAfterResolve (node:internal/modules/esm/loader:567:36)
        at afterResolve (node:internal/modules/esm/loader:614:52)
        at ModuleLoader.getOrCreateModuleJob (node:internal/modules/esm/loader:620:12) {
      code: 'ERR_UNKNOWN_FILE_EXTENSION'
    }
    
    Node.js v26.10.0
    ✖ message-states.test.js (93.30625ms)
    ℹ tests 1
    ℹ suites 0
    ℹ pass 0
    ℹ fail 1
    ℹ cancelled 0
    ℹ skipped 0
    ℹ todo 0
    ℹ duration_ms 96.376458
    
    ✖ failing tests:
    
    test at message-states.test.js:1:1
    ✖ message-states.test.js (93.30625ms)
      'test failed'
  - reproduce：node --test 'message-states.test.js'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：自动生成原因是 TASK-T100 完成门禁用裸 node 执行 TSX 测试。实现代码无此回归；相关测试已用项目 runner tsx --test 通过，性能校准样本亦通过。将测试执行器兼容性记录留在 TASK-T100，不独立跟踪。
  - files_modify: [desktop/tests/message-states.test.js]
  - files_create: []
