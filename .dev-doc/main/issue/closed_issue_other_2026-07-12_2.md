---
source: other
nums: 1
---

- [x] ISSUE-I002：permission 请求面板未按设计图对齐
  - severity: P0
  - location：crates/rozsa-gui/frontend/index.html:1908
  - description：Permission 面板未按用户截图实现：tool name 应作为醒目 pill 放在标题下，description 紧邻显示；命令区域需要按长度折叠并提供展开全部命令按钮；操作项应为 Y Allow Once、T Trust in session、N Deny Execute、H Deny and hints。当前仍显示 A/D、风险标签和旧布局。
  - reproduce：触发任意需要审批的 Bash 请求，逐项对照截图检查标题区、tool pill、description、命令展开按钮和四个操作项。
  - fix：按截图重排 permission 面板为标题、tool pill + description、命令框和四个操作项；命令显示 Bash $ 前缀与语法高亮；Y/T/N/H 操作键和文案对齐；移除风险标签。
  - files_modify: [crates/rozsa-gui/frontend/app.js, crates/rozsa-gui/frontend/index.html, crates/rozsa-gui/tests/permission_display_test.rs]
  - files_create: []
