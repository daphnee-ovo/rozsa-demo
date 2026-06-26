# rozsa-model — LLM Provider Abstraction

## 概述

`rozsa-model` crate 提供了与多家 LLM 提供商（Anthropic、OpenAI、Amazon Bedrock 等）交互的统一抽象层。核心职责包括：

- **Provider-agnostic 消息流接口**：统一的 `StreamEvent` 流式传输协议，屏蔽各厂商 API 差异
- **类型安全的消息体系**：`Message`、`ContentBlock`、`ToolCall` 等标准化数据结构
- **Provider 动态注册**：基于 `ApiProvider` trait 的插件式实现
- **凭证管理**：支持 API key、OAuth 2.0（含自动刷新）、企业认证
- **模型元数据**：成本、上下文窗口、推理能力等元信息

`rozsa-model` 不直接暴露网络层实现细节，而是通过 `Model` 元数据 + `Api` 协议枚举路由到对应的 provider 实现。

---

## 模块结构

```
crates/rozsa-model/src/
├── lib.rs                    # Public module exports
├── types.rs                  # 核心类型定义（Model、Message、StreamEvent 等）
├── types_serde.rs            # 私有序列化实现
├── registry.rs               # ApiProvider trait + 全局 provider 注册表
├── stream.rs                 # 公共流式接口（stream / stream_simple）
├── event_stream.rs           # 内部无边界事件流（EventStream / EventStreamSender）
├── credentials.rs            # 凭证解析、OAuth token 刷新、auth.json 管理
├── env_keys.rs               # 环境变量键名常量
├── oauth/                    # OAuth 2.0 登录流程实现
│   ├── types.rs              # OAuthCredentials、OAuthFlowEvent
│   ├── pkce.rs               # PKCE (Proof Key for Code Exchange) 实现
│   ├── device_code.rs        # Device Code Flow (RFC 8628)
│   ├── callback_server.rs    # 本地 HTTP callback server（用于 Authorization Code Flow）
│   ├── anthropic.rs          # Anthropic OAuth login
│   ├── openai_codex.rs       # OpenAI Codex OAuth login
│   └── github_copilot.rs     # GitHub Copilot OAuth login
└── providers/                # 各 provider 的具体实现
    ├── mod.rs                # register_builtin_providers()
    ├── common.rs             # Provider 共享工具（provider_id 等）
    ├── anthropic/            # Anthropic Messages API
    ├── openai_completions/   # OpenAI Completions API
    └── bedrock/              # AWS Bedrock Converse Stream
```

---

## 核心类型

### Model

模型元数据，用于路由请求到对应 provider 并计算 token 成本：

```rust
pub struct Model {
    pub id: String,                    // 模型标识符（如 "claude-opus-4"）
    pub name: String,                  // 显示名称
    pub api: Api,                      // 协议类型（AnthropicMessages / OpenAICompletions 等）
    pub provider: Provider,            // Provider 标识（Anthropic / OpenAI 等）
    pub base_url: String,              // API endpoint base URL
    pub reasoning: bool,               // 是否支持 extended thinking
    pub input_modalities: Vec<InputModality>,  // 支持的输入类型（Text / Image）
    pub cost: ModelCost,               // 每百万 token 成本
    pub context_window: usize,         // 上下文窗口大小
    pub max_tokens: usize,             // 最大输出 tokens
    pub thinking_level_map: Option<HashMap<ThinkingLevel, Option<String>>>,  // 推理级别映射
    pub headers: Option<HashMap<String, String>>,  // 自定义 HTTP headers
    pub compat: Option<serde_json::Value>,  // Provider 兼容性选项
}

pub struct ModelCost {
    pub input: f64,       // 输入成本（$/M tokens）
    pub output: f64,      // 输出成本
    pub cache_read: f64,  // 缓存读成本
    pub cache_write: f64, // 缓存写成本
}
```

### Api / Provider

**`Api`** 是协议族，决定使用哪个 streaming 实现：

```rust
pub enum Api {
    AnthropicMessages,        // Anthropic Messages API
    OpenAICompletions,        // OpenAI /v1/chat/completions
    OpenAIResponses,          // OpenAI Responses API（实时音视频）
    BedrockConverseStream,    // AWS Bedrock Converse Stream
    GoogleGenerativeAI,       // Google Gemini API
    GoogleVertex,             // Google Vertex AI
    MistralConversations,     // Mistral API
    Custom(String),           // 自定义协议
}
```

**`Provider`** 是具体 provider 身份，用于凭证解析和兼容性规则：

