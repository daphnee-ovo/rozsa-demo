# 头脑风暴记录 — Bedrock Converse Stream Provider

**日期**：2026-06-05

## 背景与目的

rozsa-model 迁移计划中 Bedrock 排第 2 位。AWS 有官方 Rust SDK `aws-sdk-bedrockruntime`，支持 ConverseStream API。目标是在 Rust 侧实现完整的 Bedrock provider，与现有 `OpenAICompletionsProvider` 同级，注册为 `Api::BedrockConverseStream` 的处理器。

## 关键决策

| 决策点 | 选择 | 理由 |
|--------|------|------|
| Credential 策略 | aws-config default chain + bearer token + skip-auth | 覆盖所有认证场景（env vars, profile, IAM role, ECS），额外代码量小 |
| Region 解析 | 完全依赖 aws-config default chain，不解析 base_url | SDK 已处理 env > profile > IMDS，无需手动实现 |
| Cache points | 第一版支持 | 逻辑不复杂，对成本影响大，SDK 已支持 CachePoint |
| Thinking/Reasoning | 全部支持（adaptive + budget-based + non-Claude） | 主力模型都需要 thinking 功能 |
| HTTP Proxy | 第一版不支持 | 企业代理场景少，后续可通过 HttpConnector override 加入 |

## 设计方案

### 架构

在 `crates/rozsa-model/src/providers/` 下新建 `bedrock/` 模块：

```
providers/
├── bedrock/
│   ├── mod.rs          — BedrockProvider + ApiProvider impl
│   ├── payload.rs      — Context → ConverseStreamInput 转换
│   └── stream.rs       — ConverseStream 事件 → StreamEvent 归一化
├── openai_completions/
│   └── ...
└── mod.rs              — register_builtin_providers() 加入 bedrock
```

### 组件

**BedrockProvider**（`mod.rs`）
- 实现 `ApiProvider` trait（`api()`, `stream()`, `stream_simple()`）
- `stream_simple()` 处理 reasoning 级别到 thinking 参数的映射
- 构建 `BedrockRuntimeClient`（credential + region）

**Payload 构建**（`payload.rs`）
- `convert_messages()` — 将 `Context.messages` 转为 Bedrock `Message` 格式
- `build_system_prompt()` — system prompt + cache point
- `convert_tool_config()` — tools → `ToolConfiguration`
- `build_additional_model_request_fields()` — thinking 参数（adaptive / budget-based）
- Cache point 插入逻辑（system prompt 末尾 + 最后 user message 末尾）

**Stream 解析**（`stream.rs`）
- 消费 `ConverseStreamOutput` 事件流
- 映射到统一 `StreamEvent` 枚举
- 处理 thinking signature 回传（仅 Claude）
- 处理 tool call 的 JSON 增量解析

### 数据流

```
Context + SimpleStreamOptions
  → stream_simple() 解析 reasoning level
  → 构建 ConverseStreamCommand（payload.rs）
  → client.send(command) 获取 event stream
  → 逐事件解析（stream.rs）
  → 发送归一化 StreamEvent 到 EventStreamSender
  → 最终 Done/Error 事件关闭流
```

### 错误处理

- Credential 加载失败 → `ProviderError::MissingApiKey`
- SDK exceptions（Throttling, Validation, InternalServer 等）→ `StreamEvent::Error`，error_message 带人类可读前缀
- Stream 中途异常 → 清理 partial blocks，设置 `StopReason::Error`

### 依赖新增

workspace `Cargo.toml`：
- `aws-config` — default credential chain + region
- `aws-sdk-bedrockruntime` — ConverseStream API
- `aws-smithy-types` — Document type（tool input schema）

### Stream 事件映射

| Bedrock SDK 事件 | StreamEvent |
|---|---|
| `messageStart` | `Start` |
| `contentBlockStart(toolUse)` | `ToolCallStart` |
| `contentBlockDelta(text)` | `TextStart`（首次）+ `TextDelta` |
| `contentBlockDelta(reasoningContent)` | `ThinkingStart`（首次）+ `ThinkingDelta` |
| `contentBlockDelta(toolUse.input)` | `ToolCallDelta` |
| `contentBlockStop` | `TextEnd` / `ThinkingEnd` / `ToolCallEnd` |
| `metadata` | 更新 Usage + cost 计算 |
| `messageStop` | `Done` |
| exceptions | `Error` |

### Thinking 策略

| 模型类型 | 方式 |
|---|---|
| Claude Opus 4.6+, Sonnet 4.6 | adaptive thinking + effort level |
| Claude 3.7 Sonnet 等 | budget-based thinking + budget_tokens |
| Non-Claude (Nova 等) | 透传 reasoning 参数 |

附加参数：
- `thinkingDisplay`：默认 `"summarized"`
- `interleavedThinking`：非 adaptive 模型默认开启
- thinking signature：仅 Claude 模型回传

### Cache Points 策略

- 仅对 Claude 3.5 Haiku / 3.7 Sonnet / 4.x 生效
- System prompt 末尾插入 `CachePoint`
- 最后一条 user message 末尾插入 `CachePoint`
- `CacheRetention::Long` → `ttl: ONE_HOUR`
- 控制来源：`StreamOptions.cache_retention` 或 `ROZSA_CACHE_RETENTION` 环境变量

## 约束与边界

- 不支持 HTTP proxy（第一版）
- 不支持 `onPayload` / `onResponse` 回调（需桥协议扩展）
- 不解析 model.base_url 中的 region
- 不处理 GovCloud FIPS endpoint
- 不处理自定义 endpoint override
- 不支持 `AWS_BEDROCK_FORCE_HTTP1`（Rust SDK 默认行为已满足）

## 下一步

设计已足够清晰，建议直接进入 `/spec` 出技术规范，然后 `/task` 拆分实现任务。
