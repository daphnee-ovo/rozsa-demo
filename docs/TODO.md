
- e2e对于二进制的，实际可以改用python进行测试。
- 探索tui测试方式，问gpt？
- model冗余，或考虑不内嵌那么多模型的api和价格
- model结构不清晰，且之前和ts的桥接层没有去除。

- tui现在能够渲染diff吗？
- graph能够显示渲染diff吗？
- p0 分析codex cli，真实实现codex oauth
- ban 掉其他oauth（无法实际测试）

## 延迟项（2026-07-12）

- **GUI packaging and update configuration**（原 TASK-T086）— 暂不处理跨平台安装包、更新配置和签名 endpoint；待发布渠道与平台前置条件明确后再拆任务。
- **Agent loop async hooks**（原 ISSUE-I033）— 暂不处理同步 hook 改为 async 的接口设计；待明确 compaction、context transform 和 steering queue 的异步需求后再排期。
- **Package Manager**（原 ISSUE-I057）— 暂不实现扩展/技能安装管理；npm/git 支持范围、包格式、锁文件和离线行为尚未确认。
- **Auto-approve small-model permission reviewer**（原 TASK-T044，2026-07-30 移入 TODO）— 前后端统一使用 `auto-approve` 命名；当前仍作为未实现模式，选择时必须明确报错且不得持久化。后续实现时，仅对匹配 `ask` 的工具调用交给配置的小模型判断安全性与权限范围；`deny` 和 `allow` 保持现有优先级并绕过 reviewer。Reviewer 的 `approve` 直接放行，`reject` 阻止执行，`uncertain`、模型错误或超时回退到用户审批。传入 reviewer 的工具参数、作用域和 workspace 上下文必须经过敏感信息脱敏，并补齐运行时、失败与超时路径的回归测试。

## 长线规划（从 TS 差距审计转入）

- **Provider: 7 个 provider 缺失** — OpenAI Responses (WebSocket)、OpenAI Codex Responses、Google Gemini、Google Vertex AI、Mistral、Azure OpenAI Responses、Cloudflare。参考：legacy-ts/packages/ai/src/providers/
- **Extension 系统动态加载** — 当前仅有 5 hook trait (156 行)，缺少动态模块加载、UI context API、tool/command 注册。参考：legacy-ts/packages/coding-agent/src/core/extensions/
- **LSP 集成** — 无 LSP client 实现（只有 /lsp 模式开关）。TS 支持 9 种语言服务器 + diagnostics/definition/references/hover。参考：legacy-ts/packages/coding-agent/src/core/lsp/
- **HTML 导出** — /export 目前仅输出 JSONL。TS 有 standalone HTML 含 CSS/JS/ANSI 渲染/tool 折叠。参考：legacy-ts/packages/coding-agent/src/core/export-html/
- **Image Generation API** — 有 image model metadata registry 但无实际生成 API client。参考：legacy-ts/packages/ai/src/images.ts + providers/images/
- **RPC mode** — stdin/stdout JSONL 协议 (25+ 命令)，用于 IDE 扩展和程序化 API 集成。参考：legacy-ts/packages/coding-agent/src/modes/rpc/
