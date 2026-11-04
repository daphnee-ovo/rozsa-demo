---
source: other
nums: 1
---

- [x] ISSUE-I037：修正审批原生测试按键事件与格式
  - severity: P1
  - location：desktop/tests/approval-question-lifecycle.native.test.js:213
  - description：新增的 Z 快捷键测试把事件发往 document，无法触发面板内的处理器；测试文件也未通过格式检查。
  - reproduce：运行原生审批测试或提交前格式检查。
  - fix：修正面板内 Z 按键事件并格式化测试；隔离 Electron 应用包验证了高亮、点击展开和 Z 收起。
  - files_modify: [desktop/tests/approval-question-lifecycle.native.test.js, testdata/project-test-dirs/approval-question-lifecycle/.rozsa/settings.json]
  - files_create: []
