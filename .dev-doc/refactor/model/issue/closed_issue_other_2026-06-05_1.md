---
source: other
nums: 1
---

- [x] ISSUE-I001：//model 界面模型展示格式改为 [Provider]model_id
  - severity: P2
  - location：crates/rozsa-tui/src/components/model_selector.rs:202
  - description：当前模型列表展示为 provider/model_id（如 openai/gpt-5.5），需改为 [Provider]model_id 格式（如 [OpenAI]gpt-5.5）。需增加 provider ID → 品牌显示名映射函数，同时更新第 83 行的模糊搜索 haystack 格式保持一致。
  - fix：增加 provider_display_name() 和 format_model_display() 函数，将模型列表渲染和模糊搜索 haystack 均改为 [Provider]model_id 格式

