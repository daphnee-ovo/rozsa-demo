---
source: test
nums: 1
---

- [x] ISSUE-I016：Test TASK-T029 fail:node:internal/modules/esm/get_format:243
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
    ✖ message-states.test.js (102.520333ms)
    ℹ tests 1
    ℹ suites 0
    ℹ pass 0
    ℹ fail 1
    ℹ cancelled 0
    ℹ skipped 0
    ℹ todo 0
    ℹ duration_ms 111.579042
    
    ✖ failing tests:
    
    test at message-states.test.js:1:1
    ✖ message-states.test.js (102.520333ms)
      'test failed'
  - reproduce：node --test 'message-states.test.js'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：当前正式入口 npm --prefix desktop/tests test 使用 tsx --test；其中 message-states.test.js 与其他 TSX 组件测试成功加载 conversation-view.tsx，未复现 ERR_UNKNOWN_FILE_EXTENSION。该记录仅针对旧裸 node --test 复现方式；完整正式入口仍有与 TSX 加载无关的既有失败，见本次验证报告。
  - files_modify: [desktop/tests/message-states.test.js]
  - files_create: []
