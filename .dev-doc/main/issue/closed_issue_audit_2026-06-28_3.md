---
source: audit
nums: 1
---

- [x] ISSUE-I016：agent loop: stream 中断后 context.messages 残留脏 partial message
  - severity: P1
  - location：crates/rozsa-core/src/agent_loop.rs:243
  - description：stream_assistant_response 在收到 Start 时 push partial message 到 context。若 stream 之后断掉（无 Done/Error），返回 None 但 context 残留半成品 message。当前 loop 直接结束所以不影响，但若扩展 fallback 逻辑就会踩坑。defensive fix: return None 前 pop partial。
  - reproduce：
  - fix：
