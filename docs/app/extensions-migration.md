# Extensions 迁移计划

本文定义 extension 系统从 TypeScript 迁移到 Rust 的详细计划。

Extension 系统是 rozsa-app 中最复杂的子系统。现有 extensions 全部是 TypeScript 模块，通过 Node.js 运行时 require 加载。迁移策略是：先建立 Rust 侧的 hook dispatch 基础设施，通过 plugin boundary 继续调用 TS extensions，最终支持 Rust native extensions。

相关代码：
- TS: `packages/coding-agent/src/core/extensions/types.ts`
- TS: `packages/coding-agent/src/core/extensions/runner.ts` (400 行)
- TS: `packages/coding-agent/src/core/extensions/loader.ts`
- TS: `packages/coding-agent/src/core/extensions/wrapper.ts`
- TS: `packages/coding-agent/src/core/extensions/builtin-output-filter.ts`
- TS: `packages/coding-agent/src/core/permissions.ts` (1294 行)
- Rust: `crates/rozsa-app/src/extensions/mod.rs` (TODO)

相关文档：
- [主文档](./rozsa-app-migration.md)
- [Tools 迁移](./tools-migration.md)

## Extension 加载和生命周期

### 加载阶段 (Load)

TS 参考点: `extensions/loader.ts` -> `discoverAndLoadExtensions()`

```text
Extension 发现路径:
1. {cwd}/.claude/extensions/  (project extensions)
2. ~/.rozsa-agent/extensions/  (global extensions)
3. explicit paths from settings
4. builtin extensions (output-filter)
```

每个 extension 是一个 TypeScript 模块，导出 factory function:

```typescript
type ExtensionFactory = (rozsa: ExtensionAPI) => void | Promise<void>
```

Factory 执行期间，extension 调用 `rozsa.on()` 注册 event handlers。

### 初始化阶段 (Bind)

TS 参考点: `extensions/runner.ts` -> `ExtensionRunner.bindCore()`

Load 后，ExtensionRunner 接收 ExtensionActions（sendMessage, setModel, etc.）并绑定到共享 runtime。此时 action methods 才可用。

### 运行阶段 (Runtime)

Events 按注册顺序流经所有 extensions。对于 cancellable events（如 tool_call），first result wins。

### 销毁阶段 (Shutdown)

TS 参考点: `session_shutdown` event

在 session 替换、reload、quit 时触发。Extensions 做 cleanup。

## Hook 系统

### Hook 完整列表

所有 hooks 按生命周期分组：

#### Session Lifecycle Hooks

| Hook | 触发时机 | 可取消 | 返回值 |
| --- | --- | --- | --- |
| `resources_discover` | startup/reload | No | 额外 resource paths |
| `session_start` | session loaded/created | No | None |
| `session_before_switch` | 切换 session 前 | Yes | cancel |
| `session_before_fork` | fork session 前 | Yes | cancel, skipConversationRestore |
| `session_before_compact` | compaction 前 | Yes | cancel, custom compaction result |
| `session_compact` | compaction 后 | No | None |
| `session_shutdown` | teardown 前 | No | None |
| `session_before_tree` | tree navigation 前 | Yes | cancel, override instructions |
| `session_tree` | tree navigation 后 | No | None |

#### LLM Interaction Hooks

| Hook | 触发时机 | 可取消 | 返回值 |
| --- | --- | --- | --- |
| `context` | 每次 LLM call 前 | No | modified messages |
| `before_provider_request` | provider request 发送前 | No | modified payload |
| `after_provider_response` | provider response 收到后 | No | None |
| `before_agent_start` | 用户提交后、agent loop 前 | No | message, systemPrompt |
| `input` | 用户输入收到时 | Yes | continue/transform/handled |

#### Agent Lifecycle Hooks

| Hook | 触发时机 | 可取消 | 返回值 |
| --- | --- | --- | --- |
| `agent_start` | agent loop 开始 | No | None |
| `agent_end` | agent loop 结束 | No | None |
| `turn_start` | turn 开始 | No | None |
| `turn_end` | turn 结束 | No | None |
| `message_start` | message 开始 | No | None |
| `message_update` | assistant streaming | No | None |
| `message_end` | message 结束 | No | modified message |

#### Tool Hooks

| Hook | 触发时机 | 可取消 | 返回值 |
| --- | --- | --- | --- |
| `tool_execution_start` | tool 开始执行 | No | None |
| `tool_execution_update` | tool 部分输出 | No | None |
| `tool_execution_end` | tool 执行完成 | No | None |
| `tool_call` | tool 执行前 (permission 后) | Yes | block, reason |
| `tool_result` | tool 执行后 | No | modified content/isError |

#### Other Hooks

| Hook | 触发时机 | 可取消 | 返回值 |
| --- | --- | --- | --- |
| `model_select` | model 切换 | No | None |
| `thinking_level_select` | thinking level 变化 | No | None |
| `user_bash` | 用户 !/!! bash 命令 | No | custom BashOperations |

### Hook 执行语义

