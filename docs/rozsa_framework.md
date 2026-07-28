# Rozsa Framework 架构设计

项目从 TypeScript monorepo (packages/) 全量重写为 Rust workspace (crates/)。

## 项目根目录结构

```
rozsa/
├── Cargo.toml              ← workspace 根配置
├── Cargo.lock
├── CLAUDE.md               ← 项目指令
├── AGENTS.md               ← Agent 开发规则
├── README.md
├── LICENSE
├── crates/                 ← 所有 Rust crate
│   ├── rozsa-model/
│   ├── rozsa-core/
│   ├── rozsa-app/
│   ├── rozsa-tui/
│   └── rozsa-cli/
├── docs/                   ← 所有文档（统一入口）
│   ├── architecture.md     ← 本文件（架构设计）
│   ├── model/              ← rozsa-model 相关文档
│   ├── core/               ← rozsa-core 相关文档
│   ├── app/                ← rozsa-app 相关文档
│   ├── tui/                ← rozsa-tui 相关文档
│   └── dev/                ← 开发指南、变更日志、迁移记录
├── tests/                  ← 所有测试（统一入口）
│   ├── unit/               ← 单元测试（按 crate 分子目录）
│   │   ├── model/
│   │   ├── core/
│   │   ├── app/
│   │   └── tui/
│   ├── integration/        ← 集成测试（跨 crate 行为验证）
│   └── fixtures/           ← 测试用共享数据（mock responses, 配置文件等）
├── devtools/               ← 构建/发布/代码生成脚本
│   ├── generate-models.rs  ← 模型静态表生成
│   └── release.sh
└── tmp/                    ← 临时文件（.gitignore 排除）
```

### 文档组织原则

- 所有文档放 `docs/`，crate 内部不放 `.md` 文件
- `docs/` 按 crate 分子目录，每个子目录放该 crate 的设计文档、API 说明、协议描述
- `docs/dev/` 放跨 crate 的开发指南（贡献规范、发布流程、变更日志）
- crate 的 `Cargo.toml` 中 `[package.documentation]` 指向对应 docs/ 子目录

### 测试组织原则

- 所有测试放 `tests/`，crate 内部不放 `tests/` 目录
- `tests/unit/` 按 crate 分子目录，每个文件对应一个模块的单元测试
- `tests/integration/` 放跨 crate 的集成测试（如 "model → core → app 全链路"）
- `tests/fixtures/` 放共享测试数据，避免各测试文件内联大段 mock 数据
- Cargo.toml 中通过 `[[test]]` 表指定测试路径：

```toml
# 根 Cargo.toml
[[test]]
name = "integration"
path = "tests/integration/main.rs"
```

```toml
# crates/rozsa-model/Cargo.toml
[[test]]
name = "unit_model"
path = "../../tests/unit/model/main.rs"
```

## Crate 结构

```
crates/
├── rozsa-model/    ← 模型抽象层
├── rozsa-core/     ← Agent 引擎
├── rozsa-app/      ← 应用运行时
├── rozsa-tui/      ← ratatui 终端前端
└── rozsa-cli/      ← binary 入口
```

## 依赖方向

```
rozsa-cli ──→ rozsa-app ──→ rozsa-core ──→ rozsa-model
rozsa-tui ──↗
```

## 完整文件树

