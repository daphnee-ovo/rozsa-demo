---
source: other
nums: 1
---

- [x] ISSUE-I067：fix: /login 浏览器打开失败 + URL 不显示
  - severity: P0
  - location：crates/rozsa-tui/src/backend/native.rs:1099
  - description：WSL 下 xdg-open 无法打开浏览器，且通知中的 URL 可能不可见
  - reproduce：/login 后无反应，不打开浏览器也不显示链接
  - fix：通知改为单行+WSL优先浏览器策略
