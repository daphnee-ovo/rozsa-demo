# rozsa-core 迁移计划

本文定义从 TypeScript agent core 迁移到 Rust rozsa-core 的最小可行计划。

这里的 core 指 agent package 的运行核心。它不是 coding-agent 的整个 core 目录。coding-agent core 现在是应用运行时，包含 settings、session、permissions、extensions、tools、compaction、subagent、LSP 和 UI 状态。直接整体迁移会变成跨模块重写，不适合作为 core 迁移起点。

## 目标

第一阶段目标不是删除 TypeScript API，而是在保持 Agent public API 不变的前提下，把内部 loop 从 TypeScript 切到 Rust。

最终运行方向：

```text
TypeScript AgentSession host
  -> TypeScript Agent API shell
    -> Rust core backend
      -> rozsa-core agent loop
      -> rozsa-model stream
      -> TypeScript tool host
```

必须保持：

- Agent prompt 行为
- Agent continue 行为
- Agent steer 和 follow-up 行为
- AgentEvent 事件顺序
- tool 调度语义
- permission 和 extension 安全边界
- abort 和 error 行为

## 范围

本次迁移包含：

- agent loop 状态机
- prompt 和 continue 生命周期
- turn 生命周期
- model stream 消费
- assistant partial message 更新
- tool call 调度
- tool result message 构造
- steering queue
- follow-up queue
- turn 后模型和 thinking 状态更新
- stop 和 abort 行为
- AgentEvent 发射顺序
- TypeScript backend 与 Rust backend 的显式切换

本次迁移不包含：

- provider 协议实现
- model registry
- provider availability
- AgentSession 整体重写
- settings manager
- session JSONL 持久化
- permission 规则系统重写
- extension runtime
- built-in tools 的 Rust 重写
- TypeBox 替换
- LSP
- TUI protocol
- OAuth interactive flow
- compaction summary 生成
- subagent orchestration

这些不迁不是因为不重要，而是它们不是最小 core。先迁它们会扩大风险。

## 当前事实

当前 TypeScript core 已经有一个清楚的低层入口：

- agent loop 模块：低层 agent loop。
- agent public API 模块：Agent API 和 run lifecycle。
- agent types 模块：Agent state、tool、config、event contract。
- tool validation 模块：tool 参数校验。
- agent harness 模块：非 UI harness，也依赖同一套 loop。

当前 coding-agent runtime 是更高一层：

- sdk 模块创建 Agent，注入 model stream、tools、settings、session。
- agent session 模块包装 Agent，处理 permission、extension、runtime state、compaction、retry、bash、subagent、session navigation、export。

当前 Rust core 状态：

- rozsa-core 已有 agent、agent loop、config、events、messages、tool 模块。
- Rust agent loop 仍是 TODO 骨架。
- Rust 类型方向是对的，但还不能直接作为 bridge runtime 使用。
- 自定义 message 还不是稳定可序列化边界。
- config 里的闭包模型需要改成 async host boundary。

## 迁移参考点

迁移不是重写一套相似功能，而是逐项对齐现有行为。每个任务必须先找参考点，再实现 Rust 等价，再做优化。

### TypeScript 行为参考点

| 参考点 | 当前职责 | 迁移时必须保留 |
| --- | --- | --- |
| TS agent loop 模块 | agent run 主状态机 | turn 顺序、message 顺序、tool 顺序、queue 顺序 |
| TS Agent public API 模块 | prompt、continue、steer、follow-up、abort、state、subscribe | public API 不变，外部调用方不改 |
| TS Agent types 模块 | AgentEvent、AgentTool、AgentLoopConfig、AgentState | 事件字段、hook 入参、tool result 语义 |
| TS tool validation 模块 | TypeBox 和 JSON schema 参数校验 | 第一阶段继续由 TS host 执行 |
| TS Agent harness 模块 | 非 UI agent 运行环境 | harness prompt、session write、hook 行为 |
| coding-agent sdk 模块 | 创建 Agent 并注入 model stream 与 tools | createAgentSession 不大改 |
| coding-agent agent session 模块 | permission、extension、runtime state、compaction、subagent | 第一阶段保持 TS host |
| rozsa-model Rust types | Model、Context、Message、ToolSchema、StreamEvent | Rust core 复用，不再创建平行类型 |
| rozsa-model bridge 经验 | JSON Lines、child process、fail fast | core bridge 复用同类策略 |

### Rust 落点参考点

