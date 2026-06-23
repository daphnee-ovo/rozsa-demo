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

## 2026-06-07
- feat: migrate OAuth login to rozsa-model Rust layer
- refactor: extension compat mapping + cleanup (T007)
- refactor: migrate type imports and remove model registry TS fallback (T004-T006)
- refactor: eliminate ts-ai middleman (T001-T003)
- refactor: rename all pi/Pi identifiers to rozsa

## 2026-06-16
- docs: 同步 task_2026-06-06_1 — 确认 T001-T007 全部完成并归档
- feat: implement Anthropic Messages Rust provider (payload, SSE stream, auth routing)
- feat: thinking/reasoning config (adaptive + budget-based)
- feat: compat layer (Fireworks, Cloudflare, Copilot, OAuth stealth mode)
- test: TS/Rust parity test for Anthropic Messages provider (5 cases, all pass)
- docs: move Anthropic Messages from Deferred to Supported in supported-providers.md
- feat: register anthropic-messages in rust-supported-apis.ts
- 15:51 feat: Anthropic Messages payload 构建

## 2026-06-18
- 16:55 fix: ISSUE-I002：smoke test — anthropic-messages custom provider JSON 端到端验证