```rust
pub enum Provider {
    Anthropic, OpenAI, AmazonBedrock, Google, GoogleVertex,
    DeepSeek, OpenRouter, XAI, Groq, Cerebras, Mistral,
    Nvidia, Zai, Together, MoonshotAI, MoonshotAICn,
    HuggingFace, CloudflareWorkersAI, CloudflareAIGateway,
    Xiaomi, XiaomiTokenPlanCn, XiaomiTokenPlanAms, XiaomiTokenPlanSgp,
    Custom(String),
}
```

同一个 `Api` 可对应多个 `Provider`（如 OpenRouter / Groq / Together 都可用 `OpenAICompletions`）。

---

### Message 体系

对话历史由 `Message` 枚举组成：

```rust
pub enum Message {
    User(UserMessage),          // 用户消息
    Assistant(AssistantMessage), // 模型回复
    ToolResult(ToolResultMessage), // 工具执行结果
}

pub struct UserMessage {
    pub content: UserContent,            // 文本或多模态 blocks
    pub display_text: Option<String>,    // 可选的显示文本（用于 UI）
    pub timestamp: i64,
}

pub struct AssistantMessage {
    pub content: Vec<ContentBlock>,       // 消息内容块
    pub api: Api,
    pub provider: Provider,
    pub model: String,
    pub response_model: Option<String>,   // 实际响应的模型 ID（如果不同）
    pub response_id: Option<String>,      // Provider 的响应 ID
    pub usage: Usage,                     // Token 使用量
    pub stop_reason: StopReason,          // 停止原因
    pub error_message: Option<String>,    // 错误信息
    pub timestamp: i64,
}

pub struct ToolResultMessage {
    pub tool_call_id: String,
    pub tool_name: String,
    pub content: Vec<ContentBlock>,  // 工具返回的内容
    pub is_error: bool,
    pub timestamp: i64,
}
```

---

### ContentBlock

消息内容块，支持文本、思考、图片、工具调用：

```rust
pub enum ContentBlock {
    Text {
        text: String,
        signature: Option<String>,  // 可选的加密签名（防篡改）
    },
    Thinking {
        thinking: String,           // Extended thinking 内容
        signature: Option<String>,
        redacted: bool,             // 是否已脱敏
    },
    Image {
        data: String,               // Base64 编码的图片数据
        mime_type: String,          // 如 "image/png"
    },
    ToolCall(ToolCall),             // 模型请求调用工具
}

pub struct ToolCall {
    pub id: String,                      // 工具调用 ID
    pub name: String,                    // 工具名称
    pub arguments: serde_json::Value,    // 工具参数（JSON）
}
```

---

### UserContent

用户消息内容，支持纯文本或多模态：

```rust
pub enum UserContent {
    Text(String),               // 纯文本
    Blocks(Vec<ContentBlock>),  // 多模态（文本 + 图片）
}

impl UserContent {
    /// 提取纯文本（忽略图片、ToolCall 等）
    pub fn text(&self) -> String;
}
```

- `UserContent::Text("hello")` — 纯文本消息
- `UserContent::Blocks(vec![ContentBlock::Text{..}, ContentBlock::Image{..}])` — 文本 + 图片
- `text()` 方法统一提取文本部分（多个文本块以换行连接）

---

### StreamEvent

Provider 流式传输事件：

```rust
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    Start { partial: AssistantMessage },                        // 流开始
    TextStart { content_index: usize, partial: AssistantMessage },
    TextDelta { content_index: usize, delta: String, partial: AssistantMessage },
    TextEnd { content_index: usize, content: String, partial: AssistantMessage },
    ThinkingStart { content_index: usize, partial: AssistantMessage },
    ThinkingDelta { content_index: usize, delta: String, partial: AssistantMessage },
    ThinkingEnd { content_index: usize, content: String, partial: AssistantMessage },
    ToolCallStart { content_index: usize, partial: AssistantMessage },
    ToolCallDelta { content_index: usize, delta: String, partial: AssistantMessage },
    ToolCallEnd { content_index: usize, tool_call: ToolCall, partial: AssistantMessage },
    Done { reason: StopReason, message: AssistantMessage },     // 成功完成
    Error { reason: StopReason, error: AssistantMessage },      // 错误
}
```

- `content_index`：当前 content block 在 `partial.content` 中的索引
- `partial`：流式过程中的部分消息（实时更新）
- `Done` / `Error`：流结束，包含完整的 `AssistantMessage`

---

### ThinkingLevel / StopReason

```rust
pub enum ThinkingLevel {
    Off, Minimal, Low, Medium, High, XHigh,
}

pub enum StopReason {
    Stop,      // 正常停止
    Length,    // 达到 max_tokens
    ToolUse,   // 请求调用工具
    Error,     // 发生错误
    Aborted,   // 用户中止
}
```

