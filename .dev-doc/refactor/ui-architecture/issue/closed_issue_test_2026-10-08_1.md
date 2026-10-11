---
source: test
nums: 1
---

- [x] ISSUE-I007：共享 UI 覆盖库存测试仍依赖旧模板与事件结构
  - severity: P2
  - location：crates/rozsa-gui/tests/prototype_coverage_inventory_test.rs:94
  - description：进入日期：2026-10-08。T022 验证发现四个既有库存失败：Sidebar 测试读取转发模板却要求内联 aside；non-visual notification 项的空 css/markup 与统一非空断言冲突；app.js 事件库存未反映 notification adapter 注册位置；GAP-003/UI-QUESTION-001 缺少库存要求的 runtime_trigger 等字段。HEAD 已包含上述状态，本轮未修改相关实现；新增 GAP-014 字段已补齐。应在共享资源清理与验证任务中按真实架构修复库存契约。
  - reproduce：cargo test -p rozsa-gui --test prototype_coverage_inventory_test
  - fix：按 SPEC 共享真源与 non-visual 契约调整库存检查：校验原型转发路径和实际共享模板/交互根；notification 事件按 runtime adapter 实际注册位置登记；non-visual 项 css/markup 可为空；补齐既有 AskUserQuestion 缺口字段。修复前库存测试 2 通过/4 失败，修复后 6/6 通过。
  - files_modify: [crates/rozsa-gui/tests/prototype_coverage_inventory_test.rs, prototype/docs/UI_COVERAGE.json, prototype/docs/DESIGN_GAPS.md]
  - files_create: []
