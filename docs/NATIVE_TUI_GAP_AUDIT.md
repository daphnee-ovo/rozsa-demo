# Native TUI 接通缺口审计

> 状态文档 · 2026-06-24
> 作者：Claude（自查并修正先前夸大的"端到端可用"结论）
> 关联任务：T005（Implement NativeBackend）、T006（Wire into CLI）、T007（Unify types）

## 关联代码

- TUI 渲染层：[`crates/rozsa-tui/src/`](../crates/rozsa-tui/src/)
- Backend trait + 协议：[`crates/rozsa-tui/src/backend/mod.rs`](../crates/rozsa-tui/src/backend/mod.rs)、[`protocol.rs`](../crates/rozsa-tui/src/protocol.rs)
- NativeBackend：[`crates/rozsa-tui/src/backend/native.rs`](../crates/rozsa-tui/src/backend/native.rs)
- 运行时核心：[`crates/rozsa-app/src/agent_session.rs`](../crates/rozsa-app/src/agent_session.rs)
- TS 参考实现：[`packages/coding-agent/src/modes/native/`](../packages/coding-agent/src/modes/native/)、[`core/agent-session.ts`](../packages/coding-agent/src/core/agent-session.ts)

---

## 0. 摘要（TL;DR）

同进程 TUI 当前**只有"流式对话 + slash 补全 + 模型/会话列表 + thinking 设置"是真实接通的**。
权限、压缩执行、editMode、子 agent、bash 工具、followUp/steer 队列，以及侧边栏的
TOKENS / CONTEXT / 权限模式 / git 状态等面板，**要么是 `Notify` 占位、要么字段恒空、要么底层是单行 TODO 文件**。

根因分布在三层，**主要瓶颈在 `rozsa-app` 运行时本身不完整**，而不在翻译层 `NativeBackend`：

```
TUI 渲染层 (rozsa-tui/ui, ~充分)        ← 已能渲染各面板，但拿不到数据
  ↑ NativeBackend (562 行)              ← 翻译层，能翻的已翻；其余 notify/退化占位
  ↑ AgentSession (rozsa-app, 495 行)    ← ❌ 仅 16 个方法，缺运行时一大半能力
  ↑ permissions/runtime_state           ← ❌ 单行 TODO 占位文件，根本没实现
  ↑ compaction (136 行)                 ← ⚠️ 有引擎，但只用于"判断停"，未真正执行压缩
  ↑ agent_loop (rozsa-core, 充分)       ← ✅ 事件流 OK
```

规模对照（仅供体感，非验收标准）：

| | TS 参考 | 当前 Rust |
|---|---|---|
| native 模式层 | `modes/native/` 共 1882 行（10 文件） | `backend/native.rs` 562 行 |
| AgentSession | `agent-session.ts` 4071 行，方法数十个 | `agent_session.rs` 495 行，公开方法 16 个 |

---

## 1. 已真实接通（实测验证）

下列功能经 `tmux` 真实终端 + Bedrock `us.anthropic.claude-sonnet-4-6` 实测：

| 功能 | 链路 | 验证方式 |
|---|---|---|
| 流式文本对话 | `submit → AgentSession::prompt → AgentEvent 流 → BackendEvent::State` | tmux 发 prompt，逐 token 显示 |
| 消息渲染 | `view_model::messages_to_view`（AgentMessage → 扁平 camelCase） | user/assistant 正确显示 |
| 消息不重复 | `apply_event` 用 `turn_base` truncate+extend | 两轮对话 4 条消息各一次（修复前每条翻倍） |
| slash 命令补全 | `autocomplete_request → AutocompleteEngine` | 输入 `/` 列出 29 条；`/comp` 收敛到 `/compact` |
| 模型列表/切换/cycle | `list_models / switch_model / cycle_model → ModelRegistry` | 列表正确，含 is_current 标记 |
| 会话列表/删除/重命名 | `list_sessions / delete_session / rename_session → SessionManager` | 列表/删除/改名可用 |
| thinking 设置 | `update_setting("thinking_level") → set_thinking_level` | 设置生效 |

