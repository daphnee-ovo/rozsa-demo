---
source: devtest
nums: 1
---

- [ ] ISSUE-I088: devtest failed: TASK-T081: Implement GUI queue steer stop input state
  - severity: P1
  - source: devtest
  - location: TASK-T081: Implement GUI queue steer stop input state
  - current: crates/rozsa-gui/tests/queue_steer_test.rs: crates/rozsa-gui/tests/queue_steer_test.rs: line 1: use: command not found
  - expected: task passes devtest
  - reproduce: dow test --task
  - root_cause:
  - fix:
  - close_when: Re-running devtest returns PASS
