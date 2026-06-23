---
source: other
nums: 1
---

- [x] ISSUE-I004：fake-anthropic-server 增加长文本流式输出和 tool 调用场景
  - severity: P2
  - location：devtools/fake-anthropic-server.py
  - description：fake server 需要增加：1）长文本分多个 delta chunk 逐步流式输出模拟真实场景，2）可控的 tool 调用（通过用户消息触发词控制），验证 Rust backend 对 streaming + tool call 的完整处理
  - fix：
