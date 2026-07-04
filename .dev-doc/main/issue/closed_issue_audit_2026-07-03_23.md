---
source: audit
nums: 1
---

- [x] ISSUE-I063：App: 缺少 version check — 用户无法知晓新版本
  - severity: P2
  - location：crates/rozsa-cli/src/main.rs
  - description：TS version-check.ts (100 行)：启动时查询 rozsa.dev/api/latest-version、semver 对比、支持 ROZSA_SKIP_VERSION_CHECK 和 ROZSA_OFFLINE env。Rust 无版本检查。实现参考：legacy-ts/packages/coding-agent/src/utils/version-check.ts。方案：在 CLI 启动时异步 GET 版本 API，使用 semver crate 比较，新版本时在状态栏显示提示。不阻塞启动。
  - reproduce：有新版本发布时，用户无任何提示
  - fix：CLI 启动时 tokio::spawn 异步 GET version API，支持 ROZSA_SKIP_VERSION_CHECK=1
