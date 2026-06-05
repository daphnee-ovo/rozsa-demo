---
source: other
nums: 1
---

- [x] ISSUE-I001：[Provider] 和 model_id 之间需要空格
  - severity: P2
  - location：crates/rozsa-tui/src/components/model_selector.rs:122
  - description：format_model_display 的格式串需从 "[{}]{}" 改为 "[{}] {}"，展示效果为 [OpenAI] gpt-5.5
  - fix：格式串改为 "[{}] {}"，加入空格分隔

