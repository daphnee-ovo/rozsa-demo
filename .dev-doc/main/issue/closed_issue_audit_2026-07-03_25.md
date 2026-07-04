---
source: audit
nums: 1
---

- [x] ISSUE-I065：App: 缺少 HTTP dispatcher — 无集中代理/超时配置
  - severity: P2
  - location：crates/rozsa-model/src/stream.rs
  - description：TS http-dispatcher.ts (56 行)：全局 HTTP dispatcher 配置 idle timeout、proxy (EnvHttpProxyAgent)、禁用 HTTP/2。Rust 无集中 HTTP 配置，各 provider 各自管理 reqwest client。实现参考：legacy-ts/packages/coding-agent/src/core/http-dispatcher.ts。方案：在 rozsa-model 中增加共享 HttpClient builder，统一读取 HTTP_PROXY/HTTPS_PROXY env、配置 timeout、connection pool。
  - reproduce：设置 HTTP_PROXY env 期望所有 API 请求走代理，实际无效
  - fix：新增 http_client.rs 共享 Client (timeout + proxy)，所有 provider 通过 common.rs 统一使用
