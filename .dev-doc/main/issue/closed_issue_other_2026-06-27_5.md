---
source: other
nums: 1
---

- [x] ISSUE-I009：单元测试迁移到 crate/tests/ + 禁止内嵌测试 pre-commit hook
  - severity: P1
  - location：tests/unit/, crates/*/tests/, devtools/
  - description：1) 将 tests/unit/app/ 下的测试移到 crates/rozsa-app/tests/，tests/unit/tui/ 移到 crates/rozsa-tui/tests/，tests/unit/model/ 移到 crates/rozsa-model/tests/。删除顶层 Cargo.toml 中的 [[test]] 注册，改用 crate 内 tests/ 约定。2) 新增 pre-commit hook (devtools/guard-no-inline-test.sh) 检测 #[cfg(test)] mod tests 模式，阻止提交内嵌测试代码。
  - reproduce：
  - fix：