```
crates/
├── rozsa-model/
│   ├── Cargo.toml
│   ├── build.rs                          ← 生成 models.rs（从 models.json 或 codegen）
│   └── src/
│       ├── lib.rs                        ← pub mod 声明 + re-exports
│       ├── types.rs                      ← Model, Api, Provider, ContentBlock, Message...
│       ├── stream.rs                     ← stream_simple() / stream() 入口
│       ├── event_stream.rs              ← EventStream<T> 异步事件流
│       ├── registry.rs                   ← ApiProvider 全局注册表
│       ├── env_keys.rs                   ← 环境变量 API key 解析
│       ├── models.rs                     ← [generated] 模型静态表
│       └── providers/
│           ├── mod.rs                    ← 各 provider 注册入口
│           ├── anthropic.rs             ← Anthropic Messages API
│           ├── openai_completions.rs    ← OpenAI Chat Completions
│           ├── openai_responses.rs      ← OpenAI Responses API
│           ├── bedrock.rs               ← AWS Bedrock Converse Stream
│           ├── google.rs                ← Google Generative AI
│           ├── google_vertex.rs         ← Google Vertex AI
│           ├── mistral.rs               ← Mistral Conversations
│           ├── deepseek.rs              ← DeepSeek (OpenAI compat)
│           ├── openrouter.rs            ← OpenRouter (OpenAI compat)
│           ├── xai.rs                   ← xAI (OpenAI compat)
│           ├── groq.rs                  ← Groq (OpenAI compat)
│           └── faux.rs                  ← 测试用 mock provider
│
├── rozsa-core/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                        ← pub mod 声明 + re-exports
│       ├── agent.rs                      ← Agent struct + state machine
│       ├── agent_loop.rs                ← agent_loop() / agent_loop_continue() / run_loop()
│       ├── events.rs                     ← AgentEvent enum
│       ├── messages.rs                   ← AgentMessage enum + CustomMessage trait
│       ├── tool.rs                       ← Tool trait + ToolResult + ToolExecutionMode
│       ├── session.rs                    ← SessionStore trait + SessionInfo
│       ├── config.rs                     ← AgentLoopConfig + hooks (before/after tool call)
│       └── queue.rs                      ← PendingMessageQueue + QueueMode
│
├── rozsa-app/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                        ← pub mod 声明 + re-exports
│       ├── session/
│       │   ├── mod.rs                   ← AgentSession struct + 公共 API
│       │   ├── runtime.rs               ← AgentSessionRuntime + 工厂
│       │   ├── services.rs              ← AgentSessionServices（cwd 绑定服务集）
│       │   └── manager.rs               ← SessionManager（jsonl 持久化实现）
│       ├── tools/
│       │   ├── mod.rs                   ← create_*_tool 工厂 + ToolName enum
│       │   ├── bash.rs                  ← shell 命令执行
│       │   ├── read.rs                  ← 文件读取
│       │   ├── edit.rs                  ← 文件编辑（fuzzy match + diff）
│       │   ├── write.rs                 ← 文件写入
│       │   ├── grep.rs                  ← ripgrep 封装
│       │   ├── find.rs                  ← fd 封装
│       │   ├── ls.rs                    ← 目录列表
│       │   ├── subagent.rs              ← 子 agent 调用
│       │   ├── queue.rs                 ← 文件写操作排队
│       │   └── truncate.rs             ← 输出截断策略
│       ├── permissions/
│       │   ├── mod.rs                   ← PermissionGuard trait + PermissionMode
│       │   ├── rules.rs                 ← 白名单/黑名单规则匹配
│       │   ├── risk.rs                  ← 风险等级评估
│       │   └── decision.rs             ← PermissionDecision 构建
│       ├── settings/
│       │   ├── mod.rs                   ← SettingsManager
│       │   ├── schema.rs               ← 配置 schema 定义
│       │   ├── loader.rs               ← 文件加载 + 合并（global → project → runtime）
│       │   └── resolve.rs              ← 配置值解析（env var, 默认值）
│       ├── extensions/
│       │   ├── mod.rs                   ← ExtensionRunner
│       │   ├── types.rs                 ← Extension trait + ExtensionContext
│       │   ├── loader.rs               ← 扩展发现 + 加载
│       │   └── runner.rs               ← 生命周期事件分发
│       ├── resources/
│       │   ├── mod.rs                   ← ResourceLoader
│       │   └── instructions.rs         ← CLAUDE.md / AGENTS.md 解析
│       ├── compaction/
│       │   ├── mod.rs                   ← CompactionEngine + CompactionResult
│       │   ├── summarizer.rs           ← LLM 摘要生成
│       │   └── branch.rs              ← 分支摘要
│       ├── skills/
│       │   ├── mod.rs                   ← SkillMatcher
│       │   ├── types.rs                 ← Skill + SkillTrigger
│       │   └── system_prompt.rs        ← system prompt 拼装
│       ├── model_registry/
│       │   ├── mod.rs                   ← ModelRegistry
│       │   ├── probe.rs                ← provider 可用性探测
│       │   └── auth.rs                 ← AuthStorage（API key 持久化）
│       ├── messages.rs                   ← 产品级自定义消息类型
│       └── runtime_state.rs             ← RuntimeState 快照（供 TUI）
│
├── rozsa-tui/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── main.rs
│       ├── app.rs                       ← App 主循环 (event loop + state)
│       ├── protocol.rs                  ← 协议定义 (HostMessage, ClientMessage)
│       ├── ui/
│       │   ├── mod.rs                   ← 缓存基础设施 + render() 主入口
│       │   ├── layout.rs               ← 布局计算 (heights/constraints)
│       │   └── render.rs               ← 所有 render_* 函数 + message_lines
│       ├── input/
│       │   ├── mod.rs                   ← InputState 类型 + 核心数据方法
│       │   ├── keys.rs                  ← handle_key + 选区/折叠/编辑操作
│       │   └── mouse.rs                ← 鼠标事件 + 粘贴处理
│       ├── components/
│       │   ├── mod.rs
│       │   ├── editor.rs               ← 编辑器插件接口
│       │   ├── sidebar.rs              ← 侧边栏 (git/model/tokens/agents/files)
│       │   ├── model_selector.rs       ← 模型选择器
│       │   ├── session_selector.rs     ← 会话选择器
│       │   ├── session_search.rs       ← 会话搜索
│       │   ├── session_tree.rs         ← 会话树结构
│       │   ├── permission.rs           ← 权限审批面板
│       │   ├── autocomplete.rs         ← 自动补全面板
│       │   ├── autocomplete_provider.rs ← Provider 架构
│       │   └── graph.rs                ← 会话历史图
│       ├── backend/
│       │   ├── mod.rs                   ← AgentBackend trait + BackendEvent
│       │   ├── socket.rs               ← SocketBackend（与 TS 通信）
│       │   └── mock.rs                 ← MockBackend（测试用）
│       ├── command/
│       │   ├── mod.rs                   ← 命令系统
│       │   └── builtin.rs              ← 内置命令
│       ├── theme/
│       │   ├── mod.rs                   ← 主题运行时管理 (ThemeProxy, THEME)
│       │   └── palette.rs              ← 调色板定义 (Theme struct, dark/light)
│       ├── overlay.rs                   ← Overlay 定位与焦点栈
│       ├── keymap.rs                    ← 快捷键绑定匹配 + KeybindingsManager
│       ├── markdown.rs                  ← Markdown 渲染 (语法高亮, 图片, 超链接)
│       ├── highlight.rs                ← 代码语法高亮 (syntect/two-face)
│       ├── hyperlink.rs                ← OSC 8 终端超链接
│       ├── terminal_image.rs           ← 终端图片协议 (Kitty/iTerm2)
│       ├── terminal_caps.rs            ← 终端能力检测
│       ├── ansi.rs                     ← ANSI SGR → ratatui Style
│       ├── fuzzy.rs                    ← Fuzzy 匹配评分
│       ├── undo.rs                     ← Undo 栈（编辑器撤销）
│       └── kill_ring.rs                ← Kill Ring（Emacs 剪切环）
│
└── rozsa-cli/
    ├── Cargo.toml
    └── src/
        ├── main.rs                       ← fn main() 入口
        ├── args.rs                       ← clap 参数定义
        └── run.rs                        ← 模式分发（interactive / print / rpc）
```

