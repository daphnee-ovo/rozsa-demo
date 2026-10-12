---
source: test
nums: 1
---

- [x] ISSUE-I001：GUI token 测试依赖未创建的 temp 目录
  - severity: P2
  - location：crates/rozsa-gui/tests/commands/token_tests.rs
  - description：quoted_file_reference_tokens_with_spaces_highlight 使用 ../../temp 作为 tempdir_in 根目录，但未创建该目录。仓库不存在 temp/ 时 fixture 初始化返回 NotFound，GUI 全模块测试提前失败；该实现已存在于本次测试迁移前的提交，当前文件无改动。临时文件应遵循项目 tmp/ 约定，测试应自主管理 fixture 根目录。
  - reproduce：在没有 temp/ 的 checkout 运行 cargo test -p rozsa-gui --lib quoted_file_reference_tokens_with_spaces_highlight，commands.rs:2945 unwrap 返回 NotFound。
  - fix：原迭代 I006：fixture 改用项目 tmp/ 并在创建 TempDir 前 create_dir_all；七个 token 测试移至独立 tests/commands/token_tests.rs，保留原断言。原失败用例修复前 NotFound、修复后通过；cargo test -p rozsa-gui --lib 七项全通过，fmt 与 diff 检查通过。
  - files_modify: [crates/rozsa-gui/src/commands.rs]
  - files_create: [crates/rozsa-gui/tests/commands/token_tests.rs]
