---
source: test
nums: 1
---

- [x] ISSUE-I004：普通消息词内下划线被误判为未覆盖 Markdown 而留空
  - severity: P1
  - location：crates/rozsa-gui/frontend/ui/scripts/features/tools.js:93
  - description：进入日期：2026-10-02。T018 的未覆盖 Markdown 判定把 QA_PERMISSION_ALLOW 等标识符的词内下划线视作 emphasis，导致已经发送并保存的普通消息正文显示为空容器。仅修复普通标识符误判，保留真实 emphasis 与其他未覆盖 Markdown family 的暂停契约。
  - reproduce：调用 messageTextMarkup(QA_PERMISSION_ALLOW)、messageTextMarkup(some_identifier_name) 得到 suspended 空容器；隔离模拟 AI API 的原生 GUI 测试中用户正文不可见。
  - fix：下划线 emphasis 判定增加 Unicode 词边界，普通标识符正文不再留空；保留真实 emphasis 和其他未覆盖 Markdown 契约。回归测试修复前失败、修复后通过，三个实际 prompt 与 snake_case 经隔离 mock API 在原生 GUI 确认可见，证据仅放 tmp。
  - files_modify: [crates/rozsa-gui/frontend/ui/scripts/features/tools.js, crates/rozsa-gui/tests/shared_tools_ui_test.rs]
  - files_create: [tmp/ui-architecture-refactor/native-message-underscore/]