---

## rozsa-model — 模型抽象层

不依赖任何其他 crate。对上层暴露统一的流式调用接口。

### 内部模块

```rust
// crates/rozsa-model/src/
mod types;           // Model, Api, Provider, ContentBlock, Message 等核心类型
mod stream;          // stream_simple() 入口 + EventStream
mod registry;        // ApiProvider 注册表（全局 registry）
mod env_keys;        // 环境变量 API key 解析
mod models;          // 自动生成的模型静态表
pub mod providers {  // 各 LLM provider 实现
    mod anthropic;
    mod openai;
    mod bedrock;
    mod google;
    mod mistral;
    mod deepseek;
    // ...
}
```

### 核心类型

```rust
/// API 协议标识 — 决定请求/响应格式
pub enum Api {
    AnthropicMessages,
    OpenAICompletions,
    OpenAIResponses,
    BedrockConverseStream,
    GoogleGenerativeAI,
    GoogleVertex,
    MistralConversations,
    Custom(String),
}

/// LLM 厂商标识
pub enum Provider {
    Anthropic,
    OpenAI,
    AmazonBedrock,
    Google,
    GoogleVertex,
    DeepSeek,
    // ... 30+ variants
    Custom(String),
}

/// 模型静态描述
pub struct Model {
    pub id: String,
    pub name: String,
    pub api: Api,
    pub provider: Provider,
    pub base_url: String,
    pub reasoning: bool,
    pub input_modalities: Vec<InputModality>,  // text, image
    pub cost: ModelCost,
    pub context_window: usize,
    pub max_tokens: usize,
    pub thinking_level_map: Option<ThinkingLevelMap>,
    pub headers: Option<HashMap<String, String>>,
    pub compat: Option<ProviderCompat>,
}

pub struct ModelCost {
    pub input: f64,       // $/million tokens
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

pub enum InputModality { Text, Image }

pub enum ThinkingLevel { Off, Minimal, Low, Medium, High, XHigh }
```

### 流式调用接口

```rust
/// Provider trait — 每个 API 协议实现一次
#[async_trait]
pub trait ApiProvider: Send + Sync {
    fn api(&self) -> Api;

    fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: &StreamOptions,
    ) -> EventStream<StreamEvent>;

    fn stream_simple(
        &self,
        model: &Model,
        context: &Context,
        options: &SimpleStreamOptions,
    ) -> EventStream<StreamEvent>;
}

/// 流式事件 — 逐 token 推送
pub enum StreamEvent {
    Start { partial: AssistantMessage },
    TextStart { content_index: usize, partial: AssistantMessage },
    TextDelta { content_index: usize, delta: String, partial: AssistantMessage },
    TextEnd { content_index: usize, content: String, partial: AssistantMessage },
    ThinkingStart { content_index: usize, partial: AssistantMessage },
    ThinkingDelta { content_index: usize, delta: String, partial: AssistantMessage },
    ThinkingEnd { content_index: usize, content: String, partial: AssistantMessage },
    ToolCallStart { content_index: usize, partial: AssistantMessage },
    ToolCallDelta { content_index: usize, delta: String, partial: AssistantMessage },
    ToolCallEnd { content_index: usize, tool_call: ToolCall, partial: AssistantMessage },
    Done { reason: StopReason, message: AssistantMessage },
    Error { reason: StopReason, error: AssistantMessage },
}

/// EventStream — 异步事件流（类似 tokio::sync::mpsc）
pub struct EventStream<T> { /* ... */ }

impl<T> EventStream<T> {
    pub fn push(&self, event: T);
    pub fn end(&self, result: T);
    pub async fn next(&mut self) -> Option<T>;
    pub async fn result(self) -> T;  // 等待最终结果
}
```

### 消息类型