---

### ToolSchema

工具定义（JSON Schema）：

```rust
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,  // JSON Schema object
}
```

---

### Usage / UsageCost

Token 使用统计：

```rust
pub struct Usage {
    pub input: u64,         // 输入 tokens
    pub output: u64,        // 输出 tokens
    pub cache_read: u64,    // 缓存读 tokens
    pub cache_write: u64,   // 缓存写 tokens
    pub total_tokens: u64,
    pub cost: UsageCost,    // 本次请求的货币成本
}

pub struct UsageCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub total: f64,         // 美元总成本
}
```

---

### StreamOptions / SimpleStreamOptions

请求选项：

```rust
pub struct StreamOptions {
    pub temperature: Option<f64>,
    pub max_tokens: Option<usize>,
    pub api_key: Option<String>,
    pub transport: Transport,             // Sse / WebSocket / Auto
    pub cache_retention: CacheRetention,  // None / Short / Long
    pub session_id: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    pub max_retries: Option<u32>,
    pub max_retry_delay_ms: Option<u64>,
    pub metadata: Option<serde_json::Value>,
}

pub struct SimpleStreamOptions {
    #[serde(flatten)]
    pub base: StreamOptions,
    pub reasoning: Option<ThinkingLevel>,           // 统一推理控制
    pub thinking_budgets: Option<ThinkingBudgets>,  // 推理 token 预算
    pub tool_choice: Option<serde_json::Value>,
}

pub enum Transport {
    Sse, WebSocket, WebSocketCached, Auto,
}

pub enum CacheRetention {
    None, Short, Long,
}
```

---

## Provider 实现

### ApiProvider Trait

Provider 实现需实现此 trait：

```rust
pub trait ApiProvider: Send + Sync {
    fn api(&self) -> &Api;
    fn stream(&self, model: &Model, context: &Context, options: &StreamOptions) -> EventStream<StreamEvent>;
    fn stream_simple(&self, model: &Model, context: &Context, options: &SimpleStreamOptions) -> EventStream<StreamEvent>;
}
```

### 内置 Providers

当前已实现的 providers（通过 `register_builtin_providers()` 注册）：

1. **`anthropic::AnthropicProvider`** — Anthropic Messages API (`Api::AnthropicMessages`)
2. **`openai_completions::OpenAICompletionsProvider`** — OpenAI Chat Completions API (`Api::OpenAICompletions`)
3. **`bedrock::BedrockProvider`** — AWS Bedrock Converse Stream (`Api::BedrockConverseStream`)

Provider 模块内部结构（以 `anthropic` 为例）：

```
providers/anthropic/
├── mod.rs       # AnthropicProvider 实现（ApiProvider trait）
├── payload.rs   # 请求 payload 构建（messages、tools、thinking）
└── stream.rs    # SSE 流解析（将 Anthropic events 转换为 StreamEvent）
```

---

### EventStream

内部流式传输接口：

```rust
pub struct EventStream<T> {
    rx: mpsc::UnboundedReceiver<T>,
}

impl<T> EventStream<T> {
    pub async fn next(&mut self) -> Option<T>;
}

pub struct EventStreamSender<T> {
    tx: mpsc::UnboundedSender<T>,
}

impl<T> EventStreamSender<T> {
    pub fn push(&self, event: T);
}

pub fn create_event_stream<T>() -> (EventStreamSender<T>, EventStream<T>);
```

Provider 实现通常：
1. 调用 `create_event_stream()` 创建发送/接收对
2. Spawn 异步任务处理 SSE / WebSocket 流
3. 通过 `EventStreamSender::push()` 推送 `StreamEvent`
4. 返回 `EventStream` 给调用方

---

## OAuth 模块

支持三种 OAuth 登录流程：

### 1. Anthropic OAuth (`oauth::anthropic`)

```rust
pub async fn login_anthropic(
    event_tx: tokio::sync::mpsc::UnboundedSender<OAuthFlowEvent>,
) -> OAuthLoginResult;
```

- 使用 **Authorization Code Flow + PKCE**
- 本地启动 HTTP callback server（`http://localhost:8341/callback`）
- 发送 `OAuthFlowEvent::AuthUrl` 让用户浏览器授权
- 接收 authorization code 后交换 access + refresh token

### 2. OpenAI Codex OAuth (`oauth::openai_codex`)

```rust
pub async fn login_openai_codex(
    event_tx: tokio::sync::mpsc::UnboundedSender<OAuthFlowEvent>,
) -> OAuthLoginResult;
```

