---
source: audit
nums: 1
---

- [x] ISSUE-I055：TUI: 缺少 clipboard 集成 — 无法复制/粘贴文本和图片
  - severity: P1
  - location：crates/rozsa-tui/src/app.rs
  - description：TS clipboard 系统 (3 文件 ~230 行)：多平台支持 (macOS pbcopy、Windows clip、Linux wl-copy/xclip/xsel、Termux)、OSC 52 远程 session fallback、native addon、图片粘贴 (Wayland wl-paste、X11 xclip)。Rust 完全没有 clipboard 集成。实现参考：legacy-ts/packages/coding-agent/src/utils/clipboard*.ts。方案：使用 arboard crate 或 CLI 工具 (wl-copy/xclip) 实现 copy/paste；OSC 52 作为 SSH fallback；/copy 命令复制最后 assistant 消息。
  - reproduce：执行 /copy 期望复制 assistant 消息到系统剪贴板，命令无效果
  - fix：验证确认已实现：native.rs /copy 使用 OSC52 + pbcopy/wl-copy/xclip 双通道。误报。
