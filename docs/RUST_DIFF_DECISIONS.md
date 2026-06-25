# Native Backend: TS → Rust 迁移差异决策

本文档记录 Rust ROZSA 实现与 TS 版 的行为差异，
说明每处差异的动机。这不是遗漏，而是迁移过程中的设计改进。

## 1. Settings 持久化范围

**TS 行为：** 部分 settings 修改（如 thinking level）只影响当前 session 运行时，
不写入 `settings.json`。重启后丢失。

**Rust 行为：** 所有通过 `/thinking`、`/model`、settings dialog、`Ctrl+T` 修改的
设置都持久化到 `~/.rozsa/agent/settings.json`。下次启动自动恢复。

**动机：** 用户修改设置的意图通常是持久的。TS 的非持久化行为导致用户反复设置，
是 UX 痛点而非设计意图。

### 持久化的设置项
| 操作 | 持久化字段 |
|---|---|
| `/thinking <level>` | `defaultThinkingLevel` |
| `/model <id>` | `defaultProvider`, `defaultModel` |
| Ctrl+T (toggle thinking display) | `hideThinking` |
| Settings dialog (all items) | 对应字段 |

## 2. `hideThinking` 新增字段

**TS 行为：** `Ctrl+T` 只切换本地 UI 状态 (`thinking_visible`)，不持久化。
重启后 thinking 显示恢复默认（visible）。

**Rust 行为：** 新增 `hideThinking: bool` 字段到 `settings.json`。
`Ctrl+T` 切换后立即持久化。下次启动读取此值决定初始 thinking 显示状态。

**语义区分：**
- `defaultThinkingLevel: "off"` → 模型不产生 thinking blocks（API 级别）
- `hideThinking: true` → 模型仍然 thinking，但 UI 不显示内容（纯展示级别）

## 3. Session 文件 Lazy 创建

**TS 行为：** 启动时立即创建 session JSONL 文件（即使用户不发任何消息）。
导致大量空 session 文件积累。

**Rust 行为：** `SessionManager::create_lazy()` — 只在第一条消息写入时
才创建文件。打开 TUI 后直接退出不产生垃圾文件。

**动机：** 减少文件系统污染，`/resume` 列表更干净。

## 4. `!` Bang Command 流式渲染

**TS 行为：** `!command` 通过 `session.executeBash()` 执行，结果作为
`bashExecution` 消息加入对话。输出是一次性返回。

**Rust 行为：** 同样生成 `bashExecution` 消息，但输出是**逐行流式**渲染。
用户可以实时看到长命令的输出，不需要等命令完成。

**动机：** 提升交互体验，尤其对 `!tail -f`、`!cargo build` 等长运行命令。

## 5. Autocomplete Staleness 保护

**TS 行为：** autocomplete 响应按到达顺序直接应用，无 staleness 检查。
快速输入时偶尔出现旧响应覆盖新响应（低概率，因为 TS 单线程）。

**Rust 行为：** 每个 autocomplete 响应携带 `id`（单调递增）和 `prefix`。
TUI 接收时验证 `prefix == current_input`，不匹配则丢弃。
防止异步 spawn 导致的乱序响应覆盖。

**动机：** Rust 异步 spawn 使乱序概率远高于 TS 单线程环境，必须显式处理。

## 6. CLI 启动 Settings 读取

**TS 行为：** CLI 通过 settings.json 读取 model/provider 配置，
但 thinking level 等运行时参数在每次启动时重置为默认。

**Rust 行为：** 所有 settings（包括 `defaultThinkingLevel`）在启动时从
`settings.json` 读取并应用。不再硬编码任何默认值。

**动机：** 一致性 — 用户设的 thinking level 应该跨 session 保持。

---

## 7. Token 估算改进

**TS 行为：** 无本地 tokenizer，compaction 阈值判断没有明确的估算策略
（多数情况依赖 provider 返回的真实 token 计数）。

**Rust 行为：** 使用双轨估算：ASCII 字符 ÷ 4，非 ASCII 字符 × 2 ÷ 3。
相比单一 `chars/4`，对 CJK 混合内容的偏差从 4 倍缩小到约 1.5 倍。

**动机：** 纯 `chars/4` 对中文内容严重低估 token 数，导致 compaction
在 context window 已满时仍不触发。

---

## 8. 未实现 Provider 友好报错

**TS 行为：** 未实现的 provider 在 TS 版中不会出现在选择列表里
（models.json 不包含无实现的 provider）。

**Rust 行为：** `ModelRegistry` 可能从 models.json 加载了
Google/Mistral/OpenAI-Responses 等模型条目。选中后不再 panic，
而是返回 `StreamEvent::Error` 附带 "Provider not yet implemented" 消息，
agent loop 正常处理错误。

**动机：** panic 导致终端状态损坏，是比功能缺失更严重的 UX 问题。

---

## 9. Compaction 安全切割与图片剥离

**TS 行为：** compaction 按 token 比例切割，无 tool 配对保护。
切割点可能落在 tool_use（assistant）和 tool_result 之间。

**Rust 行为：**
- 切割点自动调整到 tool_use/tool_result 配对边界外
  （`adjust_to_safe_boundary`），保证 tail 不以孤立 ToolResult 开头。
- 摘要序列化时剥离 Image block 中的 base64 数据，
  替换为 `[image data omitted]`，避免图片灌入摘要 prompt。
- compaction 触发使用最新一轮真实 `usage.input`（API 返回），
  而非历史 total_tokens 累加。

**动机：**
- Anthropic/Bedrock API 要求 ToolResult 必须紧跟含 ToolCall 的 Assistant 消息，
  断开后 resume 会报 400 错误。
- 图片 base64 可能占数万 token，灌入摘要 prompt 浪费预算且无意义。
- 累加 total_tokens 会随对话增长远超实际 context size，导致过早触发。

---

## 相关代码

- Settings schema: `crates/rozsa-app/src/settings/schema.rs`
- NativeBackend: `crates/rozsa-tui/src/backend/native.rs`
- CLI entry: `crates/rozsa-cli/src/run.rs`
- Session manager: `crates/rozsa-app/src/session/manager.rs`
- Stream entry: `crates/rozsa-model/src/stream.rs`
- Compaction: `crates/rozsa-app/src/compaction/mod.rs`