| Rust 落点 | 当前状态 | 目标 |
| --- | --- | --- |
| rozsa-core agent 模块 | AgentState 骨架 | 承载 Rust runtime state |
| rozsa-core agent loop 模块 | TODO 骨架 | 承载 prompt、continue、turn loop |
| rozsa-core events 模块 | AgentEvent 骨架 | 与 TS AgentEvent 等价并可序列化 |
| rozsa-core messages 模块 | AgentMessage 骨架 | 支持 standard message 和 opaque custom message |
| rozsa-core tool 模块 | Tool trait 骨架 | 承载调度需要的 tool metadata 和 result |
| rozsa-core queue 模块 | PendingMessageQueue 骨架 | 对齐 all 与 one-at-a-time |
| rozsa-core config 模块 | 同步闭包草图 | 改成 async runtime boundary |

### UI 和安全参考点

这些参考点第一阶段不迁，但必须验证不被破坏：

- permission request、reject、session approval 行为。
- extension tool call 和 tool result hooks。
- LSP 在 edit 类工具后的附加诊断。
- runtime state 里的 current model、tool 状态、permission 状态。
- session 持久化中的 message 写入顺序。
- native TUI 看到的 message、tool、queue、model 状态。

## 迁移后的优化点

迁移不是机械翻译。Rust core 稳定后，允许在不破坏外部行为的前提下做以下优化。

### 第一阶段允许优化

- 明确 backend：只保留 ts 和 rust 两种显式模式，不引入 auto。
- 明确错误边界：Rust bridge 启动失败、协议失败、host failure 都返回结构化错误。
- 明确事件结构：Rust AgentEvent 可序列化，字段名稳定。
- 明确 custom message 边界：Rust 不理解的 custom payload 只做 opaque transport。
- 明确 tool host 边界：Rust 只调度，TS 执行和审批。
- 明确 cancellation：model stream、tool host、bridge process 都有取消路径。
- 明确测试数据：fake model、fake tool、fixture event sequence 可复用。

### 第二阶段允许优化

- 将 session branch 构建、compaction prepare 这类纯数据逻辑迁入 Rust。
- 将 permission 中纯规则匹配部分迁入 Rust，但保留 user prompt 和 reviewer host。
- 将 runtime state 中纯快照计算迁入 Rust。
- 将 harness 的非 UI 能力作为 Rust core 的稳定测试入口。

### 第一阶段禁止优化

- 不重写 AgentSession。
- 不重写真实 built-in tools。
- 不替换 TypeBox。
- 不重写 extension runtime。
- 不改变 session file 格式。
- 不改变 native TUI 协议。
- 不改变 provider 路由。
- 不改变 OAuth interactive flow。
- 不改变用户可见命令。

## 完整性测试要求

迁移必须证明迁移后功能没有比迁移前变少。完整性测试分三层。

### 第一层：行为 parity 测试

这一层用 fake model 和 fake tools，不接真实 provider，不读写真工作区文件。

必须固定以下行为：

- prompt run 的完整事件顺序。
- continue run 的完整事件顺序。
- assistant stream partial 更新。
- stream done。
- stream error。
- stream abort。
- tool call start、update、end。
- sequential tool batch。
- parallel tool batch。
- unknown tool。
- blocked tool。
- validation failure。
- execute throw。
- after hook override。
- after hook throw。
- steering queue。
- follow-up queue。
- prepare next turn。
- should stop after turn。
- tool batch terminate。

每个 parity case 都要先跑 TS backend，保存 expected event sequence，再跑 Rust backend 对齐。

### 第二层：应用集成 smoke

这一层通过 coding-agent runtime 创建 AgentSession，验证 TS host 没被 Rust core 破坏。

必须覆盖：

- no-tool prompt。
- fake read-only tool。
- permission reject。
- extension tool call hook。
- extension tool result hook。
- LSP tool result append。
- abort during model stream。
- abort during tool execution。
- queue update。
- session message persistence。
- model switch 后继续 prompt。

### 第三层：运行入口 smoke

这一层验证用户实际启动路径。

必须覆盖：

- 主启动入口默认 backend。
- TS 启动入口强制 TS backend。
- Rust backend 缺 binary 时 fail fast。
- Rust bridge stdout 不混日志。
- cancel 后没有残留 run lock。
- native TUI 能看到 message 和 tool 状态。

### 完整性通过标准

每个默认切换前必须满足：

