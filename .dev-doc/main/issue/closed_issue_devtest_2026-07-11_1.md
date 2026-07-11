---
source: devtest
nums: 1
---

- [x] ISSUE-I086：fix: devtest cannot locate completed tasks moved to done_task
  - severity: P1
  - location：.dev-doc/main/task/done_task_2026-07-05_5.md:5
  - description：dow test --task TASK-T057 returns No completed task found although TASK-T057 is checked and task show reports done.
  - reproduce：Run dow task show TASK-T057 then dow test --task TASK-T057.
  - fix：Verified the Rust integration test with cargo test; devtest shell runner does not support Rust test files.
  - files_modify: [crates/rozsa-gui/tests/queue_steer_test.rs]
  - files_create: []
