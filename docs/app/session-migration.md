# Session 迁移计划

本文定义 session 管理从 TypeScript 迁移到 Rust 的详细计划。

Session 系统包含两个核心组件：
- **AgentSession**: 运行时编排层，组合 Agent + 产品服务
- **SessionManager**: 持久化层，JSONL 文件 + tree navigation + context build

相关代码：
- TS: `packages/coding-agent/src/core/agent-session.ts` (4071 行)
- TS: `packages/coding-agent/src/core/session-manager.ts` (1474 行)
- TS: `packages/coding-agent/src/core/sdk.ts` (session 创建工厂)
- Rust: `crates/rozsa-app/src/session/mod.rs` (TODO)
- Rust: `crates/rozsa-core/src/agent.rs` (Agent state)
- Rust: `crates/rozsa-core/src/agent_loop.rs` (agent loop)

相关文档：
- [主文档](./rozsa-app-migration.md)
- [Settings 迁移](./settings-migration.md)
- [Extensions 迁移](./extensions-migration.md)

## Session Lifecycle

### 1. Session 创建

TS 参考点: `sdk.ts` -> `createAgentSession()`

```text
创建流程:
1. SettingsManager.load(global + project)
2. AuthStorage.load()
3. ModelRegistry.load(generated + models.json)
4. resolve model (options > session restore > settings default > provider default)
5. clamp thinking level to model capabilities
6. ResourceLoader.load(skills, prompts, themes)
7. new Agent(streamFn, convertToLlm, hooks)
8. SessionManager.load(sessionId) or .create()
9. if restore: SessionManager.buildSessionContext() -> agent.state.messages
10. new AgentSession(agent, sessionManager, settingsManager, ...)
11. AgentSession._buildRuntime() -> tool registry + system prompt
12. AgentSession.bindExtensions(uiContext, abortHandler, ...)
```

Rust 目标: `AgentSession::new()` builder pattern

```rust
pub struct AgentSessionBuilder {
    settings: SettingsManager,
    model_registry: ModelRegistry,
    session_id: Option<String>,
    cwd: PathBuf,
    // ...
}

impl AgentSessionBuilder {
    pub async fn build(self) -> Result<AgentSession, AgentSessionError> {
        // 1. resolve model
        // 2. load resources
        // 3. create/restore session
        // 4. build tool registry
        // 5. build system prompt
        // 6. create Agent (rozsa-core)
        // 7. return AgentSession
    }
}
```

### 2. Session Prompt

TS 参考点: `agent-session.ts` -> `AgentSession.prompt()`

```text
Prompt 流程:
1. extension input hook (transform/handle/continue)
2. expand prompt templates if enabled
3. check streaming state (queue via steer/followUp if already streaming)
4. validate model + API key
5. check compaction needed
6. build messages: user message + pending 'nextTurn' custom messages
7. emit 'before_agent_start' extension event
8. agent.prompt(messages) -> agent loop starts
9. subscribe agent events -> _handleAgentEvent
10. after loop ends -> _handlePostAgentRun (retry, compaction)
```

Rust 目标:

```rust
impl AgentSession {
    pub async fn prompt(&mut self, input: PromptInput) -> Result<(), AgentSessionError> {
        // 1. extension input hook (via plugin boundary)
        // 2. validate state
        // 3. compaction check
        // 4. build prompt messages
        // 5. call core agent_loop
        // 6. consume event stream -> handle events
        // 7. post-run hooks (retry, compaction)
    }
}
```

### 3. Event Handling

TS 参考点: `agent-session.ts` -> `_handleAgentEvent()`

每个 AgentEvent 触发：
- `message_end` -> `SessionManager.appendMessage()` (JSONL persist)
- `tool_execution_end` -> RuntimeStateStore update, git status refresh
- `agent_end` -> retry check, compaction check
- 所有事件 -> extension emit

Rust 目标: 事件消费 loop

```rust
async fn consume_events(&mut self, stream: EventStream<AgentEvent>) {
    while let Some(event) = stream.next().await {
        match &event {
            AgentEvent::MessageEnd { message } => {
                self.session_manager.append_message(message).await?;
            }
            AgentEvent::ToolExecutionEnd { .. } => {
                self.runtime_state.update(&event);
            }
            AgentEvent::AgentEnd { .. } => {
                self.check_retry().await?;
                self.check_compaction().await?;
            }
            _ => {}
        }
        self.extension_runner.emit(&event).await;
        self.event_tx.send(event)?; // forward to TUI
    }
}
```

### 4. Session Abort

TS 参考点: `agent-session.ts` -> `abort()`

