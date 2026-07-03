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
- 19:08 fix: ISSUE-I014：agent loop: compaction stop 在 tool result 后中断，model 无机会生成最终回答
- 19:45 fix: ISSUE-I030：agent loop: hook panic 导致整个 loop task crash — 无 panic 保护
- 20:37 fix: ISSUE-I026：agent loop: 缺失 tool_execution_update 事件 — 长时间 tool 无进度反馈
- 20:56 fix: ISSUE-I033：agent loop: 关键 hook 全为同步 — 限制 async compaction/context transform 等场景

## 2026-07-03
- 22:48 fix: ISSUE-I052：Provider: 缺少 OpenAI Responses / Google / Vertex / Mistral / Azure / Cloudflare 共 7 个 provider
