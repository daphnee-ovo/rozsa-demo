---
source: audit
nums: 1
---

- [x] ISSUE-I001：修正 GUI 迁移覆盖文档语言与实质性验证
  - severity: P1
  - location：crates/rozsa-gui/tests/prototype_coverage_inventory_test.rs
  - description：TASK-T001 交付的两份迁移文档未使用中文，inventory 测试主要断言 Markdown 包含字符串，不能验证 covered/partial/missing 分类是否有真实 runtime/prototype 证据。改为中文文档，并以机器可读 manifest 驱动对运行时入口、原型场景 fixture、DOM/JS 证据及 no-op/缺失证据的直接验证。
  - reproduce：运行 cargo test -p rozsa-gui --test prototype_coverage_inventory_test 并检查断言：现有测试即使文档虚构映射，只要包含预期字符串也可通过。
  - fix：将覆盖与缺口文档改为中文；新增机器可读覆盖清单；重写 focused test，使其直接验证磁盘场景集合、scene identity、原版 CSS/JS 引用、runtime/prototype/blocking 源码 token、covered/missing 证据约束及实际事件注册集合。
  - files_modify: [docs/gui/NEW_VERSION_MIGRATION_COVERAGE.md, docs/gui/NEW_VERSION_PROTOTYPE_GAPS.md, crates/rozsa-gui/tests/prototype_coverage_inventory_test.rs]
  - files_create: [docs/gui/NEW_VERSION_MIGRATION_COVERAGE.json]
