---
source: other
nums: 1
---

- [ ] ISSUE-I010：模型列表/价格动态分发 — 替代 include_str 硬编码 JSON
  - severity: P2
  - location：crates/rozsa-app/src/model_registry/mod.rs
  - description：当前 models.generated.json 和 image-models.generated.json 通过 include_str! 编译时嵌入，更新模型列表必须重新编译。需要设计动态分发机制：可选方案包括启动时从远程拉取、本地可覆盖配置文件、或定期自动更新缓存。需考虑离线场景 fallback。
  - reproduce：
  - fix：
