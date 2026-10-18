---
source: other
nums: 1
---

- [x] ISSUE-I018：更正 T040 恢复决策记录的 Rust 类型名
  - severity: P2
  - location：docs/app/README.md:207
  - description：T040 收口决策记录写为 SharedState::restore_agent；crates/rozsa-gui/src/state.rs:448 定义 SharedResources，impl SharedResources:460 中 restore_agent:524 为实际方法。仅更正该类型名。
  - reproduce：对照 docs/app/README.md 恢复边界决策段与 crates/rozsa-gui/src/state.rs 的 SharedResources::restore_agent。
  - fix：仅将 docs/app/README.md 恢复边界决策中的 SharedState::restore_agent 更正为 SharedResources::restore_agent；已对照 crates/rozsa-gui/src/state.rs:448、460、524 核对。
  - files_modify: [docs/app/README.md]
  - files_create: []