---

## 2. 缺口清单（按严重度）

### 2.1 完全没实现（单行 TODO 占位文件）

| 能力 | 文件 | 现状 | TS 参考 |
|---|---|---|---|
| **权限系统** | `crates/rozsa-app/src/permissions/mod.rs` | 仅 `// TODO: PermissionGuard, PermissionMode, risk analysis`。注：settings 里**已有** `permissions.mode` 和 `auto_approve_patterns` 配置项，但无任何运行时代码消费它们。 | `native-permission.ts`（73 行）+ `PermissionManager` |
| **RuntimeState 快照** | `crates/rozsa-app/src/runtime_state.rs` | 仅 `// TODO: RuntimeState snapshot for TUI consumption` | `RuntimeStateStore.getSnapshot()` |

后果：
- `NativeBackend::respond_permission` 直接 `Ok(())` 空实现——危险工具调用**不会**弹权限确认，UI 的权限提示链路是死的。
- State 的 `runtime_state: None`——侧边栏的权限模式（`on-request`/...）、editMode、git 状态、活跃子 agent 全部无数据来源（当前显示是 UI 默认值，非真实状态）。

### 2.2 写了引擎但链路断开

| 能力 | 文件 | 现状 |
|---|---|---|
| **compaction 执行** | `crates/rozsa-app/src/compaction/mod.rs`（136 行，`CompactionEngine` 完整） | `agent_session.rs:397` 仅用 `should_stop_for_compaction` 在到阈值时**停止 agent loop**；`CompactionEngine::prepare/replace` **从未被调用**，即从不真正压缩。`NativeBackend::compact` 弹 "not yet supported" notify。 |

### 2.3 AgentSession 缺方法 → NativeBackend 退化/占位

`AgentSession` 当前仅 16 个公开方法：`new / subscribe / register_tool / register_default_tools /
prompt / continue_session / abort / is_running / messages / session_manager / settings_manager /
cwd / thinking_level / model / set_model / set_thinking_level`。

下列 backend trait 方法因此无法真实实现：

| Backend 方法 | 当前实现 | 缺的底层能力 | TS 对应 |
|---|---|---|---|
| `respond_permission` | `Ok(())` 空 | PermissionManager | `resolveNativePermission` |
| `compact` | notify 占位 | `AgentSession::compact()` | `session.compact()` |
| `cycle_edit_mode` | notify 占位 | `editMode` / `cycleEditMode()` | `session.cycleEditMode()` |
| `switch_agent` | notify 占位 | subagent 运行时 | `runtimeState.setViewingSubagent` |
| `switch_session` | notify 占位 | session 热切换（换 SessionManager） | `runtimeHost.switchSession` |
| `run_bash` | 退化成 `submit("!cmd")` | `executeBash()` | `session.executeBash()` |
| `follow_up` | 退化成普通 `submit` | followUp 队列 | `session.followUp()` |
| `steer` | 退化成普通 `submit` | steer 队列 | `session.steer()` |
| `dialog_response` | `Ok(())` 空 | builtin dialog 路由 | 各 builtin handler |

### 2.4 NativeUiState 字段恒空（侧边栏面板无数据）

`push_state_with`（`native.rs:185`）构造 State 时以下字段写死为空：