- 使用 **Device Code Flow**（RFC 8628）
- 发送 `OAuthFlowEvent::DeviceCode` 显示 user_code 和 verification_uri
- 轮询 token endpoint 直到用户完成授权

### 3. GitHub Copilot OAuth (`oauth::github_copilot`)

```rust
pub async fn login_github_copilot(
    event_tx: tokio::sync::mpsc::UnboundedSender<OAuthFlowEvent>,
) -> OAuthLoginResult;
```

- **Device Code Flow** + enterprise URL 支持
- 可选 `OAuthFlowEvent::Prompt` 让用户输入企业域名

### OAuth 数据类型

```rust
pub struct OAuthCredentials {
    pub access: String,
    pub refresh: String,
    pub expires: i64,  // 过期时间（毫秒时间戳）
    pub extra: HashMap<String, serde_json::Value>,
}

pub enum OAuthFlowEvent {
    AuthUrl { url: String, instructions: Option<String> },
    DeviceCode { user_code: String, verification_uri: String },
    Prompt { message: String, placeholder: Option<String> },
    Select { message: String, options: Vec<String> },
    Progress { message: String },
    Waiting { message: String },
}
```

### 凭证存储与刷新

`credentials.rs` 提供 `auth.json` 管理：

```rust
// 存储 OAuth 凭证
pub fn store_oauth_credentials(
    path: &str,
    provider: &str,
    credentials: &OAuthCredentials,
) -> Result<(), String>;

// 解析请求选项（自动刷新过期 token）
pub async fn resolve_request_options(
    model: &Model,
    options: &SimpleStreamOptions,
    models_json_path: Option<&str>,
    auth_json_path: Option<&str>,
) -> Result<SimpleStreamOptions, String>;
```

**自动 token 刷新规则**：
- `resolve_request_options` 检测 `auth.json` 中的 `expires` 字段
- 如已过期，调用对应 provider 的 refresh endpoint（Anthropic / OpenAI Codex / GitHub Copilot）
- 更新后的 token 写回 `auth.json`（带文件锁防止并发冲突）

---

## 与其他 crate 的关系

```
rozsa-cli
   ↓
rozsa-tui
   ↓
rozsa-app ──→ rozsa-model ──→ (HTTP/SSE 网络层)
   ↓              ↓
rozsa-core ←──────┘
```

- **`rozsa-core`** 依赖 `rozsa-model` 的 `types` 和 `event_stream`（Agent 运行时消费 `StreamEvent`）
- **`rozsa-app`** 依赖 `rozsa-model` 的 `registry` 和 `credentials`（初始化 providers、解析凭证）
- **`rozsa-tui` / `rozsa-cli`** 间接通过 `rozsa-app` 使用 `rozsa-model`

职责边界：
- `rozsa-model` — 纯 LLM API 交互层（不涉及 agent 逻辑、TUI 渲染、CLI 参数解析）
- `rozsa-core` — Agent 循环、工具调用、上下文管理
- `rozsa-app` — Session 管理、权限控制、技能加载
- `rozsa-tui` / `rozsa-cli` — 用户界面

---

## 使用示例

### 1. 注册 Provider

```rust
use rozsa_model::providers::register_builtin_providers;

fn main() {
    register_builtin_providers();  // 注册 Anthropic / OpenAI / Bedrock
    // 或手动注册自定义 provider：
    // rozsa_model::registry::register_provider(Box::new(MyCustomProvider));
}
```

### 2. 发起流式请求

```rust
use rozsa_model::{stream, types::*};

#[tokio::main]
async fn main() {
    let model = Model {
        id: "claude-opus-4".to_string(),
        name: "Claude Opus 4".to_string(),
        api: Api::AnthropicMessages,
        provider: Provider::Anthropic,
        base_url: "https://api.anthropic.com/v1".to_string(),
        reasoning: true,
        input_modalities: vec![InputModality::Text, InputModality::Image],
        cost: ModelCost { input: 15.0, output: 75.0, cache_read: 1.5, cache_write: 18.75 },
        context_window: 200_000,
        max_tokens: 16_384,
        thinking_level_map: None,
        headers: None,
        compat: None,
    };

    let context = Context {
        system_prompt: Some("You are a helpful assistant.".to_string()),
        messages: vec![Message::User(UserMessage {
            content: UserContent::Text("Explain photosynthesis in one sentence.".to_string()),
            display_text: None,
            timestamp: 0,
        })],
        tools: vec![],
    };

    let options = SimpleStreamOptions {
        base: StreamOptions {
            temperature: Some(1.0),
            max_tokens: Some(1024),
            api_key: Some(std::env::var("ANTHROPIC_API_KEY").unwrap()),
            transport: Transport::Auto,
            cache_retention: CacheRetention::Short,
            session_id: None,
            headers: None,
            timeout_ms: None,
            max_retries: None,
            max_retry_delay_ms: None,
            metadata: None,
        },
        reasoning: Some(ThinkingLevel::Low),
        thinking_budgets: None,
        tool_choice: None,
    };

    let mut stream = rozsa_model::stream::stream_simple(&model, &context, &options);

    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::TextDelta { delta, .. } => print!("{}", delta),
            StreamEvent::Done { message, .. } => {
                println!("\n[Done] Tokens: {} in / {} out", message.usage.input, message.usage.output);
                println!("Cost: ${:.4}", message.usage.cost.total);
            }
            StreamEvent::Error { error, .. } => {
                eprintln!("Error: {:?}", error.error_message);
            }
            _ => {}
        }
    }
}
```

