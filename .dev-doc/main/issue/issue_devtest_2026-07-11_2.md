---
source: devtest
nums: 1
---

- [ ] ISSUE-I087: devtest failed: TASK-T070: feat: GUI permission runtime and per-session trust
  - severity: P1
  - source: devtest
  - location: TASK-T070: feat: GUI permission runtime and per-session trust
  - current: crates/rozsa-app/tests/permission_controller_test.rs: crates/rozsa-app/tests/permission_controller_test.rs: line 1: use: command not found; crates/rozsa-gui/tests/permission_runtime_test.rs: crates/rozsa-gui/tests/permission_runtime_test.rs: line 1: use: command not found; crates/rozsa-app/tests/permissions_test.rs: crates/rozsa-app/tests/permissions_test.rs: line 1: use: command not found
  - expected: task passes devtest
  - reproduce: dow test --task
  - root_cause:
  - fix:
  - close_when: Re-running devtest returns PASS
