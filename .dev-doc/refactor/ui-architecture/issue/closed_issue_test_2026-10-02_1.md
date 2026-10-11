---
source: test
nums: 1
---

- [x] ISSUE-I003：共享 permission 命令预览丢失 executable 后的空格
  - severity: P2
  - location：crates/rozsa-gui/frontend/ui/scripts/features/permissions.js:36
  - description：进入日期：2026-10-02。原生 GUI 实测 bash tool call 的实际命令为 printf 'ROZSA_PERMISSION_ALLOW
    '，面板却显示 printf'ROZSA_PERMISSION_ALLOW
    '。共享 renderer 拼接 executable 与剩余参数时丢掉分隔空格。该逻辑沿用原型实现；不影响真实 command 执行，但预览与实际请求不一致。
  - reproduce：通过本地模拟 AI API 返回带参数的 bash tool call，在真实 GUI Permission Request 查看 command preview。tmp/ui-architecture-refactor/native-permission-api/permission-request-native.jpg 已复现。
  - fix：共享 command renderer 按原始 leading/executable/arguments 分段转义，保留空格 Tab 换行和无参数命令；Node 回归修复前失败、修复后 3 项通过；本地模拟 API 触发的真实 macOS PermissionRequest AX 显示正确的 printf 与参数分隔。
  - files_modify: [crates/rozsa-gui/frontend/ui/scripts/features/permissions.js, crates/rozsa-gui/js-tests/shared-interruption.test.js]
  - files_create: [tmp/ui-architecture-refactor/TASK-T010/native/permission/]