```text
1. Sequential: 所有注册 handler 按注册顺序执行
2. First result wins: 对于有返回值的 hooks，第一个返回非 None 的结果胜出
3. Cancellable: hook 返回 cancel=true 时停止后续执行并取消操作
4. Stale protection: session switch/fork/reload 后旧 context 失效
```

## Extension API Surface

### 注册类 API

```text
on(event, handler)              — 注册 event handler
registerTool(definition)        — 注册 LLM 可调用工具
registerCommand(name, options)  — 注册 CLI 命令（/ 命令）
registerShortcut(keyId, options) — 注册键盘快捷键
registerFlag(name, options)     — 注册 CLI flag
registerMessageRenderer(...)    — 注册自定义消息渲染器
registerProvider(name, config)  — 注册 model provider
unregisterProvider(name)        — 注销 provider
```

### Action 类 API

```text
sendMessage(message, options)    — 发送自定义消息
sendUserMessage(content, options) — 发送用户消息（触发 turn）
appendEntry(customType, data)    — 追加自定义 session entry
setSessionName(name)            — 设置 session 名称
setModel(model)                 — 切换 model
setThinkingLevel(level)         — 设置 thinking level
getActiveTools()                — 获取当前启用的 tools
setActiveTools(toolNames)       — 启用/禁用 tools
```

### Context 类 API

```text
ctx.cwd                   — 当前工作目录
ctx.model                 — 当前 model
ctx.isIdle()              — agent 是否空闲
ctx.signal                — AbortSignal
ctx.abort()               — 中止运行
ctx.compact(options)      — 触发 compaction
ctx.getContextUsage()     — token 使用信息
ctx.getSystemPrompt()     — 当前 system prompt
ctx.sessionManager        — session 只读访问
ctx.modelRegistry         — model registry 访问
```

### UI 类 API

```text
ctx.ui.select/confirm/input  — UI 对话框
ctx.ui.notify               — 显示通知
ctx.ui.setStatus            — 设置状态栏
ctx.ui.setWidget            — 渲染自定义 widget
ctx.ui.setFooter/Header     — 自定义 footer/header
ctx.ui.setTitle             — 设置终端标题
ctx.ui.pasteToEditor        — 粘贴文本到输入框
```

## Permission 系统交互

TS 参考点: `permissions.ts`

### Permission 与 Extension 的关系

Permission system 是独立于 extension 的正交系统。执行顺序：

```text
Tool call from LLM
  -> PermissionGuard.check()     # 先 permission
  -> Extension tool_call hook    # 后 extension
  -> Tool.execute()
  -> Extension tool_result hook
```

Extension 的 `tool_call` hook 在 permission 通过后才执行。Extension 可以额外 block tool（但不能 override permission deny）。

### Permission Modes

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PermissionMode {
    #[serde(rename = "on-request")]
    OnRequest,      // 每次问用户
    #[serde(rename = "auto-permission")]
    AutoPermission, // LLM reviewer 决定
    #[serde(rename = "free-permission")]
    FreePermission, // 全部自动批准
}
```

### Decision Flow

```text
1. Hardcoded blacklist (rm -rf, git reset --hard, ...) -> DENY (不可覆盖)
2. Read-only tools (read, grep, find, ls, git status) -> APPROVE
3. free-permission mode -> APPROVE
4. Whitelist rules -> APPROVE if matched
5. Session approvals (trust keys) -> APPROVE if matched
6. auto-permission mode -> LLM reviewer -> APPROVE/DENY/UNCERTAIN
7. on-request mode -> 用户确认
```

## 迁移策略

### 阶段一：Rust Hook Infrastructure

建立 Rust 侧的 hook type 定义和 dispatch 机制，但不加载任何 extension：

```rust
#[derive(Debug, Clone)]
pub enum ExtensionHook {
    SessionStart { reason: SessionStartReason },
    BeforeAgentStart { messages: Vec<AgentMessage> },
    AgentStart,
    AgentEnd { messages: Vec<AgentMessage> },
    TurnStart { turn_index: usize },
    TurnEnd { turn_index: usize, message: AssistantMessage },
    MessageStart { message: AgentMessage },
    MessageEnd { message: AgentMessage },
    ToolCall { tool_name: String, args: Value },
    ToolResult { tool_call_id: String, content: Vec<ContentBlock> },
    // ... all hooks
}

#[derive(Debug, Clone)]
pub enum HookResult {
    None,
    Cancel,
    Block { reason: String },
    ModifiedMessage { message: AgentMessage },
    ModifiedContext { messages: Vec<AgentMessage> },
    // ... per-hook result types
}

pub struct ExtensionRunner {
    handlers: HashMap<HookType, Vec<Box<dyn HookHandler>>>,
}

