---
source: audit
nums: 1
---

- [x] ISSUE-I003：Dev-flow sidebar/detail/dashboard 改动导致 GUI 不可用
  - severity: P0
  - location：crates/rozsa-gui/frontend/
  - description：T007 提交后应用完全不可用：需要定位 sidebar 或 main view 的前端/后端回归并修复，恢复会话切换、设置、对话等核心功能。
  - reproduce：启动应用后核心交互不可用（用户报告 completely unusable）
  - fix：根因：真实 dow dashboard 会输出带尾部杂质文本的 issue ID（如 ISSUE-I001：Test TASK-T002 fail），rozsa 的严格 ID 校验使整个 snapshot 解码失败，dashboard 启动探测 5s 超时，导致全部 dev-flow GUI 功能不可用。修复：decode 时用 normalize_id 提取 canonical 前缀+数字作为权威 ID，容忍尾部杂质，同时仍拒绝无前缀/无数字的无效 ID；新增污染 ID 回归测试；顺带修复侧边栏/详情 "1 Issues" 单复数文案。
  - files_modify: [crates/rozsa-gui/frontend/sidebar.js, crates/rozsa-gui/frontend/app.js, crates/rozsa-gui/frontend/sidebar.html, crates/rozsa-gui/frontend/index.html, crates/rozsa-gui/src/commands.rs, crates/rozsa-gui/src/dev_flow.rs, crates/rozsa-gui/src/state.rs, crates/rozsa-gui/src/lib.rs, crates/rozsa-app/src/dev_flow/registry.rs, crates/rozsa-app/src/dev_flow/dashboard.rs, crates/rozsa-app/tests/dev_flow_dashboard_test.rs]
  - files_create: []
