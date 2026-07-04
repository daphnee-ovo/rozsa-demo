
- e2e对于二进制的，实际可以改用python进行测试。
- 探索tui测试方式，问gpt？
- model冗余，或考虑不内嵌那么多模型的api和价格
- model结构不清晰，且之前和ts的桥接层没有去除。

- tui现在能够渲染diff吗？
- graph能够显示渲染diff吗？
- p0 分析codex cli，真实实现codex oauth
- ban 掉其他oauth（无法实际测试）

## 长线规划（从 TS 差距审计转入）

- **Provider: 7 个 provider 缺失** — OpenAI Responses (WebSocket)、OpenAI Codex Responses、Google Gemini、Google Vertex AI、Mistral、Azure OpenAI Responses、Cloudflare。参考：legacy-ts/packages/ai/src/providers/
- **Extension 系统动态加载** — 当前仅有 5 hook trait (156 行)，缺少动态模块加载、UI context API、tool/command 注册。参考：legacy-ts/packages/coding-agent/src/core/extensions/
- **LSP 集成** — 无 LSP client 实现（只有 /lsp 模式开关）。TS 支持 9 种语言服务器 + diagnostics/definition/references/hover。参考：legacy-ts/packages/coding-agent/src/core/lsp/
- **HTML 导出** — /export 目前仅输出 JSONL。TS 有 standalone HTML 含 CSS/JS/ANSI 渲染/tool 折叠。参考：legacy-ts/packages/coding-agent/src/core/export-html/
- **Image Generation API** — 有 image model metadata registry 但无实际生成 API client。参考：legacy-ts/packages/ai/src/images.ts + providers/images/
- **RPC mode** — stdin/stdout JSONL 协议 (25+ 命令)，用于 IDE 扩展和程序化 API 集成。参考：legacy-ts/packages/coding-agent/src/modes/rpc/
