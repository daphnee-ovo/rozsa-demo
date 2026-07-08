---
source: audit
nums: 1
---

- [ ] ISSUE-I084：fix: complete GUI slash, file mention, attachment, and context integration
  - severity: P0
  - location：crates/rozsa-gui
  - description：GUI implementation left gaps from TASK-T058: skill slash commands are not truly surfaced, / and @ valid-token highlighting is incomplete, attachment picker only supports macOS, and features need real verification against old TS TUI behavior.
  - reproduce：Use GUI input: try /skill commands, @ valid path highlighting, attachment button, and context ring hover.
  - fix：