- TS parity suite 通过。
- Rust parity suite 通过。
- TS 和 Rust 的 event sequence 无差异，除非文档列出并经确认。
- coding-agent smoke 通过。
- harness smoke 通过。
- startup smoke 通过。
- npm run check 通过。
- Rust core tests 通过。
- 所有失败都能通过 ROZSA_CORE_BACKEND=ts 回滚。

## 迁移原则

- 先冻结 TypeScript 行为，再做 Rust 等价实现。
- 先迁纯状态机，再迁 host 能力。
- Rust core 调度工具，但不直接执行 Node 工具。
- TypeScript host 继续执行工具、权限检查、extension hooks、LSP hooks。
- backend 只允许显式 ts 或 rust。
- 不引入 auto。
- 不做 silent fallback。
- Rust backend 启动失败必须 fail fast。
- 每一步都有可单独验证的最小闭环。
- 每一步都能回滚到 TypeScript backend。

## Backend 切换

新增显式配置：

```text
ROZSA_CORE_BACKEND=ts
ROZSA_CORE_BACKEND=rust
```

规则：

- 缺省值按 rollout 阶段决定。
- 非法值直接报错。
- TS 启动脚本必须显式设置 ROZSA_CORE_BACKEND 为 ts。
- Rust backend 不可用时不能自动回到 TS。
- 报错必须包含 backend、bridge command、binary path、exit code 或 spawn error。

Rollout：

| 阶段 | 默认 backend | 说明 |
| --- | --- | --- |
| 实现期 | ts | 手动设置 rust 做 focused 验证 |
| 本地 dogfood | 主启动入口使用 rust | TS 启动入口保持 ts |
| release candidate | rust | 保留 ts 回滚 |
| 清理期 | rust | 稳定一个迭代后再删 TS loop |

## 目标架构

```text
coding-agent runtime
  createAgentSession
  AgentSession
  settings
  permissions
  extensions
  tools

agent package
  Agent public API
  AgentLoopBackend
    TsAgentLoopBackend
    RustAgentLoopBackend

Rust bridge
  JSON Lines protocol
  run lifecycle
  tool host request
  cancellation

rozsa-core
  agent loop
  event order
  tool scheduling
  queue draining

rozsa-model
  model stream
  provider adapters
```

## 最小可行任务拆分

每个任务都必须写清四件事：

- 参考点：迁移前以哪个现有实现为准。
- 迁移动作：本任务实际改变什么。
- 优化点：迁移后允许改进什么。
- 完整性测试：如何证明没有破坏迁移前行为。

如果某个任务不能回答这四件事，不进入开发。

### 任务参考点和优化点总表

| 任务 | 参考点 | 迁移动作 | 迁移后优化点 | 完整性测试 |
| --- | --- | --- | --- | --- |
| CORE-001 | model 迁移文档、Rust workspace 设计文档 | 建立 core 独立迁移文档 | 把 core 边界从 model 和 app 中拆清楚 | 文档审阅 |
| CORE-002 | TS agent loop、TS Agent types | 固定 TS parity fixtures | 将隐式行为变成可断言 fixtures | TS backend parity suite |
| CORE-003 | TS Agent types、Rust core skeleton、Rust model types | 整理 Rust 可序列化类型 | 消除不可传输 custom message 边界 | Rust serialize tests |
| CORE-004 | TS prompt run、Rust model StreamEvent | 实现 no-tool prompt loop | 把 stream 消费逻辑集中到 Rust | Rust no-tool parity |
| CORE-005 | TS continue run | 实现 Rust continue loop | fail fast 校验空 context 和 assistant 结尾 | Rust continue parity |
| CORE-006 | TS stream error 和 abort 行为 | 实现 Rust error 和 abort | 统一 cancellation token 边界 | Rust error and abort parity |
| CORE-007 | TS steering queue | 实现 Rust steering queue | queue mode 语义显式化 | Rust steering parity |
| CORE-008 | TS follow-up queue | 实现 Rust follow-up queue | follow-up 停止点显式化 | Rust follow-up parity |
| CORE-009 | TS prepare next turn hook | 实现 Rust next-turn update | model 和 thinking 更新边界显式化 | Rust next-turn parity |
| CORE-010 | TS should-stop hook | 实现 Rust should-stop hook | 停止顺序固定在 follow-up 前 | Rust should-stop parity |
| CORE-011 | TS tool lifecycle、Rust bridge 经验 | 定义 tool host protocol | tool 调度和 tool 执行解耦 | protocol parse tests |
| CORE-012 | TS tool validation、permission、extension hooks | 实现 TS tool host adapter | host failure 结构化 | TS host adapter tests |
| CORE-013 | TS sequential tool batch | 实现 Rust sequential scheduling | 顺序执行语义集中到 Rust | Rust sequential parity |
| CORE-014 | TS parallel tool batch | 实现 Rust parallel scheduling | end order 和 message order 分离可测 | Rust parallel parity |
| CORE-015 | TS terminate 逻辑 | 实现 Rust terminate 判断 | terminate 规则集中且可测 | Rust terminate parity |
| CORE-016 | TS Agent public API | 新增 backend abstraction | public API 与 runtime backend 解耦 | TS backend regression |
| CORE-017 | Rust model bridge、JSON Lines 经验 | 实现 Rust core bridge | stdout protocol-only，stderr logs-only | bridge protocol tests |
| CORE-018 | TS Rust model client | 实现 RustCoreClient | child lifecycle 和 pending request 显式化 | TS bridge client tests |
| CORE-019 | TS Agent backend abstraction | 接入 RustAgentLoopBackend | Rust mode fail fast，无 silent fallback | TS plus Rust backend smoke |
| CORE-020 | coding-agent sdk、AgentSession hooks | 接入 createAgentSession | app runtime 不感知 loop 实现 | coding-agent smoke |
| CORE-021 | Agent harness | harness 切 backend | harness 成为 core 稳定测试入口 | harness smoke |
| CORE-022 | 主启动入口、TS 启动入口 | 本地 dogfood | 本地默认验证 Rust，TS 入口可回滚 | startup smoke |
| CORE-023 | 全部 parity 和 smoke 结果 | 默认 Rust core | 默认路径切 Rust，保留 TS 回滚 | complete regression gate |
| CORE-024 | 稳定后的 Rust default | 清理 TS loop runtime | 删除过渡层，保留 API shell | full focused core suite |

