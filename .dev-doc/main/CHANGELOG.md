# Changelog

## 2026-06-26
- 14:02 fix: ISSUE-I003：cycle_edit_mode 为 stub — 编辑模式切换无效
- 16:38 fix: ISSUE-I002：Multi-agent 运行时缺失 — switch_agent 不可用

## 2026-06-27
- 00:22 fix: ISSUE-I006：Graph: 搜索交互问题 — 缺 hints + 误触进入 + backspace 无效
- 00:29 fix: ISSUE-I007：Graph: 增加 tool call/result 节点 — 默认隐藏，按 o 切换显示
- 02:49 fix: autocomplete 快速输入前缀替换 + think_first 模式接入 tool gate
- 02:54 fix: ISSUE-I009：单元测试迁移到 crate/tests/ + 禁止内嵌测试 pre-commit hook
- 03:46 fix: ISSUE-I010：模型列表/价格动态分发 — 替代 include_str 硬编码 JSON
- 03:55 fix: ISSUE-I011：为各 Rust crate 编写详细接口文档

## 2026-06-28
- 18:19 fix: ISSUE-I013：TUI 消息区虚拟滚动 — 只渲染可见消息
