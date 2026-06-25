---
source: audit
nums: 1
---

- [ ] ISSUE-I008：三套 Bridge 协议未统一（维护成本 3x）
  - severity: P2
  - location：crates/rozsa-core/src/protocol.rs, crates/rozsa-model/src/protocol.rs, crates/rozsa-app/src/main.rs
  - description：三个 crate 各自定义 BridgeInput/BridgeOutput 并独立运行 stdin/stdout loop，无共用 trait。协议演进需同步修改三处。注：这些是 TS 迁移期过渡产物，完全迁移后可能自然消亡。
  - reproduce：
  - fix：