### CORE-001：建立 core 迁移文档

目的：把 core 迁移从 model 迁移中拆出来。

改动：

- 新增本文档。
- 后续进入 dev-flow task 时，再新增对应 task 文档。

验收：

- 文档明确 core 定义。
- 文档明确不迁范围。
- 文档能直接拆成开发任务。

验证：

- 文档审阅。
- 不需要运行代码检查。

回滚：

- 删除新增文档即可。

### CORE-002：冻结 TypeScript loop parity fixtures

目的：先固定当前 TS 行为，避免 Rust 实现凭感觉复刻。

改动：

- 新增 focused loop 测试。
- 测试只使用 fake model stream。
- 测试只使用 fake tools。
- 不调用真实 provider。
- 不调用 shell 或文件工具。

必须覆盖：

- no-tool 单 turn
- model stream 正常 done
- model stream 在 start 前 error
- model stream 在 partial 后 error
- abort during stream
- 一个成功 tool call
- unknown tool
- blocked tool
- tool 参数 validation failure
- tool execute throw
- sequential tool batch
- parallel tool batch
- all tool results terminate
- partial tool results terminate
- steering queue injection
- follow-up queue continuation
- prepare next turn 更新 model
- prepare next turn 更新 thinking
- should stop after turn

验收：

- TS backend 全部通过。
- 测试断言 event order。
- 测试断言 final new messages。

验证：

- 运行新增 focused vitest。
- 不跑 full vitest。
- 不跑 real-provider e2e。

回滚：

- 删除新增测试，不影响 runtime。

### CORE-003：整理 Rust core 可序列化类型

目的：让 Rust core 能表达 TS loop contract。

改动：

- 给 AgentEvent 增加 serde 支持。
- 给 AgentMessage 增加 bridge-safe 表示。
- ToolResult terminate 改成 optional。
- ToolExecutionMode 序列化为 sequential 和 parallel。
- config 拆成更清楚的 runtime 边界。

建议结构：

```rust
pub enum AgentMessage {
    Standard(rozsa_model::types::Message),
    Custom(CustomAgentMessage),
}
```

```rust
pub struct CustomAgentMessage {
    pub role: String,
    pub custom_type: Option<String>,
    pub content: serde_json::Value,
    pub display: Option<bool>,
    pub details: Option<serde_json::Value>,
    pub timestamp: i64,
}
```

```rust
pub struct ToolResult {
    pub content: Vec<rozsa_model::types::ContentBlock>,
    pub details: serde_json::Value,
    pub terminate: Option<bool>,
}
```

验收：