```rust
pub enum ContentBlock {
    Text { text: String, signature: Option<String> },
    Thinking { thinking: String, signature: Option<String>, redacted: bool },
    Image { data: String, mime_type: String },
    ToolCall(ToolCall),
}

pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub cost: UsageCost,
}

pub enum StopReason { Stop, Length, ToolUse, Error, Aborted }

pub struct UserMessage {
    pub content: UserContent,
    pub display_text: Option<String>,
    pub timestamp: i64,
}

pub enum UserContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

pub struct AssistantMessage {
    pub content: Vec<ContentBlock>,
    pub api: Api,
    pub provider: Provider,
    pub model: String,
    pub response_model: Option<String>,
    pub response_id: Option<String>,
    pub usage: Usage,
    pub stop_reason: StopReason,
    pub error_message: Option<String>,
    pub timestamp: i64,
}

pub struct ToolResultMessage {
    pub tool_call_id: String,
    pub tool_name: String,
    pub content: Vec<ContentBlock>,
    pub is_error: bool,
    pub timestamp: i64,
}

pub enum Message {
    User(UserMessage),
    Assistant(AssistantMessage),
    ToolResult(ToolResultMessage),
}

/// 发给 LLM 的完整上下文
pub struct Context {
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSchema>,
}

pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,  // JSON Schema
}
```

### Provider 注册机制

```rust
/// 全局 registry — 按 Api 分发
static REGISTRY: Lazy<RwLock<HashMap<Api, Box<dyn ApiProvider>>>> = ...;

pub fn register_provider(provider: impl ApiProvider + 'static);
pub fn get_provider(api: &Api) -> Option<&dyn ApiProvider>;

/// 统一调用入口
pub fn stream_simple(
    model: &Model,
    context: &Context,
    options: &SimpleStreamOptions,
) -> EventStream<StreamEvent> {
    let provider = get_provider(&model.api)
        .unwrap_or_else(|| panic!("No provider for api: {:?}", model.api));
    provider.stream_simple(model, context, options)
}
```

### StreamOptions

```rust
pub struct StreamOptions {
    pub temperature: Option<f64>,
    pub max_tokens: Option<usize>,
    pub api_key: Option<String>,
    pub transport: Transport,
    pub cache_retention: CacheRetention,
    pub session_id: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    pub max_retries: Option<u32>,
    pub max_retry_delay_ms: Option<u64>,
    pub metadata: Option<serde_json::Value>,
}

pub struct SimpleStreamOptions {
    pub base: StreamOptions,
    pub reasoning: Option<ThinkingLevel>,
    pub thinking_budgets: Option<ThinkingBudgets>,
}

pub enum Transport { Sse, WebSocket, WebSocketCached, Auto }
pub enum CacheRetention { None, Short, Long }
```

---

## rozsa-core — Agent 引擎

依赖：rozsa-model。纯循环引擎 + 类型 + trait 接口。

### 内部模块

```rust
// crates/rozsa-core/src/
mod agent;           // Agent struct：状态机 + 事件 + 队列
mod agent_loop;      // agent_loop() / run_loop() 核心循环
mod events;          // AgentEvent 枚举
mod messages;        // AgentMessage 类型（扩展 Message）
mod tool;            // Tool trait + ToolResult
mod session;         // SessionStore trait
mod queue;           // PendingMessageQueue
mod config;          // AgentLoopConfig
```

### Agent 状态机

```rust
pub struct Agent {
    state: AgentState,
    listeners: Vec<Box<dyn Fn(&AgentEvent) + Send + Sync>>,
    steering_queue: PendingMessageQueue,
    follow_up_queue: PendingMessageQueue,
    stream_fn: Box<dyn StreamFn>,
    active_run: Option<ActiveRun>,
    pub session_id: Option<String>,
    pub thinking_budgets: Option<ThinkingBudgets>,
    pub transport: Transport,
    pub max_retry_delay_ms: Option<u64>,
    pub tool_execution: ToolExecutionMode,
}

pub struct AgentState {
    pub system_prompt: String,
    pub model: Model,
    pub thinking_level: ThinkingLevel,
    pub tools: Vec<Box<dyn Tool>>,
    pub messages: Vec<AgentMessage>,
    pub is_streaming: bool,
    pub streaming_message: Option<AssistantMessage>,
    pub pending_tool_calls: HashSet<String>,
    pub error_message: Option<String>,
}

/// 消息队列 — 支持 steering（插入优先消息）和 follow-up（追加后续消息）
struct PendingMessageQueue {
    messages: Vec<AgentMessage>,
    mode: QueueMode,
}

pub enum QueueMode { All, OneAtATime }
```

### StreamFn trait

```rust
/// 对 rozsa_model::stream_simple 的 trait 抽象
/// 允许 app 层包装（加 retry、auth 刷新、logging 等）
pub trait StreamFn: Send + Sync {
    fn call(
        &self,
        model: &Model,
        context: &Context,
        options: &SimpleStreamOptions,
    ) -> EventStream<StreamEvent>;
}

/// 默认实现：直接委托到 rozsa_model::stream_simple
pub struct DefaultStreamFn;

impl StreamFn for DefaultStreamFn {
    fn call(&self, model: &Model, context: &Context, options: &SimpleStreamOptions)
        -> EventStream<StreamEvent>
    {
        rozsa_model::stream_simple(model, context, options)
    }
}
```

### Agent Loop

