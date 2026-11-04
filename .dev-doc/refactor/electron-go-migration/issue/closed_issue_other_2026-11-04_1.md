---
source: other
nums: 1
---

- [x] ISSUE-I036：修复 Bash 权限摘要闪烁及折叠时丢失高亮
  - severity: P1
  - location：desktop/renderer/scripts/components/permission-request-panels.tsx:256
  - description：折叠时隐藏完整内容导致尺寸测量在溢出与不溢出之间反复切换，折叠副本未使用语法高亮。
  - reproduce：打开超过两行的 Bash permission request，观察摘要并切换展开状态。
  - fix：折叠时保留完整内容用于尺寸测量，首尾摘要复用高亮内容；原生回归测试待后续验证。
  - files_modify: [desktop/renderer/scripts/components/permission-request-panels.tsx, desktop/renderer/styles/composites/permission-question.css, desktop/tests/approval-question-lifecycle.native.test.js]
  - files_create: []
