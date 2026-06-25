# 未实装功能审计报告

> 生成时间: 2026-06-25
> 最后修复: 2026-06-25（Phase 1+2 完成）
> 扫描范围: crates/ 下所有 Rust 代码（5 crates, ~1043 函数）

## 概览

| 严重程度 | 数量 | 含义 |
|----------|------|------|
| 🔴 空壳（定义无实现） | 5 | 只有类型/trait 定义，完全没有业务逻辑 |
| 🟡 未对接（有实现但没接入） | 5 | 代码写好了但没有被使用方调用 |
| 🟠 部分实装（明确标注 not yet） | 6 | 有框架但部分方法是 stub |
| 🔵 Provider 缺失 | 4 | types.rs 中定义了 API 枚举但无 provider 实现 |
| ⚪ 预留占位/死字段 | 4 | 文件级 placeholder 或始终为默认值的字段 |

---

## 🔴 空壳（定义无实现）

### 1. `rozsa-core::session::SessionStore` trait — 无实现者

**文件**: `crates/rozsa-core/src/session.rs`

定义了 `SessionStore` trait（save/load/list/delete），但整个项目没有任何 `impl SessionStore for ...`。`rozsa-app` 的 `SessionManager` 自行处理持久化，完全没有走这个 trait。

**影响**: trait 是死代码，或者说 session 持久化的抽象层完全未生效。

### 2. `rozsa-core::agent::Agent` struct — 无方法

**文件**: `crates/rozsa-core/src/agent.rs`

定义了 `Agent` 和 `AgentState` 两个 struct（含 model, tools, messages, queues），但整个 crate 没有 `impl Agent` 块。没有构造函数、没有 run 方法、没有状态机。

**影响**: 这是一个纯数据声明，实际 agent 执行逻辑在 `agent_loop.rs` 中通过 `AgentContext` + 函数式风格实现，Agent struct 未被使用。

### 3. `rozsa-app::extensions::ExtensionRunner` — 框架完整但零使用

**文件**: `crates/rozsa-app/src/extensions/mod.rs`

`Extension` trait + `ExtensionRunner` 完整实现了 6 个生命周期 hook（session_start, before_provider_request, after_provider_response, tool_call, tool_result, context），但：
- 没有任何 `impl Extension for ...`
- `ExtensionRunner` 没有被任何外部代码引用
- agent_session.rs 不持有 ExtensionRunner

**影响**: 扩展系统定义完备但完全未接入执行链。

### 4. `rozsa-app::permissions::PermissionGuard` — 未被调用

**文件**: `crates/rozsa-app/src/permissions/mod.rs`

实现了 `PermissionGuard`（含 `evaluate()`, `record_session_approval()`, `mode()`），策略评估逻辑完整（支持 auto_approve_patterns），但：
- 没有被 agent_session、agent_loop 或 native backend 调用
- TUI 的 `respond_permission` 是空壳（直接 `Ok(())`）

**影响**: 权限系统是死代码，tool 调用目前完全无守卫。

### 5. `rozsa-app::skills::SkillRegistry` — 未被调用

**文件**: `crates/rozsa-app/src/skills/mod.rs`

实现了 `SkillRegistry`（match_input, find_by_name, build_system_prompt_fragment），但没有被 TUI/CLI/agent_session 任何地方引用。

**影响**: Skill 匹配和注入系统写好了但完全未对接。

---

## 🟡 未对接（有实现但没接入）

### 1. `rozsa-app::compaction` — 实现完整，NativeBackend 未对接

**文件**: `crates/rozsa-app/src/compaction/mod.rs`, `crates/rozsa-app/src/agent_session.rs:379`

`CompactionEngine` 和 `agent_session.compact()` 都是完整实现（prepare plan → summarize → replace），但 NativeBackend 的 `compact()` 方法直接返回 "not yet supported" 通知。

**影响**: `/compact` 命令在 native mode 下无效（Ctrl+O 热键不工作）。

