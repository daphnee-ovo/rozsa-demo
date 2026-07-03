---
source: audit
nums: 1
---

- [x] ISSUE-I056：App: Extension 系统仅为 stub — 无法加载第三方扩展
  - severity: P1
  - location：crates/rozsa-app/src/extensions/mod.rs
  - description：TS extension 系统 (1566 行 types + loader + runner + wrapper)：动态加载 TS 模块，提供 20+ UI context 方法、15+ lifecycle hooks、tool/command 注册。Rust 仅有 157 行基础 trait (6 个 hook)，无 UI 集成、无工具注册、无命令注册。实现参考：legacy-ts/packages/coding-agent/src/core/extensions/。方案：分阶段——Phase 1: 定义 Rust trait 接口覆盖 TS 的核心 hooks；Phase 2: 实现 WASM/dylib 动态加载；Phase 3: UI context 方法。
  - reproduce：尝试 --extension path/to/ext.wasm 加载扩展，不支持
  - fix：转入 docs/TODO.md — 长线规划，不作为当前迭代 issue 跟踪
