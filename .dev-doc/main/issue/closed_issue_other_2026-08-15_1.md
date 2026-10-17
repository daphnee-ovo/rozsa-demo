---
source: other
nums: 1
---

- [x] ISSUE-I001：同步 T001 覆盖文档与 styles 组件库验证
  - severity: P1
  - location：docs/gui/NEW_VERSION_MIGRATION_COVERAGE.md
  - description：TASK-T013 已将新版原型从单体 rozsa-gui.css 拆为 styles/ 组件库并删除兼容入口。T001 创建的覆盖文档、机器清单和 inventory test 需要以 styles/main.css、sidebar.css、source-order.json、组件文件和 scene override 为新的权威输入，并直接验证组件库完整性与 HTML 入口。
  - reproduce：检查 T001 产物：现有文档仅局部提到 styles/；inventory test 只验证 scene 加载 main.css，尚未双向发现 styles/ 文件、验证 entry/import 注册及 scene override 对应关系。
  - fix：原 GUI 路线的覆盖文档同步问题已由合并的共享 UI 迭代承接：当前真源为 prototype/docs/UI_COVERAGE.json、UI_COVERAGE.md、DESIGN_GAPS.md 和 prototype/styles/README.md；旧 NEW_VERSION 文档不再适用。2026-10-12 当前主线 prototype_coverage_inventory_test 六项通过，验证双向 inventory、路径及文档/缺口对应关系。按用户确认关闭历史记录，无新增代码变更。
  - files_modify: [docs/gui/NEW_VERSION_MIGRATION_COVERAGE.md, docs/gui/NEW_VERSION_MIGRATION_COVERAGE.json, docs/gui/NEW_VERSION_PROTOTYPE_GAPS.md, crates/rozsa-gui/tests/prototype_coverage_inventory_test.rs]
  - files_create: []