- rozsa-core 可编译。
- Rust 测试可 serialize 和 deserialize agent events。
- 不切换 runtime 行为。

验证：

- cargo test -p rozsa-core

回滚：

- 回退 Rust 类型改动。
- TS runtime 不受影响。

### CORE-004：实现 Rust no-tool prompt loop

目的：先跑通没有工具的最小 agent loop。

改动：

- 实现 agent loop。
- 实现 prompt mode。
- 注入 fake ModelRuntime。
- 消费 rozsa-model StreamEvent。
- 发出 Rust AgentEvent。

事件要求：

- run 开始发 agent_start。
- turn 开始发 turn_start。
- user prompt 发 message_start 和 message_end。
- stream start 发 assistant message_start。
- delta 发 message_update。
- done 发 assistant message_end。
- turn 完成发 turn_end。
- run 完成发 agent_end。

验收：

- no-tool fixture 与 TS 事件顺序一致。
- final messages 与 TS 一致。

验证：

- cargo test -p rozsa-core

回滚：

- 保持 TS backend 默认，不影响应用。

### CORE-005：实现 Rust continue loop

目的：支持从已有 context 继续。

改动：

- 实现 continue mode。
- 校验 context 非空。
- 校验最后一条不能是 assistant。
- 不重复注入 prompt。

验收：

- last message 是 user 时可继续。
- last message 是 tool result 时可继续。
- last message 是 assistant 时返回明确错误。
- 空 context 返回明确错误。

验证：

- cargo test -p rozsa-core

回滚：

- Rust backend 未默认，不影响 TS。

### CORE-006：实现 Rust stream error 和 abort

目的：对齐失败路径。

改动：

- 处理 stream error event。
- 处理 stream 提前结束。
- 处理 cancellation token。
- 生成 aborted stop reason。

验收：

- start 前 error 有 message_start 和 message_end。
- partial 后 error 替换 partial assistant。
- abort 后最终 assistant stop reason 是 aborted。
- run lock 能释放。

验证：

- cargo test -p rozsa-core

回滚：

- 保持 TS backend 默认。

### CORE-007：实现 steering queue

目的：迁移用户在运行中插入消息的机制。

改动：

- 实现 initial steering poll。
- 支持 all queue mode。
- 支持 one-at-a-time queue mode。
- steering message 注入前发 message lifecycle。

验收：

- 初始 steering 可跳过一次。
- steering message 加入 context。
- steering message 加入 new messages。
- queue UI 所需事件顺序不变。

验证：

- cargo test -p rozsa-core

回滚：

- 不影响 TS backend。

### CORE-008：实现 follow-up queue

目的：迁移 agent 本来要停下时继续处理 follow-up 的机制。

改动：

- no more tools 后读取 follow-up。
- follow-up 注入下一轮。
- 支持两种 queue mode。

验收：

- 无 tool 且无 steering 时读取 follow-up。
- follow-up 存在时开启新 turn。
- follow-up 不存在时 agent end。

验证：

- cargo test -p rozsa-core

回滚：

- 不影响 TS backend。

### CORE-009：实现 prepare next turn

目的：支持 turn 后更新下一轮 runtime 状态。

改动：

- hook 返回新的 context。
- hook 返回新的 model。
- hook 返回新的 thinking level。
- 只影响下一次 provider request。

验收：

- context update 生效。
- model update 生效。
- thinking update 生效。
- 未返回字段保持旧值。

验证：

- cargo test -p rozsa-core

回滚：

- 不影响 TS backend。

### CORE-010：实现 should stop after turn

目的：支持 turn 后优雅停止。

改动：

- 在 turn end 后调用 should-stop hook。
- should-stop 在 follow-up 前执行。
- true 时直接 agent end。

验收：

- hook 返回 true 时不再读取 follow-up。
- hook 返回 false 时正常进入 queue drain。
- hook context 包含 message、tool results、current context、new messages。

验证：

- cargo test -p rozsa-core

回滚：

- 不影响 TS backend。

### CORE-011：定义 tool host protocol

目的：明确 Rust 调度工具，TS 执行工具。

改动：

- 定义 Rust 请求 TS prepare tool。
- 定义 TS 返回 prepared args 或 blocked reason。
- 定义 Rust 请求 TS execute tool。
- 定义 TS 返回 tool update 和 final result。

协议消息：

```json
{"type":"tool_prepare","version":1,"runId":"run_1","requestId":"p_1","toolCall":{}}
```

```json
{"type":"tool_prepared","version":1,"runId":"run_1","requestId":"p_1","ok":true,"args":{}}
```

