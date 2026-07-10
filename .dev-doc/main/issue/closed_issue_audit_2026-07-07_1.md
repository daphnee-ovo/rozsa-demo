---
source: audit
nums: 1
---

- [x] ISSUE-I084：fix: remove duplicate GUI /quit slash arm
  - severity: P2
  - location：crates/rozsa-gui/src/commands.rs
  - description：dispatch_slash_command contains duplicate "quit" match arms.
  - reproduce：Inspect dispatch_slash_command match arms.
  - fix：False positive: rg confirmed only one /quit match arm; duplicate was caused by overlapping sed ranges.
