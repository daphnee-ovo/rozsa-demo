---
source: devtest
nums: 1
---

- [ ] ISSUE-I001: devtest failed: TASK-T087: test: verify fake-model GUI event forwarding
  - severity: P1
  - source: devtest
  - location: TASK-T087: test: verify fake-model GUI event forwarding
  - current: crates/rozsa-gui/tests/gui_event_forwarder_test.rs: crates/rozsa-gui/tests/gui_event_forwarder_test.rs: line 1: use: command not found
  - expected: task passes devtest
  - reproduce: dow test --task
  - root_cause:
  - fix:
  - close_when: Re-running devtest returns PASS
