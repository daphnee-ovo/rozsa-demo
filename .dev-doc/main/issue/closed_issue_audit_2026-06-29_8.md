---
source: audit
nums: 1
---

- [x] ISSUE-I044：permission: RiskLevel扩充+路径风险检测
  - severity: P2
  - location：crates/rozsa-app/src/permissions/mod.rs
  - description：TS版7级RiskLevel(含network/git/unknown)+路径分析(工作区外/secret文件/重定向目标)。Rust版仅4级无路径分析。
  - reproduce：
  - fix：