### 2. `rozsa-core::protocol` (Bridge) — 有三套协议未统一

**文件**: `crates/rozsa-core/src/protocol.rs` (core bridge), `crates/rozsa-model/src/protocol.rs` (model bridge), `crates/rozsa-app/src/main.rs` (app bridge)

三个 crate 各自定义了自己的 Bridge 协议（`BridgeInput`/`BridgeOutput`），但三者之间没有复用或统一的 trait，各自独立运行 stdin/stdout loop。

**影响**: 非 bug，但三个独立协议意味着协议演进成本 3x。

### 3. Slash Commands — 定义 29 个，大量无 handler

**文件**: `crates/rozsa-app/src/slash_commands.rs`

定义了 29 个 builtin slash commands（纯元数据），**没有 dispatch 层**。NativeBackend 中实现了部分命令（/model, /session, /settings, /new, /resume, /quit 等），但以下命令无执行逻辑：
- `/export` — 未找到导出实现
- `/import` — 未找到导入实现
- `/share` — 未找到 GitHub gist 逻辑
- `/copy` — 未找到剪贴板操作
- `/fork`, `/clone`, `/tree`, `/graph` — 未找到 session 分支逻辑
- `/login`, `/logout` — OAuth 流程在 model crate 有实现，但 TUI/CLI 未对接
- `/permissions` — 权限系统本身未接入
- `/search` — 未找到 tool output 搜索实现
- `/gc` — 未找到 session 清理实现
- `/lsp` — 未找到 LSP 配置持久化

**影响**: 大量 slash commands 只有自动补全，执行时要么静默失败，要么转发给 agent（不保证正确执行）。

### 4. `rozsa-model::providers::faux` — 空文件

**文件**: `crates/rozsa-model/src/providers/faux.rs`（1 行注释）

标注为 "Placeholder for the future faux provider used by model-layer tests"，但从未实现。

**影响**: 测试用 mock provider 未完成。

### 5. `rozsa-cli` 的 `--print` flag — 声明未使用

**文件**: `crates/rozsa-cli/src/args.rs:14-15`

clap 声明了 `--print` / `-p` flag，但 `run.rs` 中从未检查此字段。

**影响**: 用户可以传 `-p` 但无任何效果。

---

## 🟠 部分实装（NativeBackend 中明确标注 "not yet"）

| 方法 | 文件:行 | 现状 |
|------|---------|------|
| `respond_permission()` | native.rs:1312 | 直接返回 Ok(())，权限响应被忽略 |
| `compact()` | native.rs:1327 | 返回通知文字，不执行压缩 |
| `cycle_edit_mode()` | native.rs:1339 | 返回通知文字 |
| `switch_agent()` | native.rs:1349 | 返回通知文字，不切换子 agent |
| `dialog_response()` | native.rs:1357 | 空实现，所有对话框回调被忽略 |
| `update_setting()` (fallback) | native.rs:1457 | 非 thinking_level/hide_thinking/theme 的 setting 返回 "not yet" |

---

## 🔵 Provider 缺失（types.rs 定义了但无实现）

**文件**: `crates/rozsa-model/src/types.rs` (API enum)

| Provider | 枚举值 | 实现状态 |
|----------|--------|----------|
| Anthropic | `AnthropicMessages` | ✅ 完整实现 |
| AWS Bedrock | `Bedrock` | ✅ 完整实现 |
| OpenAI Completions | `OpenAICompletions` | ✅ 完整实现 |
| Google Generative AI | `GoogleGenerativeAI` | ❌ 无实现文件 |
| Google Vertex | `GoogleVertex` | ❌ 无实现文件 |
| Mistral | `MistralConversations` | ❌ 无实现文件 |
| OpenAI Responses | `OpenAIResponses` | ❌ 无实现文件（新协议） |
| Custom | `Custom(String)` | ❌ 扩展点，无通用实现 |