```rust
/// 启动一轮 agent 循环（添加 prompt 到上下文，然后驱动）
pub fn agent_loop(
    prompts: Vec<AgentMessage>,
    context: AgentContext,
    config: &AgentLoopConfig,
    signal: Option<CancellationToken>,
) -> EventStream<AgentEvent> { /* ... */ }

/// 从当前上下文继续循环（用于 retry、tool result 后继续）
pub fn agent_loop_continue(
    context: AgentContext,
    config: &AgentLoopConfig,
    signal: Option<CancellationToken>,
) -> EventStream<AgentEvent> { /* ... */ }

/// 内部驱动循环
async fn run_loop(
    context: &mut AgentContext,
    config: &AgentLoopConfig,
    emit: &dyn Fn(AgentEvent),
    signal: Option<CancellationToken>,
) -> Vec<AgentMessage> {
    // 1. transformContext（如果配置了）
    // 2. convertToLlm → 得到 LLM 可理解的 Message[]
    // 3. stream_fn.call(model, context, options)
    // 4. 消费 StreamEvent → 产出 AgentEvent
    // 5. 如果有 tool_use → 依次/并行执行 tools
    // 6. beforeToolCall / afterToolCall hooks
    // 7. 检查 shouldStopAfterTurn
    // 8. 检查 steering_queue / follow_up_queue
    // 9. 如果还需继续 → 回到 step 1
}
```

### AgentEvent

```rust
pub enum AgentEvent {
    // 生命周期
    AgentStart,
    AgentEnd { messages: Vec<AgentMessage> },

    // Turn 粒度（一次 LLM 调用 + 其 tool 执行）
    TurnStart,
    TurnEnd { message: AssistantMessage, tool_results: Vec<ToolResultMessage> },

    // 消息粒度
    MessageStart { message: AgentMessage },
    MessageUpdate { message: AgentMessage, stream_event: StreamEvent },
    MessageEnd { message: AgentMessage },

    // Tool 执行粒度
    ToolExecutionStart { tool_call_id: String, tool_name: String, args: serde_json::Value },
    ToolExecutionUpdate { tool_call_id: String, tool_name: String, partial_result: ToolResult },
    ToolExecutionEnd { tool_call_id: String, tool_name: String, result: ToolResult, is_error: bool },
}
```

### Tool trait

```rust
/// Agent 运行时使用的 tool 定义
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn label(&self) -> &str;
    fn parameters_schema(&self) -> &serde_json::Value;  // JSON Schema

    /// 执行 tool call
    async fn execute(
        &self,
        tool_call_id: &str,
        params: serde_json::Value,
        signal: Option<CancellationToken>,
        on_update: Option<&dyn Fn(ToolResult)>,
    ) -> Result<ToolResult, ToolError>;

    /// 执行模式覆盖
    fn execution_mode(&self) -> Option<ToolExecutionMode> { None }
}

pub struct ToolResult {
    pub content: Vec<ContentBlock>,
    pub details: serde_json::Value,
    pub terminate: bool,
}

pub enum ToolExecutionMode { Sequential, Parallel }
```

### SessionStore trait

```rust
/// 会话持久化接口 — app 层提供具体实现
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn save(&self, session_id: &str, messages: &[AgentMessage]) -> Result<()>;
    async fn load(&self, session_id: &str) -> Result<Option<Vec<AgentMessage>>>;
    async fn list(&self) -> Result<Vec<SessionInfo>>;
    async fn delete(&self, session_id: &str) -> Result<()>;
}

pub struct SessionInfo {
    pub id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: usize,
}
```

### AgentLoopConfig

```rust
/// 循环配置 — 由 app 层组装后传入
pub struct AgentLoopConfig {
    pub model: Model,
    pub stream_options: SimpleStreamOptions,

    /// 将 AgentMessage 转为 LLM 可理解的 Message（过滤自定义消息类型）
    pub convert_to_llm: Box<dyn Fn(&[AgentMessage]) -> Vec<Message> + Send + Sync>,

    /// 可选：在 convert_to_llm 前变换上下文（裁剪、注入）
    pub transform_context: Option<Box<dyn Fn(&[AgentMessage]) -> Vec<AgentMessage> + Send + Sync>>,

    /// 动态获取 API key（支持短期 OAuth token 刷新）
    pub get_api_key: Option<Box<dyn Fn(&str) -> Option<String> + Send + Sync>>,

    /// Turn 结束后是否提前停止
    pub should_stop_after_turn: Option<Box<dyn Fn(&ShouldStopContext) -> bool + Send + Sync>>,

    /// Turn 结束后准备下一轮的状态覆盖
    pub prepare_next_turn: Option<Box<dyn Fn(&ShouldStopContext) -> Option<TurnUpdate> + Send + Sync>>,

    /// 获取 steering 消息
    pub get_steering_messages: Option<Box<dyn Fn() -> Vec<AgentMessage> + Send + Sync>>,

    /// 获取 follow-up 消息
    pub get_follow_up_messages: Option<Box<dyn Fn() -> Vec<AgentMessage> + Send + Sync>>,

    /// Tool 执行模式
    pub tool_execution: ToolExecutionMode,

    /// Tool call 前置 hook
    pub before_tool_call: Option<Box<dyn Fn(&BeforeToolCallContext) -> Option<BeforeToolCallResult> + Send + Sync>>,

    /// Tool call 后置 hook
    pub after_tool_call: Option<Box<dyn Fn(&AfterToolCallContext) -> Option<AfterToolCallResult> + Send + Sync>>,
}
```

### AgentMessage 扩展

```rust
/// AgentMessage — 基础 LLM Message 的超集
/// app 层可通过 enum variant 扩展自定义消息类型
pub enum AgentMessage {
    /// 标准 LLM 消息
    Standard(Message),
    /// 自定义消息（app 层定义具体内容）
    Custom(CustomMessage),
}

/// 自定义消息 — app 层通过此 trait 扩展
pub trait CustomMessage: Send + Sync + std::fmt::Debug {
    fn message_type(&self) -> &str;
    fn as_any(&self) -> &dyn std::any::Any;
}
```

