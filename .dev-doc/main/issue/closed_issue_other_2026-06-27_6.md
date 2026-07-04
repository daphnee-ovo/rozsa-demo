---
source: other
nums: 1
---

- [x] ISSUE-I010：模型注册重构 — 去除 include_str 嵌入，改为 ~/.rozsa/models/*.json 用户配置
  - severity: P2
  - location：crates/rozsa-app/src/model_registry/mod.rs
  - description：当前 models.generated.json (498KB) 通过 include_str! 编译时嵌入所有模型元数据。新方案：1) 删除 models.generated.json 和 image-models.generated.json；2) ModelRegistry 改为扫描 ~/.rozsa/models/ 目录下所有 .json 文件；3) 每个 JSON 描述一组模型（provider 名 + 协议类型 + 模型列表 + 认证方式）；4) 协议字段 (protocol) 决定走哪个 provider 实现：anthropic / openai-completions / openai-responses；5) auth 方式可能仍嵌入（OAuth flow 等），模型列表不再嵌入。用户通过新建如 minimax.json、deepseek.json 即可接入任意兼容 provider。
  - reproduce：
  - fix：删除 include_str! 嵌入的 models.generated.json (498KB) 和 image-models.generated.json (14KB)；新增 ModelRegistry::load_from_dir() 扫描 ~/.rozsa/models/*.json；CLI 改用新入口；新增 docs/model/models-config.md 示例文档