### 3. OAuth 登录

```rust
use rozsa_model::oauth::{anthropic::login_anthropic, types::*};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::unbounded_channel();

    let handle = tokio::spawn(async move {
        login_anthropic(tx).await
    });

    // 处理登录流程事件
    while let Some(event) = rx.recv().await {
        match event {
            OAuthFlowEvent::AuthUrl { url, .. } => {
                println!("请访问: {}", url);
            }
            OAuthFlowEvent::Progress { message } => {
                println!("{}", message);
            }
            _ => {}
        }
    }

    match handle.await.unwrap() {
        Ok(credentials) => {
            println!("登录成功！Access token: {}", credentials.access);
            // 存储到 auth.json
            rozsa_model::credentials::store_oauth_credentials(
                "auth.json",
                "anthropic",
                &credentials,
            ).unwrap();
        }
        Err(e) => eprintln!("登录失败: {}", e),
    }
}
```

### 4. 工具调用

```rust
let tools = vec![ToolSchema {
    name: "get_weather".to_string(),
    description: "Get the current weather for a city".to_string(),
    parameters: serde_json::json!({
        "type": "object",
        "properties": {
            "city": { "type": "string", "description": "City name" }
        },
        "required": ["city"]
    }),
}];

let context = Context {
    system_prompt: Some("You are a helpful assistant.".to_string()),
    messages: vec![Message::User(UserMessage {
        content: UserContent::Text("What's the weather in Paris?".to_string()),
        display_text: None,
        timestamp: 0,
    })],
    tools,
};

let mut stream = rozsa_model::stream::stream_simple(&model, &context, &options);

while let Some(event) = stream.next().await {
    match event {
        StreamEvent::ToolCallEnd { tool_call, .. } => {
            println!("Tool call: {} with args: {:?}", tool_call.name, tool_call.arguments);
            // 执行工具，然后发送 ToolResultMessage
        }
        StreamEvent::Done { .. } => break,
        _ => {}
    }
}
```

---

## 扩展指南

### 添加新 Provider

1. 在 `providers/` 下创建新模块（如 `my_provider/`）
2. 实现 `ApiProvider` trait：

```rust
use crate::registry::ApiProvider;
use crate::types::*;
use crate::event_stream::{create_event_stream, EventStream};

pub struct MyProvider;

impl ApiProvider for MyProvider {
    fn api(&self) -> &Api {
        &Api::Custom("my_api".to_string())
    }

    fn stream(&self, model: &Model, context: &Context, options: &StreamOptions) -> EventStream<StreamEvent> {
        let (tx, rx) = create_event_stream();
        tokio::spawn(async move {
            // 1. 构建 HTTP 请求
            // 2. 解析 SSE / WebSocket 流
            // 3. 转换为 StreamEvent 并 push 到 tx
            tx.push(StreamEvent::Start { partial: /* ... */ });
            tx.push(StreamEvent::TextDelta { /* ... */ });
            tx.push(StreamEvent::Done { /* ... */ });
        });
        rx
    }

    fn stream_simple(&self, model: &Model, context: &Context, options: &SimpleStreamOptions) -> EventStream<StreamEvent> {
        // 转换 SimpleStreamOptions 为 StreamOptions，调用 stream()
        self.stream(model, context, &options.base)
    }
}
```

3. 在 `providers/mod.rs` 中注册：

```rust
pub fn register_builtin_providers() {
    register_provider(Box::new(MyProvider));
    // ...
}
```

---

## 参考

- **源码**：`crates/rozsa-model/src/`
- **测试**：`crates/rozsa-model/tests/`
- **Rust 迁移决策**：`docs/RUST_DIFF_DECISIONS.md`
