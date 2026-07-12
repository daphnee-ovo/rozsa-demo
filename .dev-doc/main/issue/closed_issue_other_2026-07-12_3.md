---
source: other
nums: 1
---

- [x] ISSUE-I003：Deny and hints 支持自定义拒绝理由
  - severity: P0
  - location：crates/rozsa-gui/frontend/app.js:1933
  - description：点击或键盘确认 Deny and hints 后应进入 hint 输入态；输入框固定显示不可删除的 Deny, 前缀，用户填写内容作为自定义 hint，提交后拒绝当前工具并把 hint 传给 agent。
  - reproduce：触发 permission 请求，点击 Deny and hints 或选中后按 Tab；输入框应显示 Deny, ，前缀不可删除，提交后 agent 收到用户理由。
  - fix：Deny and hints 点击、H 快捷键或选中后 Tab 会进入 hint 输入态；输入框固定保留 Deny, 前缀并拦截删除；Enter 或 Deny 按钮提交自定义 hint，GUI command 将 hint 传给 PermissionResponse::DenyWithHint。
  - files_modify: [crates/rozsa-gui/frontend/app.js, crates/rozsa-gui/frontend/index.html, crates/rozsa-gui/src/commands.rs, crates/rozsa-gui/tests/permission_hint_test.rs]
  - files_create: [crates/rozsa-gui/tests/permission_hint_test.rs]
