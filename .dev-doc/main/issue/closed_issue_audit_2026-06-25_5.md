---
source: audit
nums: 1
---

- [x] ISSUE-I005：Google/Vertex/Mistral/OpenAI-Responses provider 未实现
  - severity: P1
  - location：crates/rozsa-model/src/providers/
  - description：types.rs 中定义了 GoogleGenerativeAI、GoogleVertex、MistralConversations、OpenAIResponses 枚举值，但无实现文件。用户选中这些 provider 的模型后进程 panic。最低要求：选中时友好报错而非 panic。
  - reproduce：
  - fix：