---

## rozsa-app — 应用运行时

依赖：rozsa-core, rozsa-model。承载全部产品逻辑。

### 内部模块

```rust
// crates/rozsa-app/src/
mod session;          // AgentSession 主体 + 生命周期
mod tools;            // 内置 tool 实现
mod permissions;      // 权限策略
mod settings;         // 配置管理
mod extensions;       // 插件系统
mod resources;        // 资源加载（CLAUDE.md 等）
mod compaction;       // 上下文压缩
mod skills;           // Skill 匹配 + system prompt 拼装
mod model_registry;   // 模型发现/探测
mod messages;         // 产品级自定义消息类型
mod runtime_state;    // 运行时状态快照（UI 消费）
```

### session — AgentSession

```rust
/// 应用层会话 — 组合 Agent + 产品服务
pub struct AgentSession {
    agent: Agent,
    session_manager: SessionManager,
    settings_manager: SettingsManager,
    extension_runner: ExtensionRunner,

    scoped_models: Vec<ScopedModel>,
    steering_messages: Vec<String>,
    follow_up_messages: Vec<String>,
    pending_next_turn: Vec<CustomMessage>,

    // 压缩状态
    compaction_handle: Option<JoinHandle<()>>,
    auto_compaction_handle: Option<JoinHandle<()>>,
}

impl AgentSession {
    /// 发送 prompt 启动一轮对话
    pub async fn send(&mut self, prompt: &str, attachments: Vec<Attachment>) -> EventStream<AgentEvent>;

    /// 继续（retry / 手动 continue）
    pub async fn continue_run(&mut self) -> EventStream<AgentEvent>;

    /// 中止当前运行
    pub fn abort(&mut self);

    /// 手动触发上下文压缩
    pub async fn compact(&mut self);

    /// 切换模型
    pub fn set_model(&mut self, model: Model, thinking_level: ThinkingLevel);

    /// 获取运行时状态快照（供 TUI 渲染）
    pub fn runtime_state(&self) -> &RuntimeState;
}

/// 工厂 — 从配置创建完整运行时
pub struct AgentSessionRuntime {
    pub session: AgentSession,
    pub services: AgentSessionServices,
    pub diagnostics: Vec<Diagnostic>,
}

pub async fn create_session(options: CreateSessionOptions) -> Result<AgentSessionRuntime>;
```

### tools — 内置工具

```rust
// crates/rozsa-app/src/tools/
mod bash;       // shell 命令执行
mod read;       // 文件读取
mod edit;       // 文件编辑（fuzzy match + apply edits）
mod write;      // 文件写入
mod grep;       // ripgrep 封装
mod find;       // fd 封装
mod ls;         // 目录列表
mod subagent;   // 子 agent 调用
mod queue;      // 文件写操作排队（避免并发写冲突）
mod truncate;   // 输出截断策略

/// 每个 tool 通过工厂函数创建，绑定 cwd
pub fn create_bash_tool(cwd: &Path, options: &BashToolOptions) -> Box<dyn Tool>;
pub fn create_edit_tool(cwd: &Path, options: &EditToolOptions) -> Box<dyn Tool>;
pub fn create_read_tool(cwd: &Path, options: &ReadToolOptions) -> Box<dyn Tool>;
// ...

/// 常用组合
pub fn create_coding_tools(cwd: &Path, options: &ToolsOptions) -> Vec<Box<dyn Tool>>;
pub fn create_readonly_tools(cwd: &Path, options: &ToolsOptions) -> Vec<Box<dyn Tool>>;
pub fn create_all_tools(cwd: &Path, options: &ToolsOptions) -> Vec<Box<dyn Tool>>;

pub enum ToolName { Read, Bash, Edit, Write, Grep, Find, Ls }
```

### permissions — 权限策略

```rust
/// 三种权限模式
pub enum PermissionMode {
    OnRequest,       // 每次询问用户
    AutoPermission,  // 智能判断（低风险自动放行）
    FreePermission,  // 全部放行
}

pub enum RiskLevel { Low, Medium, High, Critical }

pub struct PermissionDecision {
    pub decision: DecisionValue,
    pub risk_level: RiskLevel,
    pub source: DecisionSource,
    pub reason: String,
    pub safer_alternative: Option<String>,
    pub mode: PermissionMode,
}

pub enum DecisionValue { Allow, Deny, AskUser }
pub enum DecisionSource { Whitelist, Blacklist, RiskAnalysis, UserChoice }

/// 权限守卫 — 在 beforeToolCall hook 中调用
#[async_trait]
pub trait PermissionGuard: Send + Sync {
    async fn check_tool_call(&self, tool_name: &str, args: &serde_json::Value) -> PermissionDecision;
    async fn check_shell_command(&self, command: &str) -> PermissionDecision;
}
```

### settings — 配置管理

