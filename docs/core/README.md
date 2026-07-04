# rozsa-core — Agent Loop Engine

Related Code: [crates/rozsa-core/src/](../../crates/rozsa-core/src/)

## 概述

rozsa-core 是 agent 执行引擎，提供：
- `agent_loop` — 驱动 model → tool → model 的循环
- `Tool` trait — 工具抽象接口
- `AgentEvent` — 执行过程中的事件流
- `AgentMessage` — 对话消息类型（Standard / Custom）

不包含具体工具实现、session 管理、UI 等。纯引擎层。

## 模块结构

```
src/
├── lib.rs          模块声明
├── agent_loop.rs   agent_loop / agent_loop_continue 执行引擎
├── config.rs       AgentContext / AgentLoopConfig / hook 类型
├── events.rs       AgentEvent 枚举
├── messages.rs     AgentMessage / CustomAgentMessage
├── tool.rs         Tool trait / ToolResult / ToolError
└── queue.rs        PendingMessageQueue（steering/follow-up 队列）
```

## 核心接口

### agent_loop

```rust
pub fn agent_loop(
    prompts: Vec<AgentMessage>,
    context: AgentContext,
    config: AgentLoopConfig,
    signal: Option<CancellationToken>,
) -> EventStream<AgentEvent>
```

启动一个新的 agent 循环。`prompts` 作为初始消息注入，`context` 提供 system prompt + 历史消息 + tool schemas，`config` 控制模型调用、工具执行、停止条件等。返回可异步消费的事件流。

### agent_loop_continue

```rust
pub fn agent_loop_continue(
    context: AgentContext,
    config: AgentLoopConfig,
    signal: Option<CancellationToken>,
) -> EventStream<AgentEvent>
```

从已有上下文继续执行（用于 compaction 后恢复、retry 等）。如果上下文为空或最后一条是 assistant 消息则立即结束。

### AgentContext

```rust
pub struct AgentContext {
    pub system_prompt: Option<String>,
    pub messages: Vec<AgentMessage>,
    pub tools: Vec<ToolSchema>,
}
```

agent loop 的输入上下文。`messages` 是完整对话历史，`tools` 是当前可用工具的 JSON schema 列表。

### AgentLoopConfig

```rust
pub struct AgentLoopConfig {
    pub model: Model,
    pub reasoning: Option<ThinkingLevel>,
    pub stream_options: SimpleStreamOptions,
    pub model_stream: ModelStreamFn,
    pub convert_to_llm: Box<dyn Fn(&[AgentMessage]) -> Vec<Message> + Send + Sync>,
    pub transform_context: Option<Box<dyn Fn(&[AgentMessage]) -> Vec<AgentMessage> + Send + Sync>>,
    pub get_api_key: Option<Box<dyn Fn(&str) -> Option<String> + Send + Sync>>,
    pub should_stop_after_turn: Option<Box<dyn Fn(&ShouldStopContext) -> bool + Send + Sync>>,
    pub prepare_next_turn: Option<Box<dyn Fn(&ShouldStopContext) -> Option<TurnUpdate> + Send + Sync>>,
    pub get_steering_messages: Option<Box<dyn Fn() -> Vec<AgentMessage> + Send + Sync>>,
    pub get_follow_up_messages: Option<Box<dyn Fn() -> Vec<AgentMessage> + Send + Sync>>,
    pub tool_execution: ToolExecutionMode,
    pub pre_tool_use: Option<Box<dyn Fn(PreToolUseContext) -> Pin<Box<dyn Future<Output = Option<PreToolUseResult>> + Send>> + Send + Sync>>,
    pub post_tool_use: Option<Box<dyn Fn(&PostToolUseContext) -> Option<PostToolUseResult> + Send + Sync>>,
    pub tools: Vec<Arc<dyn Tool>>,
}
```

| 字段 | 用途 |
|------|------|
| `model_stream` | 发起 LLM 请求并返回流式事件 |
| `convert_to_llm` | AgentMessage → provider 原生 Message 格式 |
| `transform_context` | 可选的上下文变换（如 token 裁剪） |
| `should_stop_after_turn` | 每轮结束后判断是否停止 |
| `prepare_next_turn` | 轮间更新 context/model/thinking |
| `get_steering_messages` | 注入 steering 消息（tool call 之间插入） |
| `get_follow_up_messages` | 注入 follow-up 消息（所有 tool call 完成后） |
| `pre_tool_use` | 工具执行前 hook（权限检查、scope 限制） |
| `post_tool_use` | 工具执行后 hook（结果覆盖） |

### PreToolUseContext / PreToolUseResult

```rust
pub struct PreToolUseContext {
    pub assistant_message: AssistantMessage,
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
    pub context: AgentContext,
}

pub struct PreToolUseResult {
    pub block: bool,
    pub reason: Option<String>,
}
```

`pre_tool_use` 返回 `Some(PreToolUseResult { block: true, reason })` 阻止工具执行。

### PostToolUseContext / PostToolUseResult

```rust
pub struct PostToolUseContext {
    pub assistant_message: AssistantMessage,
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
    pub result: ToolResult,
    pub is_error: bool,
    pub context: AgentContext,
}

pub struct PostToolUseResult {
    pub content: Option<Vec<ContentBlock>>,
    pub is_error: Option<bool>,
    pub terminate: Option<bool>,
}
```

`post_tool_use` 可覆盖工具执行结果。

### TurnUpdate

```rust
pub struct TurnUpdate {
    pub context: Option<AgentContext>,
    pub model: Option<Model>,
    pub thinking_level: Option<ThinkingLevel>,
}
```

