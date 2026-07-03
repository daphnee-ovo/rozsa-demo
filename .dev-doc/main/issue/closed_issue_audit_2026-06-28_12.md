---
source: audit
nums: 1
---

- [x] ISSUE-I025：agent loop: ModelStreamFn 不接受 CancellationToken — 无法中止正在进行的 HTTP 请求
  - severity: P1
  - location：crates/rozsa-core/src/config.rs:11
  - description：TS 将 abort signal 传入 stream function 可中止 HTTP。Rust 的 ModelStreamFn 签名无 CancellationToken，stream_assistant_response 仅在收到下一 chunk 后才检查取消。model thinking 阶段 chunks 间隔数十秒时取消延迟不可控。需加 signal 参数或用 tokio::select! race。
  - reproduce：
  - fix：
