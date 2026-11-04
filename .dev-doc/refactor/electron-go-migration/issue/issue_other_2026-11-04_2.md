---
source: other
nums: 1
---

- [ ] ISSUE-I037：修正审批原生测试按键事件与格式
  - severity: P1
  - location：desktop/tests/approval-question-lifecycle.native.test.js:213
  - description：新增的 Z 快捷键测试把事件发往 document，无法触发面板内的处理器；测试文件也未通过格式检查。
  - reproduce：运行原生审批测试或提交前格式检查。
  - fix：
  - files_modify: [desktop/tests/approval-question-lifecycle.native.test.js]