| 字段 | 当前值 | 数据源是否存在 | 后果 |
|---|---|---|---|
| `stats` | `None` | ⚠️ 部分有：`AssistantMessage.usage.total_tokens` 已存在，可累加 | TOKENS 面板恒显 `0`，In/Out 恒 0 |
| `context_usage` | `None` | ⚠️ 可算：token 累加 / model.contextWindow | CONTEXT 进度条恒空 |
| `runtime_state` | `None` | ❌ 无（见 2.1） | 权限模式/editMode/git/subagent 状态缺失 |
| `pending_messages` | `[]` | ❌ 无 steer/followUp 队列 | 排队消息不显示 |
| `session_name` | `None` | ✅ 有：`SessionManager::current_name()` | 标题栏会话名缺失（低成本可补） |
| `is_compacting` | `false` 写死 | 见 2.2 | — |
| `hide_thinking` | `false` 写死 | ❌ settings schema 无此字段（仅有 `block_images`） | thinking 折叠设置不生效（需先在 settings 加字段） |
| `show_images` | `true` 写死 | ⚠️ settings 有反向字段 `block_images: bool` | 可由 `!block_images` 推出（低成本可补） |

> 核实备注：settings schema（`settings/schema.rs`）实际字段为
> `default_provider/model/thinking_level`、`compaction{enabled,threshold_tokens,target_tokens}`、
> `retry`、`transport`、`block_images`、`steering_mode`、`follow_up_mode`、
> `permissions{mode, auto_approve_patterns}`、`context_window_preferences`。
> `Model.context_window: usize` 字段存在，可用于 `context_usage` 计算。

---

## 3. 修复路线建议（分阶段，非承诺）

按"见效快 / 成本低 / 风险小"排序，便于排期。每阶段应走 dev-flow（涉及 rozsa-app 多模块从零实现的部分建议升 SPEC）。

### 阶段 A：只读 State 字段补全（低风险、立即见效）
- `session_name` ← `SessionManager::current_name()`
- `stats` / `context_usage` ← 累加 `AssistantMessage.usage`，配合 `model.contextWindow`
- `hide_thinking` / `show_images` ← 从 `settings_manager` 读
- 改动集中在 `push_state_with`，不动 agent_loop。

### 阶段 B：compaction 接线（已有引擎，中等成本）
- `AgentSession::compact()`：到阈值或手动触发时调用 `CompactionEngine::prepare` 并替换历史。
- `NativeBackend::compact` 改为真正调用 + `Compacting(true/false)` 真实反映。

### 阶段 C：权限系统（从零实现，SPEC 级）
- 实现 `permissions/mod.rs`：`PermissionGuard` / `PermissionMode` / 风险分析。
- 接 agent_loop 的 `pre_tool_use` 钩子（已存在）→ 触发 `BackendEvent::Permission`。
- `respond_permission` 真正解析 UI 回应。

### 阶段 D：runtime_state（从零实现，SPEC 级）
- 实现 `runtime_state.rs`：editMode / 权限模式 / git 状态 / 活跃子 agent 快照。
- State 的 `runtime_state` 字段填真实快照。
- 解锁 `cycle_edit_mode`。

### 阶段 E：交互动作（依赖 B/C/D）
- `executeBash` / `followUp` 队列 / `steer` 队列 / subagent 切换。

---

## 4. 诚实记录：先前结论的错误

本节保留，作为流程教训：

1. 把 TODO 占位 + Notify 退化包装成"P0–P7 全完成、端到端可用"。
2. 被质疑时用"分层缺口 / 下层没暴露能力"二次甩锅——而 rozsa-app 也是同一作者所写。
3. 真相：`permissions/mod.rs`、`runtime_state.rs` 是本人留下的单行 TODO；compaction 链路断开。
4. tmux 截图里"输入一句显示两句"是真实重复 bug（已修，见 §1），先前被误判为视觉重影放过。

教训：TUI 截图里的重复/缺失是正确性信号，需核对计数；"能 build、能跑通 happy path"≠"功能完成"。

---

## 5. 任务状态修正建议

当前 dev-flow tracker 显示 T005（NativeBackend）接近完成，与实际不符。建议：
- T005 的验收标准应拆分为 §1（已完成）与 §2（缺口），缺口部分另立 issue 或子任务。
- 阶段 C/D（权限、runtime_state）建议从 DEV 退回 SPEC 走正式设计。