```json
{"type":"tool_execute","version":1,"runId":"run_1","requestId":"e_1","toolName":"read","toolCallId":"tc_1","args":{}}
```

```json
{"type":"tool_result","version":1,"runId":"run_1","requestId":"e_1","result":{},"isError":false}
```

验收：

- 协议包含 version。
- requestId 可关联请求和响应。
- host failure 有明确 error response。
- Rust 不直接执行工具。

验证：

- Rust protocol unit tests。
- TS protocol parse tests。

回滚：

- 协议文件可独立回退。

### CORE-012：实现 TypeScript tool host adapter

目的：把现有 tool 行为包装成 host adapter。

TS host 负责：

- 查找 tool。
- 调用 prepareArguments。
- 调用 validateToolArguments。
- 调用 beforeToolCall。
- 执行 permission check。
- 执行 extension tool_call。
- 调用真实 tool execute。
- 转发 partial update。
- 执行 extension tool_result。
- 执行 LSP post-processing。

验收：

- unknown tool 返回 error result。
- blocked tool 返回 error result。
- validation failure 返回 error result。
- execute throw 返回 error result。
- partial update 可转发。

验证：

- focused TS tests。
- npm run check，因为会改 TS source。

回滚：

- TypeScript backend 不依赖该 adapter。

### CORE-013：实现 Rust sequential tool scheduling

目的：迁移 sequential tool batch。

改动：

- 按 assistant tool call 顺序 prepare。
- prepare 成功后 execute。
- execute 完成后 finalize。
- 立即发 tool_execution_end。
- 立即生成 tool result message。

验收：

- tool A 完成后才开始 tool B。
- event order 与 TS 一致。
- signal aborted 后停止后续 tool。

验证：

- cargo test -p rozsa-core

回滚：

- 不影响 TS backend。

### CORE-014：实现 Rust parallel tool scheduling

目的：迁移 parallel tool batch。

改动：

- prepare 仍按 source order。
- execute 并发。
- tool_execution_end 按完成顺序。
- tool result message 按 source order。

验收：

- 快 tool 先发 execution end。
- result message 顺序仍等于 assistant source order。
- 任一 tool executionMode 是 sequential 时整批改 sequential。

验证：

- cargo test -p rozsa-core

回滚：

- 不影响 TS backend。

### CORE-015：实现 tool batch termination

目的：对齐 terminate 语义。

规则：

- 只有所有 finalized tool result 都 terminate=true，才停止 tool loop。
- 只有部分 tool result terminate，不停止。
- 没有 tool result，不停止。

验收：

- all terminate fixture 通过。
- partial terminate fixture 通过。
- no terminate fixture 通过。

验证：

- cargo test -p rozsa-core

回滚：

- 不影响 TS backend。

### CORE-016：新增 AgentLoopBackend abstraction

目的：让 Agent public API 不变，内部可选 TS 或 Rust。

改动：

- 新增 AgentLoopBackend interface。
- 新增 TsAgentLoopBackend。
- 先不实现完整 Rust backend。
- Agent 构造时选择 backend。

验收：

- 默认 TS backend。
- 现有 Agent tests 通过。
- Agent prompt 和 Agent continue 外部行为不变。

验证：

- focused TS tests。
- npm run check。

回滚：

- 回退 backend abstraction。

### CORE-017：实现 Rust core bridge binary

目的：提供可被 TS 启动的 Rust runtime。

改动：

- 新增 bridge binary 或 bridge mode。
- stdin 读 JSON Lines。
- stdout 写 JSON Lines。
- stderr 写 logs。
- 支持 start run。
- 支持 cancel。
- 支持 tool host request。

验收：

- bridge 可启动。
- invalid JSON 返回 protocol error。
- run done 后退出或回到 idle。
- stdout 不混日志。

验证：

- Cargo bridge tests。

回滚：

- 不接入 TS，删除 bridge 即可。

### CORE-018：实现 TypeScript RustCoreClient

目的：让 TS Agent 可调用 Rust bridge。

改动：

- 启动 Rust child process。
- 发送 start run。
- 接收 agent events。
- 处理 tool host requests。
- 发送 cancel。
- 处理 child exit。

验收：

- child path 错误时 fail fast。
- bridge exit 时释放 pending requests。
- cancel 可送达。
- agent event 可转发。

验证：

- focused TS client tests。
- npm run check。

回滚：

- TS backend 默认，不影响 runtime。