`prepare_next_turn` 返回的更新 — 可在轮间切换 model 或调整 thinking level。

## Tool trait

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn label(&self) -> &str;
    fn parameters_schema(&self) -> &serde_json::Value;

    async fn execute(
        &self,
        tool_call_id: &str,
        params: serde_json::Value,
        signal: Option<CancellationToken>,
        on_update: Option<&(dyn Fn(ToolResult) + Send + Sync)>,
    ) -> Result<ToolResult, ToolError>;

    fn execution_mode(&self) -> Option<ToolExecutionMode> { None }
}
```

### ToolResult / ToolError

```rust
pub struct ToolResult {
    pub content: Vec<ContentBlock>,
    pub details: serde_json::Value,
    pub terminate: bool,  // true = 此 tool call 后停止循环
}

pub enum ToolError {
    Execution(String),
    Cancelled,
}
```

### ToolExecutionMode

```rust
pub enum ToolExecutionMode {
    Sequential,  // 一次执行一个 tool call
    Parallel,    // 同一轮内所有 tool call 并发执行
}
```

## AgentEvent

```rust
pub enum AgentEvent {
    AgentStart,
    AgentEnd { messages: Vec<AgentMessage> },
    TurnStart,
    TurnEnd { message: AssistantMessage, tool_results: Vec<ToolResultMessage> },
    MessageStart { message: AgentMessage },
    MessageUpdate { message: AgentMessage, stream_event: StreamEvent },
    MessageEnd { message: AgentMessage },
    ToolExecutionStart { tool_call_id: String, tool_name: String, args: Value },
    ToolExecutionEnd { tool_call_id: String, tool_name: String, result: ToolResultMessage },
}
```

事件生命周期：`AgentStart → (TurnStart → MessageStart → MessageUpdate* → MessageEnd → ToolExecutionStart → ToolExecutionEnd → TurnEnd)* → AgentEnd`

## AgentMessage

```rust
pub enum AgentMessage {
    Standard { message: Message },   // User / Assistant / ToolResult
    Custom { message: CustomAgentMessage },  // 扩展消息（bash execution 等）
}

pub struct CustomAgentMessage {
    pub message_type: String,
    pub payload: serde_json::Value,
    pub timestamp: i64,
}
```

方法：
- `AgentMessage::standard(msg)` — 构造 Standard
- `AgentMessage::custom(type, payload, ts)` — 构造 Custom
- `msg.as_standard()` — 提取 &Message（Custom 返回 None）

## PendingMessageQueue

```rust
pub enum QueueMode { All, OneAtATime }

pub struct PendingMessageQueue {
    pub mode: QueueMode,
    // ...
}

impl PendingMessageQueue {
    pub fn new(mode: QueueMode) -> Self;
    pub fn enqueue(&mut self, message: AgentMessage);
    pub fn has_items(&self) -> bool;
    pub fn drain(&mut self) -> Vec<AgentMessage>;
    pub fn clear(&mut self);
}
```

`QueueMode::All` 一次取完，`OneAtATime` 每次只取一条。用于 steering/follow-up 消息的投递控制。

## 与其他 crate 的关系

```
rozsa-model (types, streaming)
     ↑
rozsa-core (agent loop engine)
     ↑
rozsa-app (AgentSession 构建 config，调用 agent_loop)
```

- **rozsa-core 依赖 rozsa-model** — 使用 Model、Message、StreamEvent、ContentBlock 等类型
- **rozsa-app 依赖 rozsa-core** — AgentSession 构建 AgentContext + AgentLoopConfig 后调用 agent_loop，消费 EventStream

## 使用示例

```rust
use rozsa_core::{agent_loop::agent_loop, config::*, messages::AgentMessage, tool::Tool};
use rozsa_model::types::*;
use std::sync::Arc;

// 构建上下文
let context = AgentContext {
    system_prompt: Some("You are a helpful assistant.".to_string()),
    messages: vec![],
    tools: vec![],  // ToolSchema 列表
};

// 构建配置
let config = AgentLoopConfig {
    model: my_model,
    reasoning: Some(ThinkingLevel::Medium),
    stream_options: SimpleStreamOptions::default(),
    model_stream: Box::new(|model, ctx, opts| { /* 调用 provider */ }),
    convert_to_llm: Box::new(|msgs| { /* AgentMessage → Message */ }),
    tools: vec![Arc::new(my_tool) as Arc<dyn Tool>],
    tool_execution: ToolExecutionMode::Parallel,
    // ...其他字段设为 None
};

// 启动循环
let user_msg = AgentMessage::standard(Message::User(UserMessage {
    content: UserContent::Text("Hello".into()),
    display_text: None,
    timestamp: 0,
}));

let mut stream = agent_loop(vec![user_msg], context, config, None);

// 消费事件
while let Some(event) = stream.next().await {
    match event {
        AgentEvent::MessageEnd { message } => { /* 处理完整消息 */ }
        AgentEvent::ToolExecutionStart { tool_name, .. } => { /* 工具开始 */ }
        _ => {}
    }
}
```

## 执行流程

1. emit `AgentStart`
2. 注入 prompt messages → emit `MessageStart` / `MessageEnd`
3. 进入循环：
   - 取 steering messages（如有）
   - 调用 model stream → 收集 assistant response
   - 如果有 tool calls → 执行（parallel 或 sequential）
   - emit `TurnEnd`
   - 检查 `should_stop_after_turn` / `prepare_next_turn`
   - 如果无更多 tool calls → 取 follow-up messages，有则继续
4. emit `AgentEnd`
