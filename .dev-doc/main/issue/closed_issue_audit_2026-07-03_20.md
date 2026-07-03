---
source: audit
nums: 1
---

- [x] ISSUE-I066：Provider: 缺少 Image Generation API — 无法生成图片
  - severity: P1
  - location：crates/rozsa-model/src/lib.rs
  - description：TS 有完整 Image Generation 子系统：ImagesApi + image-models registry + OpenRouter image provider + register-builtins。支持通过 Chat API with modalities 生成图片。Rust 完全没有 image generation 能力。实现参考：legacy-ts/packages/ai/src/images.ts + image-models.ts + providers/images/。方案：新建 rozsa-model/src/images/ 模块，定义 ImageGenerationRequest/Response 类型，实现 OpenRouter image provider。
  - reproduce：尝试让 agent 生成图片，无此能力
  - fix：转入 docs/TODO.md — 长线规划，不作为当前迭代 issue 跟踪