### CORE-019：实现 RustAgentLoopBackend

目的：把 RustCoreClient 接到 AgentLoopBackend。

改动：

- runPrompt 走 Rust bridge。
- runContinue 走 Rust bridge。
- 将 AgentEvent 转给现有 Agent event processor。
- 将 tool host adapter 接到 bridge。

验收：

- ROZSA_CORE_BACKEND=rust 可跑 no-tool prompt。
- ROZSA_CORE_BACKEND=rust 可跑 one fake tool prompt。
- ROZSA_CORE_BACKEND=ts 行为不变。

验证：

- focused TS tests。
- npm run check。

回滚：

- 设置 ROZSA_CORE_BACKEND=ts。

### CORE-020：接入 createAgentSession

目的：coding-agent runtime 可选择 Rust core。

改动：

- createAgentSession 创建 Agent 时传 backend 选项。
- 保持 streamFn 不变。
- 保持 convertToLlmWithBlockImages 不变。
- 保持 AgentSession tool hooks 不变。

验收：

- no-tool smoke 通过。
- fake read-only tool smoke 通过。
- permission reject smoke 通过。
- abort smoke 通过。
- TS backend 仍可用。

验证：

- focused coding-agent tests。
- npm run check。

回滚：

- 设置 ROZSA_CORE_BACKEND=ts。

### CORE-021：切换 AgentHarness

目的：让 harness 也走同一 backend。

改动：

- agent-harness 不再直接调用 TS loop。
- 复用 AgentLoopBackend。
- 保持 harness session writes。
- 保持 harness hooks。

验收：

- harness prompt 在 TS backend 通过。
- harness prompt 在 Rust backend 通过。
- session writes 顺序不变。

验证：

- focused harness tests。
- 不跑 real-provider e2e。

回滚：

- harness 切回 TS backend。

### CORE-022：本地 dogfood

目的：在不影响 release 的前提下让本地入口使用 Rust core。

改动：

- 主启动入口设置或继承 Rust core。
- TS 启动入口显式设置 TS core。
- 文档写清切换方式。

验收：

- 主启动入口走 Rust core。
- TS 启动入口走 TS core。
- Rust core 启动失败报清楚。
- 没有 silent fallback。

验证：

- startup smoke。
- npm run check。

回滚：

- 主启动入口改回 TS core。

### CORE-023：默认 Rust core

目的：在 parity 完成后切默认。

前置条件：

- TS parity fixtures 通过。
- Rust no-tool fixtures 通过。
- Rust queue fixtures 通过。
- Rust tool fixtures 通过。
- bridge tests 通过。
- coding-agent smoke 通过。
- harness smoke 通过。
- npm run check 通过。
- 回滚路径已写文档。

改动：

- 默认 backend 改为 rust。
- 保留 ROZSA_CORE_BACKEND=ts。

验收：

- 不设置环境变量时使用 Rust core。
- 设置 ROZSA_CORE_BACKEND=ts 可回到 TS。

验证：

- cargo test -p rozsa-core
- focused TS tests
- npm run check
- startup smoke

回滚：

- 默认值改回 ts。

### CORE-024：清理 TS loop

目的：一个稳定迭代后移除过渡代码。

前置条件：

- Rust default 已稳定一个迭代。
- 没有 blocker issue。
- 用户确认可以删除旧 loop。

改动：

- 删除或降级 TS loop runtime。
- 保留 TS public API shell。
- 更新 docs。

验收：

- Rust core 是唯一 runtime。
- Agent public API 仍兼容。

验证：

- full focused core tests。
- npm run check。

回滚：

- 该任务执行前必须确认。
- 不在未确认时删除 TS loop。

## 协议最小集

第一版 bridge 只需要支持：

- start_run
- agent_event
- tool_prepare
- tool_prepared
- tool_execute
- tool_update
- tool_result
- cancel
- run_done
- run_error

不需要支持：

- 多 run 并发
- 长驻 daemon
- HTTP sidecar
- FFI
- N-API
- provider registry 操作
- settings 操作
- session 操作

## 验证矩阵

完整性验证不是最后一步才做。每个阶段都有对应 gate。

### Gate A：迁移前基线

必须在任何 Rust 替换前完成。

通过标准：

- TS parity suite 全部通过。
- fixtures 覆盖 no-tool、stream error、abort、tool、queue、hook。
- 每个 fixture 记录 event sequence 和 final messages。
- 没有真实 provider、真实 shell、真实文件写入依赖。

失败处理：