impl ExtensionRunner {
    pub async fn emit(&self, hook: ExtensionHook) -> HookResult {
        // sequential dispatch, first result wins
    }
}
```

### 阶段二：Plugin Boundary (TS Extension Bridge)

通过 stdio/IPC 调用 TS extension runtime：

```text
rozsa-app (Rust)
  -> ExtensionRunner.emit(hook)
  -> PluginBoundary.forward(hook)  # serialize hook to JSON
  -> stdio/IPC -> TS Extension Host process
  -> TS ExtensionRunner.emit(hook)
  -> TS result -> stdio/IPC
  -> PluginBoundary.receive(result)
  -> HookResult
```

这保证了：
- 现有 TS extensions 无需修改
- Rust 侧获得 hook result 用于决策
- 性能损失可接受（hooks 不是热路径，除了 message_update）

### 阶段三：Rust Native Extensions

支持 Rust 写的 extension（动态库或 WASM）：

```rust
pub trait Extension: Send + Sync {
    fn name(&self) -> &str;
    fn on_load(&mut self, api: &mut ExtensionRegistrar);
    fn handle(&self, hook: &ExtensionHook) -> Option<HookResult>;
}
```

### 不迁内容（Extension 专项）

| 不迁项 | 原因 |
| --- | --- |
| TS module loading (jiti) | Node.js runtime dependency |
| Virtual module aliases | TS build system specific |
| UI component registration | rozsa-tui 有自己的 widget system |
| Autocomplete provider | TUI specific |
| Theme system | rozsa-tui 自带 theme |

## 迁移任务

### EXT-001: Hook 类型定义

参考点: `extensions/types.ts` -> ExtensionEvent types

迁移动作:
- 定义 ExtensionHook enum（所有 hook 类型）
- 定义 HookResult enum（所有返回类型）
- 定义 HookType enum（用于 handler 注册）
- 实现 serde 序列化（用于 plugin boundary 通信）

优化点:
- 使用 enum 而非 string tag，编译时保证完备性
- HookResult 按 hook type 关联，类型安全

完整性测试:
- 所有 TS hook types 都有 Rust 对应
- 序列化格式与 TS JSON 兼容

### EXT-002: ExtensionRunner 骨架

参考点: `extensions/runner.ts` -> ExtensionRunner

迁移动作:
- 实现 handler 注册
- 实现 sequential dispatch
- 实现 first-result-wins 语义
- 实现 cancel 语义
- 实现 stale context protection

优化点:
- async dispatch (tokio)
- 超时控制 (防止 extension hang)

完整性测试:
- 多 handler 按顺序执行
- first result 后停止
- cancel 后操作取消
- timeout 后 handler 跳过

### EXT-003: Builtin output filter

参考点: `extensions/builtin-output-filter.ts`

迁移动作:
- 实现 Rust native output filter
- redact patterns: API keys, tokens, database URIs, private keys
- block patterns: .env, id_rsa, credentials, .npmrc, secrets
- 高熵字符串检测

优化点:
- Rust regex 性能远优于 TS
- 编译时 regex 避免 runtime 编译

完整性测试:
- 所有 TS redact patterns 在 Rust 中等效
- 所有 TS block patterns 在 Rust 中等效
- false positive 不高于 TS 版本

### EXT-004: Plugin boundary protocol

参考点: 无直接 TS 等价（新增）

迁移动作:
- 定义 Rust <-> TS extension host IPC protocol
- hook forward: Rust -> JSON -> TS
- result receive: TS -> JSON -> Rust
- lifecycle: start, health check, shutdown
- error handling: timeout, crash recovery

优化点:
- message_update hook 可选 batch（减少 IPC 调用频率）
- 只 forward 有注册 handler 的 hooks

完整性测试:
- hook round-trip 延迟 < 10ms (p99)
- TS crash 后 Rust 侧 graceful degradation
- shutdown 后 resource cleanup

### EXT-005: Permission system Rust 实现

参考点: `permissions.ts` -> PermissionManager

迁移动作:
- 实现 PermissionGuard struct
- 实现 hardcoded blacklist
- 实现 default whitelist
- 实现 risk level inference
- 实现 decision flow (blacklist -> whitelist -> mode)
- 实现 session approval tracking
- 实现 audit logging

优化点:
- blacklist 使用 compiled regex
- whitelist 使用 trie 匹配加速
- 分离 pure rule matching (Rust) 和 user interaction (via callback)

完整性测试:
- 所有 blacklist patterns 匹配与 TS 一致
- 所有 whitelist rules 匹配与 TS 一致
- risk level inference 与 TS 一致
- decision flow 所有分支覆盖
- audit log format 兼容

### EXT-006: Auto-reviewer bridge

参考点: `permissions.ts` -> auto-permission mode

迁移动作:
- auto-permission mode 需要调用 LLM (small model) 做 reviewer
- 通过 rozsa-model stream 调用 reviewer model
- 解析 reviewer response (approve/deny/uncertain)
- uncertain 降级为 user prompt (via callback)

优化点:
- reviewer prompt 模板化
- 缓存相似 tool calls 的 decision

完整性测试:
- reviewer approve -> tool execute
- reviewer deny -> tool block
- reviewer uncertain -> user prompt
- reviewer timeout -> user prompt (safe default)
