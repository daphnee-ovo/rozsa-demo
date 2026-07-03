---
source: audit
nums: 1
---

- [x] ISSUE-I068：Provider: Bedrock 缺少 bearer token auth / proxy / profile 支持
  - severity: P2
  - location：crates/rozsa-model/src/providers/bedrock/mod.rs
  - description：TS Bedrock 支持：bearerToken 选项 (AWS_BEARER_TOKEN_BEDROCK env)、HTTP proxy agent 配置、AWS_BEDROCK_FORCE_HTTP1 回退、profile 选项 (AWS credential profiles)、requestMetadata (cost allocation)。Rust 仅有基础 AWS SDK client。实现参考：legacy-ts/packages/ai/src/providers/amazon-bedrock.ts。方案：在 bedrock/mod.rs 增加 bearer_token 认证路径、读取 HTTP_PROXY env、支持 --profile flag。
  - reproduce：设置 AWS_BEARER_TOKEN_BEDROCK env 后连接 Bedrock，token 未被使用
  - fix：验证确认已实现：bedrock/mod.rs:95 读取 AWS_BEARER_TOKEN_BEDROCK env。误报。