```text
Abort 流程:
1. signal abort (AbortController)
2. agent loop detects cancellation
3. stream aborted -> stop_reason = Aborted
4. tool execution aborted
5. session state cleanup
```

Rust 目标: `CancellationToken` (已在 rozsa-core 实现)

## JSONL 持久化格式

### Entry 类型

TS 参考点: `session-manager.ts` -> `SessionEntry`

```typescript
type SessionEntry =
  | SessionMessageEntry      // 普通 message (user/assistant/tool_result)
  | SessionCompactionEntry   // compaction summary
  | SessionModelChangeEntry  // model switch record
  | SessionCustomEntry       // extension custom entry
  | SessionLabelEntry        // user label
  | SessionBranchSummary     // branch summary for navigation
```

Rust 目标:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SessionEntry {
    Message(SessionMessageEntry),
    Compaction(SessionCompactionEntry),
    ModelChange(SessionModelChangeEntry),
    Custom(SessionCustomEntry),
    Label(SessionLabelEntry),
    BranchSummary(SessionBranchSummaryEntry),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessageEntry {
    pub id: String,
    pub parent_id: Option<String>,
    pub message: AgentMessage,
    pub timestamp: i64,
}
```

### JSONL 文件格式

每行一个 JSON 对象，按时间顺序追加：

```jsonl
{"type":"header","version":"1.0","sessionId":"sess_abc","createdAt":1703001600000}
{"type":"message","id":"e_1","parentId":null,"message":{"role":"user","content":[...]},"timestamp":1703001601000}
{"type":"message","id":"e_2","parentId":"e_1","message":{"role":"assistant","content":[...]},"timestamp":1703001602000}
{"type":"modelChange","id":"e_3","parentId":"e_2","model":"claude-sonnet-4-20250514","timestamp":1703001603000}
{"type":"compaction","id":"e_4","parentId":"e_3","summary":"...","firstKeptEntryId":"e_2","timestamp":1703001604000}
```

### 关键不变量

- 每个 entry 有唯一 `id`
- 每个 entry 有 `parentId` 指向前一个 entry（tree structure）
- `header` 必须是第一行
- 顺序只增不改
- tree navigation 通过 parentId chain 实现
- branch point = 多个 entry 共享同一 parentId

## Session Navigation (Tree Structure)

TS 参考点: `session-manager.ts` -> tree traversal methods

### 概念模型

Session 是一棵树：
- root = header entry
- linear path = 按 parentId 链式追溯
- branch = 某个 entry 有多个 children
- leaf = 当前活跃位置

```text
         header
           |
         e_1 (user)
           |
         e_2 (assistant)
        /     \
     e_3       e_5 (branch)
       |         |
     e_4       e_6
     (leaf)
```

### Context Build

TS 参考点: `session-manager.ts` -> `buildSessionContext()`

从 leaf 沿 parentId 回溯到 root，收集所有 message entries 构建 context：

```text
buildSessionContext(leafId):
1. 从 leafId 开始沿 parentId chain 回溯
2. 遇到 compaction entry -> 注入 compaction summary message
3. 遇到 branch summary entry -> 注入 branch summary message
4. 遇到 message entry -> 收入 context
5. reverse 得到时间顺序 messages
6. 返回 SessionContext { messages, leafId, branchHistory }
```

### Branch Operations

- **Fork**: 从当前 leaf 创建新 branch（新的 entry chain）
- **Navigate**: 切换到不同的 leaf
- **Label**: 给 entry 打标签方便识别

## Message 格式和 Custom Messages

### Standard Messages

与 rozsa-model 的 `Message` enum 一致：

```rust
pub enum Message {
    User(UserMessage),
    Assistant(AssistantMessage),
    ToolResult(ToolResultMessage),
}
```

### Custom Messages

TS 参考点: `agent-session.ts` -> custom message handling

Custom messages 用于 extension 注入非标准内容（compaction summary、branch summary、system notification）。

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomAgentMessage {
    pub role: String,
    pub custom_type: Option<String>,
    pub content: serde_json::Value,
    pub display: Option<bool>,
    pub details: Option<serde_json::Value>,
    pub timestamp: i64,
}
```

关键语义：
- `display: false` 的 custom message 不发送给 LLM
- `custom_type` 用于 extension 识别自己的消息
- Rust 不解释 custom payload，只做 opaque transport

### Compaction Summary Message

```rust
pub fn create_compaction_summary_message(summary: &str) -> AgentMessage {
    AgentMessage::Custom(CustomAgentMessage {
        role: "user".to_string(),
        custom_type: Some("compaction_summary".to_string()),
        content: serde_json::json!([{
            "type": "text",
            "text": summary
        }]),
        display: Some(false),
        details: None,
        timestamp: now_ms(),
    })
}
```

### Branch Summary Message

```rust
pub fn create_branch_summary_message(summary: &str) -> AgentMessage {
    AgentMessage::Custom(CustomAgentMessage {
        role: "user".to_string(),
        custom_type: Some("branch_summary".to_string()),
        content: serde_json::json!([{
            "type": "text",
            "text": summary
        }]),
        display: Some(false),
        details: None,
        timestamp: now_ms(),
    })
}
```

## 迁移任务

### SESSION-001: Session entry 类型定义

参考点: `session-manager.ts` -> SessionEntry, SessionHeader, SessionInfo

迁移动作:
- 定义 Rust SessionEntry enum
- 定义 SessionHeader struct
- 定义 SessionInfo struct
- 实现 serde JSON 序列化（字段名与 TS camelCase 对齐）

优化点:
- 使用 Rust enum 代替 TS union type，编译时保证完备性
- 使用 `#[serde(rename_all = "camelCase")]` 自动对齐

完整性测试:
- 用 TS 生成的真实 session JSONL 文件作为 golden fixture
- Rust 反序列化每一行，再序列化回去，round-trip 无损

### SESSION-002: SessionManager 读取

参考点: `session-manager.ts` -> load(), getAllEntries(), buildEntryTree()

迁移动作:
- 实现 JSONL 文件读取
- 实现 entry tree 构建（parentId indexing）
- 实现 leaf detection
- 实现 entry 按 id 查找

优化点:
- 使用 HashMap<String, SessionEntry> + children index 加速 tree traversal
- 使用 tokio::fs 异步读取

完整性测试:
- 读取真实 session file，验证 entry count、tree structure、leaf id

### SESSION-003: SessionManager context build

参考点: `session-manager.ts` -> buildSessionContext()

迁移动作:
- 从 leaf 沿 parentId 回溯
- 处理 compaction entry（注入 summary message，跳过 removed entries）
- 处理 branch summary entry（注入 summary message）
- 构建有序 message array

优化点:
- 缓存 parentId chain 避免重复遍历

完整性测试:
- 用包含 compaction 和 branch 的 session file 测试
- context messages 与 TS buildSessionContext() 结果一致

### SESSION-004: SessionManager 写入

参考点: `session-manager.ts` -> appendMessage(), appendCompaction(), appendCustom()

迁移动作:
- 实现 appendFileSync 等价的原子追加
- 实现 entry id 生成
- 实现 parentId 链接（追加时 parentId = 当前 leafId）
- 实现 leaf 更新

优化点:
- 使用 file append mode 而非 read-write-truncate
- 使用 fd-lock 保证并发安全

完整性测试:
- append 后 re-read，验证 entry 存在
- 验证 parentId chain 正确
- 验证 leaf 更新正确

### SESSION-005: AgentSession 骨架

参考点: `agent-session.ts` -> constructor, prompt(), continue_run()

迁移动作:
- 定义 AgentSession struct
- 组合 SessionManager + SettingsManager + PermissionGuard + tools
- 实现 prompt() 调用 rozsa-core agent_loop
- 实现 continue_run() 调用 rozsa-core agent_loop_continue
- 实现 event 消费 loop

优化点:
- 使用 tokio::sync::broadcast 替代 event emitter
- AgentSession 持有 Arc 共享状态

完整性测试:
- no-tool prompt 事件顺序与 TS 一致
- continue 事件顺序与 TS 一致
- abort 行为与 TS 一致

### SESSION-006: Session restore

参考点: `sdk.ts` -> session restore path

迁移动作:
- 加载已有 session file
- buildSessionContext() 恢复 messages
- 恢复 model 和 settings 状态
- 恢复 leaf position

优化点:
- lazy context build（只在 prompt 时构建，不在 load 时）

完整性测试:
- 恢复后 context messages 与 TS 一致
- 恢复后可正常 prompt

### SESSION-007: Session branching

参考点: `session-manager.ts` -> fork, navigate

迁移动作:
- fork: 从 leaf 创建新 entry chain
- navigate: 切换 leaf，rebuild context
- history: 记录 branch 切换

优化点:
- branch summary 自动生成

完整性测试:
- fork 后两个 branch 独立
- navigate 切换后 context 正确
- 双向 navigate 可恢复

### SESSION-008: Compaction persistence

参考点: `session-manager.ts` -> appendCompaction()

迁移动作:
- 追加 compaction entry (summary, firstKeptEntryId)
- 更新 leaf
- buildSessionContext 中处理 compaction entry

优化点:
- compaction entry 包含 version field 用于未来扩展

完整性测试:
- compaction 后 context 只包含 firstKeptEntryId 之后的 entries + summary
- round-trip 不丢数据