```rust
/// 层级配置：全局 → 项目 → 运行时
pub struct SettingsManager {
    global_dir: PathBuf,
    project_dir: PathBuf,
    // 缓存的合并结果
    resolved: ResolvedSettings,
}

impl SettingsManager {
    pub fn get_permission_mode(&self) -> PermissionMode;
    pub fn get_model(&self) -> Option<String>;
    pub fn get_thinking_level(&self) -> Option<ThinkingLevel>;
    pub fn get_custom_tools(&self) -> Vec<ToolDefinition>;
    pub fn get_permissions_whitelist(&self) -> Vec<PermissionRule>;
    pub fn get_permissions_blacklist(&self) -> Vec<PermissionRule>;
    // ...
}
```

### extensions — 插件系统

```rust
/// 扩展能力
/// - 订阅 agent 生命周期事件
/// - 注册 LLM-callable tools
/// - 注册命令、快捷键、CLI flags
/// - 通过 UI primitives 与用户交互
pub struct ExtensionRunner {
    extensions: Vec<LoadedExtension>,
}

#[async_trait]
pub trait Extension: Send + Sync {
    fn name(&self) -> &str;

    /// 生命周期 hooks
    async fn on_session_start(&self, ctx: &ExtensionContext) -> Result<()> { Ok(()) }
    async fn on_turn_start(&self, ctx: &ExtensionContext) -> Result<()> { Ok(()) }
    async fn on_turn_end(&self, ctx: &ExtensionContext, message: &AssistantMessage) -> Result<()> { Ok(()) }
    async fn on_tool_call(&self, ctx: &ExtensionContext, tool_call: &ToolCall) -> Result<()> { Ok(()) }

    /// 提供的 tools
    fn tools(&self) -> Vec<Box<dyn Tool>> { vec![] }

    /// 提供的 slash commands
    fn commands(&self) -> Vec<SlashCommand> { vec![] }
}

pub struct ExtensionContext {
    pub session: &AgentSession,  // 只读访问
    pub model_registry: &ModelRegistry,
    pub settings: &SettingsManager,
    // UI primitives（通过 trait object 注入）
    pub ui: Option<&dyn ExtensionUI>,
}
```

### resources — 资源加载

```rust
/// 加载项目/全局配置资源
pub struct ResourceLoader {
    cwd: PathBuf,
    config_dirs: Vec<PathBuf>, // global -> project
}

impl ResourceLoader {
    /// 加载 CLAUDE.md（项目指令）
    pub async fn load_project_instructions(&self) -> Result<Option<String>>;

    /// 加载 AGENTS.md
    pub async fn load_agents_instructions(&self) -> Result<Option<String>>;

    /// 加载自定义 system prompt 片段
    pub async fn load_system_prompt_fragments(&self) -> Result<Vec<String>>;

    /// 加载 extension manifests
    pub async fn load_extension_manifests(&self) -> Result<Vec<ExtensionManifest>>;
}
```

### compaction — 上下文压缩

```rust
/// 当上下文接近 context_window 时自动/手动压缩
pub struct CompactionEngine {
    stream_fn: Box<dyn StreamFn>,
    model: Model,
}

impl CompactionEngine {
    /// 生成上下文摘要，替换旧消息
    pub async fn compact(
        &self,
        messages: &[AgentMessage],
        options: &CompactionOptions,
    ) -> Result<CompactionResult>;

    /// 分支摘要（长子对话树归拢）
    pub async fn summarize_branch(
        &self,
        branch_messages: &[AgentMessage],
    ) -> Result<String>;
}

pub struct CompactionResult {
    pub summary: String,
    pub retained_messages: Vec<AgentMessage>,
    pub tokens_saved: usize,
}
```

### skills — Skill 匹配

```rust
pub struct Skill {
    pub name: String,
    pub description: String,
    pub trigger: SkillTrigger,
    pub prompt_template: String,
}

pub enum SkillTrigger {
    SlashCommand(String),
    Pattern(Regex),
    Manual,
}

pub struct SkillMatcher {
    skills: Vec<Skill>,
}

impl SkillMatcher {
    pub fn match_input(&self, input: &str) -> Option<&Skill>;
    pub fn expand_skill(&self, skill: &Skill, input: &str) -> String;
}
```

### model_registry — 模型发现

```rust
/// 运行时模型注册表（内置 + 用户自定义）
pub struct ModelRegistry {
    models: Vec<Model>,
    auth_storage: AuthStorage,
    provider_available_ids: HashMap<Provider, HashSet<String>>,
}

impl ModelRegistry {
    pub fn create(auth_storage: AuthStorage, models_json_path: Option<&Path>) -> Self;

    /// 重新加载（内置 + models.json）
    pub fn refresh(&mut self);

    /// 查找模型
    pub fn find(&self, query: &str) -> Option<&Model>;

    /// 列出某 provider 的可用模型
    pub fn list_by_provider(&self, provider: &Provider) -> Vec<&Model>;

    /// 探测 provider 可用性
    pub async fn probe_provider(&mut self, provider: &Provider) -> Result<Vec<String>>;
}
```

### runtime_state — 运行时状态

```rust
/// 供 TUI 消费的只读状态快照
pub struct RuntimeState {
    pub project: ProjectInfo,
    pub permission: PermissionInfo,
    pub model_usage: Usage,
    pub git_status: GitStatus,
    pub active_subagents: Vec<SubagentInfo>,
    pub changed_files: Vec<PathBuf>,
    pub tool_call_stats: Vec<ToolCallStat>,
    pub edit_mode: EditMode,
}
```

---

## rozsa-tui — ratatui 终端前端

依赖：rozsa-app, rozsa-model。

### 内部模块

