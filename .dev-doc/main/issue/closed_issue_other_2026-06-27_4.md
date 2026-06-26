---
source: other
nums: 1
---

- [x] ISSUE-I008：TUI 架构重构 — 目录重组 + render 去 JSON 化 + 公共 widget 抽取
  - severity: P2
  - location：crates/rozsa-tui/src/components/common.rs (新建)
  - description：当前 TUI 架构问题：1) components/ 混装面板/数据逻辑/布局块，分类不清。2) render.rs 1600 行消费 JSON Value 而非 Rust 类型，需要 view_model.rs 适配层。3) 缺少可复用 widget（tab_bar, hints_bar 等）。重构内容：A) 目录重组为 backend/ input/ render/ panels/ widgets/ util/ data/ theme/ command/。B) render.rs 从消费 Value 改为直接消费 AgentMessage，删除 view_model.rs。C) 从 render.rs 拆出 messages.rs/input_box.rs/status.rs/dialog.rs。D) 抽取公共 widget（tab_bar, hints_bar, filterable_list, search_input）。E) overlay.rs 移入 render/，editor.rs 移入 input/。详细目录规划见 docs/tui/architecture.md。
  - reproduce：
  - fix：