- 不进入 Rust loop 实现。
- 先修测试或补齐当前行为说明。

### Gate B：Rust core 单元完整性

适用于 CORE-003 到 CORE-015。

通过标准：

- Rust core 类型测试通过。
- Rust no-tool parity 通过。
- Rust queue parity 通过。
- Rust tool scheduling parity 通过。
- Rust terminate parity 通过。
- Rust abort parity 通过。
- Rust 输出事件和 TS baseline 对齐。

失败处理：

- 不接入 TypeScript Agent backend。
- 不允许用 TS fallback 掩盖 Rust 行为差异。

### Gate C：Bridge 完整性

适用于 CORE-017 到 CORE-019。

通过标准：

- invalid input 有 protocol error。
- stdout 只有 JSON Lines protocol。
- stderr 只有日志。
- child exit 会释放 pending requests。
- cancel 会释放 run 和 tool host request。
- Rust backend 缺 binary 时 fail fast。
- Rust backend 不 silent fallback 到 TS。

失败处理：

- Rust backend 不允许进入 coding-agent runtime。

### Gate D：应用完整性

适用于 CORE-020 到 CORE-021。

通过标准：

- no-tool prompt smoke 通过。
- fake tool smoke 通过。
- permission reject smoke 通过。
- extension tool call smoke 通过。
- extension tool result smoke 通过。
- abort smoke 通过。
- session message persistence smoke 通过。
- harness prompt smoke 通过。
- harness session write smoke 通过。
- npm run check 通过。

失败处理：

- 不允许进入本地 dogfood。
- 默认 backend 保持 TS。

### Gate E：启动完整性

适用于 CORE-022 到 CORE-023。

通过标准：

- 主启动入口可用。
- TS 启动入口可用。
- Rust backend 错误信息清楚。
- native TUI 可看到 message、tool、queue 状态。
- 回滚到 ROZSA_CORE_BACKEND=ts 后功能正常。

失败处理：

- 不切默认。
- 不清理 TS loop。

| 变更类型 | 最小验证 |
| --- | --- |
| Rust core 类型 | cargo test -p rozsa-core |
| Rust loop | cargo test -p rozsa-core |
| Rust bridge | targeted Cargo bridge tests |
| TS backend abstraction | focused vitest |
| TS bridge client | focused vitest |
| coding-agent 接入 | focused tests + npm run check |
| launcher 改动 | startup smoke + npm run check |

禁止默认执行：

- npm test
- full vitest suite
- real-provider e2e
- npm run build

## 风险和控制

### 权限绕过

风险：Rust 调度 tool 后跳过 TS permission。

控制：

- Rust 不直接执行工具。
- TS host 负责 permission。
- permission reject fixture 是强制验收项。

### 事件顺序漂移

风险：UI 和 session persistence 依赖事件顺序。

控制：

- 先做 TS parity fixtures。
- Rust fixtures 复用同一套预期。
- 不合并语义不同的事件。

### bridge deadlock

风险：Rust 等 TS tool response，TS 又等 Rust event。

控制：

- 每个 host request 有 requestId。
- TS client 维护 pending request map。
- cancel 会释放 pending requests。
- bridge exit 会 fail 所有 pending requests。

### custom message 不匹配

风险：TS declaration merging 的 custom message 不能直接映射 Rust enum。

控制：

- Rust 把 custom message 当 tagged JSON。
- convertToLlm 继续由 TS host 执行。
- Rust 不解释未知 custom payload。

### abort 行为不一致

风险：model abort、tool abort、queue abort 行为不同。

控制：

- Rust 用 cancellation token。
- TS host 保持 AbortSignal。
- 单独测 stream abort 和 tool abort。

### 迁移范围膨胀

风险：core 迁移扩大成 AgentSession 重写。

控制：

- 第一阶段只迁 agent package loop。
- AgentSession 保持 TS host。
- session、settings、extensions、permissions 后续单独规划。

## 完成标准

core 迁移完成需要同时满足：

- Rust core 是默认 backend。
- TypeScript backend 至少保留一个稳定迭代用于回滚。
- Agent public API 不破坏。
- AgentSession 没有被大改。
- permission 行为不变。
- extension tool hooks 行为不变。
- steering 和 follow-up 行为不变。
- sequential 和 parallel tool scheduling 与 TS 一致。
- abort 和 error 行为与 TS 一致。
- focused parity tests 通过。
- npm run check 通过。
- 文档写清默认 backend 和回滚方式。