```rust
// crates/rozsa-tui/src/
mod app;                         // App 主循环 (event loop + state)
mod protocol;                    // 协议定义 (HostMessage, ClientMessage)
mod ui;                          // 缓存基础设施 + render() 主入口 + layout + render_*
mod input;                       // InputState + handle_key + 鼠标事件
mod components;                  // 编辑器、侧边栏、选择器、权限、自动补全、会话树、历史图
mod backend;                     // AgentBackend trait + SocketBackend + MockBackend
mod command;                     // 命令系统 + 内置命令
mod theme;                       // 主题运行时管理 + 调色板
mod overlay;                     // Overlay 定位与焦点栈
mod keymap;                      // 快捷键绑定匹配
mod markdown;                    // Markdown 渲染 (语法高亮、图片、超链接)
mod highlight;                   // 代码语法高亮 (syntect/two-face)
mod hyperlink;                   // OSC 8 终端超链接
mod terminal_image;              // 终端图片协议 (Kitty/iTerm2)
mod terminal_caps;               // 终端能力检测
mod ansi;                        // ANSI SGR → ratatui Style
mod fuzzy;                       // Fuzzy 匹配评分
mod undo;                        // Undo 栈（编辑器撤销）
mod kill_ring;                   // Kill Ring（Emacs 剪切环）
```

### 核心结构

```rust
pub struct App {
    session: AgentSession,
    ui_state: UIState,
    editor: Editor,
    conversation: ConversationView,
    status_bar: StatusBar,
}

impl App {
    pub async fn run(session: AgentSession, terminal: &mut Terminal) -> Result<()> {
        // 1. 渲染初始 UI
        // 2. 进入事件循环
        //    - crossterm 键盘/鼠标事件 → input handler
        //    - AgentEvent → conversation/status_bar 更新
        //    - 定时 tick → animation (spinner 等)
        // 3. 退出清理
    }
}
```

---

## rozsa-cli — binary 入口

依赖：rozsa-app, rozsa-tui。

### 内部模块

```rust
// crates/rozsa-cli/src/
mod args;            // clap 参数定义
mod run;             // 运行模式分发
```

### main.rs

```rust
fn main() -> Result<()> {
    let args = Args::parse();

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        // 1. 解析 CLI 参数 → CreateSessionOptions
        // 2. create_session(options) → AgentSessionRuntime
        // 3. 根据模式启动
        match args.mode {
            Mode::Interactive => {
                let terminal = setup_terminal()?;
                App::run(runtime.session, &mut terminal).await?;
                restore_terminal(terminal)?;
            }
            Mode::Print => {
                // 非交互：直接输出到 stdout
                print_mode::run(runtime.session, &args.prompt).await?;
            }
        }
        Ok(())
    })
}
```

---

## 设计原则

### core 的边界判断标准

> "如果有人用 rozsa-core 构建一个完全不同的 agent（比如数据分析 agent），他需要这个模块吗？"
>
> 需要 → core；不需要 → app 层。

### app 为什么不再拆分

Crate 边界在 Rust 里是重的（独立编译单元、显式 pub API、版本号）。app 内部的 tools、permissions、extensions、settings 虽然职责不同，但高度围绕同一个消费者（AgentSession）。用 `mod` + 可见性控制（`pub(crate)`, `pub(super)`）即可实现内部边界，无需拆 crate。

### 什么时候拆出新 crate

当出现**第二个消费者**需要独立依赖某个子模块时。例如 rozsa-tui 需要直接用 permissions 做 UI 展示但不想依赖整个 app — 那时再抽出 `rozsa-permissions`。

---

## TS → Rust 映射

| TS (packages/) | Rust (crates/) | 说明 |
|---|---|---|
| ai/ | rozsa-model | 1:1 |
| agent/ | rozsa-core | 纯引擎部分 |
| coding-agent/src/core/ | rozsa-app | 应用逻辑 |
| coding-agent/src/modes/ | rozsa-tui + rozsa-cli | interactive→tui, 入口→cli |
| tui/ (TS) | 废弃 | 被 rozsa-tui 替代 |
| tui-rs/ | rozsa-tui | 直接升级迁入 |

---

## 过渡策略

1. **Phase 1: rozsa-model** — 从依赖树叶子开始。过渡期通过子进程 + JSON-RPC 桥接 TS agent 层。
2. **Phase 2: rozsa-core** — agent 引擎 Rust 化。
3. **Phase 3: rozsa-app** — 应用层迁移，TS coding-agent 逐步废弃。
4. **Phase 4: rozsa-tui + rozsa-cli** — 整合 tui-rs，去掉 IPC，全部同进程。

每个 phase 完成后，对应的 TS package 标记 deprecated 并最终删除。

---

## 关键 Rust 依赖

| 用途 | crate |
|------|-------|
| 异步运行时 | tokio |
| HTTP 客户端 | reqwest |
| SSE 解析 | eventsource-stream 或手写 |
| JSON | serde, serde_json |
| CLI 参数 | clap |
| 终端 UI | ratatui, crossterm |
| 正则 | regex |
| 文件监听 | notify |
| UUID | uuid |
| 路径处理 | std::path + dirs |
| 日志 | tracing |
| 错误处理 | anyhow / thiserror |
| 取消信号 | tokio_util::sync::CancellationToken |
| JSON Schema 验证 | jsonschema |