**影响**: 使用 Google/Mistral/OpenAI Responses API 的模型无法工作。

---

## ⚪ 预留占位 / 死字段

| 项目 | 位置 | 说明 |
|------|------|------|
| `ToolResult.details` | rozsa-core/tool.rs:15 | 始终为 `Value::Null`，从未被赋值 |
| `ToolResultMessage.timestamp` | agent_loop.rs:681 | 始终为 `0` |
| `ToolExecutionUpdate` event | rozsa-core/events.rs:36-41 | 已定义但 agent_loop 从不 emit |
| Token 预估缺失 | rozsa-model 全局 | 无本地 tokenizer，compaction 使用 `chars/4` 近似 |

---

## 接线图：各层实际对接状态

```
rozsa-cli ─────┬──→ rozsa-app::ModelRegistry      ✅ 已对接
               ├──→ rozsa-app::ResourceLoader      ✅ 已对接
               ├──→ rozsa-app::AgentSession        ✅ 已对接
               └──→ rozsa-tui::NativeBackend       ✅ 已对接

rozsa-tui ─────┬──→ rozsa-app::AgentSession        ✅ 已对接
               ├──→ rozsa-app::ModelRegistry       ✅ 已对接
               ├──→ rozsa-app::SlashCommands       ✅ 自动补全已对接
               ├──→ rozsa-app::PermissionGuard     ❌ 未对接
               ├──→ rozsa-app::SkillRegistry       ❌ 未对接
               └──→ rozsa-app::ExtensionRunner     ❌ 未对接

rozsa-app ─────┬──→ rozsa-core::agent_loop         ✅ 已对接（通过 AgentSession）
               ├──→ rozsa-core::Tool trait          ✅ 已对接（7 个 tool 实现）
               ├──→ rozsa-core::SessionStore trait  ❌ 未对接（自行实现持久化）
               ├──→ rozsa-model::stream             ✅ 已对接
               └──→ rozsa-model::registry           ✅ 已对接

rozsa-core ────┬──→ rozsa-model::types              ✅ 已对接
               └──→ rozsa-model::event_stream       ✅ 已对接
```

---

## 建议优先级

| 优先级 | 项目 | 理由 |
|--------|------|------|
| P0 | PermissionGuard 接入 agent loop | 安全性 — 目前所有 tool 调用无守卫 |
| P1 | NativeBackend compact 对接 | 功能完整性 — 实现已存在只需接线 |
| P1 | 清理 Agent struct / SessionStore | 死代码消除 — 避免混淆 |
| P1 | Google/Mistral provider 实现 | 多模型支持 — 枚举已定义，用户选了会 panic |
| P2 | ExtensionRunner 接入执行链 | 可扩展性 — 框架就绪等对接 |
| P2 | SkillRegistry 接入 | 功能完整性 |
| P2 | Slash commands dispatch 层 + 批量实装 | 用户体验 — 29 命令中约 12 个无效 |
| P3 | faux provider 实现 | 测试基础设施 |
| P3 | 移除 `--print` flag 或实现它 | 接口清洁 |
| P3 | ToolResult.details / timestamp 赋值 | 数据完整性 |
| P3 | ToolExecutionUpdate 事件对接 | 工具进度反馈 |

---

## 补充：辅助二进制的用途

项目有 3 个辅助 binary（非主入口 rozsa-cli）：

| Binary | 文件 | 用途 |
|--------|------|------|
| `rozsa-core` bridge | `crates/rozsa-core/src/bin/bridge.rs` | TS→Rust JSONL 桥，供 TypeScript 遗留代码调用 Rust agent loop |
| `rozsa-model` main | `crates/rozsa-model/src/main.rs` | OAuth + model stream 的 JSONL 桥 |
| `rozsa-app` main | `crates/rozsa-app/src/main.rs` | ModelRegistry / ImageModelRegistry 查询桥 |

这三个都是为 TypeScript 迁移期服务的 bridge binary，架构合理但属于过渡期产物。
