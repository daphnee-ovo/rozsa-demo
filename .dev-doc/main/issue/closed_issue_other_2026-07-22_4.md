---
source: other
nums: 1
---

- [x] ISSUE-I005：隐藏未配置凭据的 Bedrock 模型
  - severity: P1
  - location：crates/rozsa-gui/src/commands.rs:1481
  - description：GUI list_models 无条件返回全部注册模型，导致没有 AWS 凭据时仍显示 amazon-bedrock 模型；应按 provider_available 过滤 Bedrock。
  - reproduce：清除 AWS 环境变量并确保用户 AWS credentials 文件不存在，启动 GUI 后模型列表仍显示项目 Bedrock 配置中的模型。
  - fix：GUI 模型列表仅在 Amazon Bedrock 凭据已配置时显示 Bedrock 模型；保留其他 provider，新增回归测试。
  - files_modify: [crates/rozsa-gui/src/commands.rs]
  - files_create: [crates/rozsa-gui/tests/model_list_presentation_test.rs]
