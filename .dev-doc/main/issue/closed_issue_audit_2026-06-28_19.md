---
source: audit
nums: 1
---

- [x] ISSUE-I032：agent loop: get_api_key 定义但从未调用 — OAuth token 过期后无法刷新
  - severity: P2
  - location：crates/rozsa-core/src/config.rs:29
  - description：config.get_api_key 字段存在但 agent_loop.rs 从未使用。stream_options.api_key 在 build_loop_config 时固定。长时间 session 中 OAuth token 过期后请求失败。应在每次 stream 调用前调用 get_api_key 刷新。
  - reproduce：
  - fix：run_loop 中 401/auth error 时调用 get_api_key 刷新并重试一次
