# Changelog


## 2026-06-04
- 18:43 Wire Rust model bridge smoke tests
- 19:28 refactor: 调整 shouldAttemptRustModelProvider 路由逻辑

## 2026-06-05
- 13:30 Merge pull request #4 from daphnee-ovo/refactor/model
- 13:56 feat: Rust 侧实现 auth_status_per_provider 计算
- 14:22 feat: Rust 侧实现 provider_available 计算
- 15:55 feat: 模型可用性筛选迁移到 Rust 侧（方案B）
- 16:47 infra: 添加 AWS SDK 依赖并配置 workspace
- 18:25 feat: 增加 AWS Bedrock Converse Stream provider 支持
- 18:54 feat: 统一模型选择器界面，展示格式改为 [Provider] model_id
- 19:06 feat: ModelSelectorState 增加 tab 状态与按 provider 筛选逻辑

## 2026-06-06
- 22:48 fix: disambiguate native model switching
- 23:32 refactor: 提取模型协议类型到 packages/model-types
