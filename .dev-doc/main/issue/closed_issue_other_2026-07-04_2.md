---
source: other
nums: 1
---

- [x] ISSUE-I068：Model registry: auth.json 被当作模型配置解析导致启动失败
  - severity: P1
  - location：crates/rozsa-app/src/model_registry/mod.rs:282
  - description：ModelRegistry 扫描 ~/.rozsa/models/*.json 时会把 OAuth credential 文件 auth.json 也交给 ModelsConfig 解析；auth.json 顶层没有 providers，启动时报 Failed to parse models.json: missing field providers。
  - reproduce：~/.rozsa/models/ 下同时存在 auth.json 和 codex-oauth.json；运行 ./target/release/rozsa 时 registry 扫描 *.json，把 auth.json 当 models config 解析，或要求 codex-oauth provider 必须有 apiKey，导致启动失败。
  - fix：ModelRegistry 扫描模型配置时跳过 auth.json；ProviderConfig 解析 authHeader，并允许 authHeader=true 的 OAuth provider 不写 apiKey；补充 auth.json 扫描过滤和 codex-oauth authHeader 回归测试。
