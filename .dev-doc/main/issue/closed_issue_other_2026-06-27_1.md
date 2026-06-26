---
source: other
nums: 1
---

- [x] ISSUE-I005：Graph: UserContent round-trip 不对称导致 old session user 节点丢失
  - severity: P2
  - location：crates/rozsa-model/src/types_serde.rs, crates/rozsa-tui/src/backend/native.rs
  - description：UserContent::Text 序列化为数组格式，反序列化后变成 UserContent::Blocks。native.rs /graph 只匹配 Text 变体，Blocks 被丢弃。load old session 后 graph 看不到历史 user 节点。复现：新建 session 发消息 → 退出 → switch 回来 → /graph 只看到 assistant 和新 user。修复：UserContent 增加 pub fn text() 统一方法，消费方改用。
  - reproduce：
  - fix：
