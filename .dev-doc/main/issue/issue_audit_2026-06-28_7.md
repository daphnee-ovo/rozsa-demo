---
source: audit
nums: 1
---

- [ ] ISSUE-I020：agent loop: 并行 tool 执行无并发上限
  - severity: P2
  - location：crates/rozsa-core/src/agent_loop.rs:466
  - description：execute_parallel 对所有 tool call 直接 tokio::spawn 无 Semaphore。若 model 一次返回大量 tool call，全部同时执行可能压垮 IO 或触发 rate limit。应加 Semaphore 限制并发度。
  - reproduce：
  - fix：
