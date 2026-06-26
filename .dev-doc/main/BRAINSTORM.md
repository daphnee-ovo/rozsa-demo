# Brainstorm Notes — Multi-agent 运行时实现

**Date**: 2026-06-26

## 背景与目的

ISSUE-I002: `switch_agent` 当前返回 "not available" — TUI 前端消费层已全部就位（sidebar 渲染 activeSubagents、键绑定 Ctrl+]/Alt+[、/subagents 命令分发），但产生数据的后端运行时完全缺失。

目标：实现 TS 对等的 subagent 体系，让模型可以 spawn 子 agent 并在 TUI 中切换查看。

## 关键决策

| 决策点 | 选择 | 理由 |
|--------|------|------|
| SubagentRuntime 重量级 | 轻量 — 直接用 `agent_loop` | 不需要 extension_runner/skill_registry 等重型组件 |
| TUI 数据获取方式 | render 层直接查询 AgentSession | 同进程无需 JSON 序列化中转 |
| render 传参方式 | render 函数签名加参数（非 AppState 持有 session） | 低耦合，只在需要的组件传入 |
| Scope 实现 | 本次一并实现 | inherit/readonly/scoped/custom 全量 |
| AgentSession vs SubagentManager | 独立 SubagentManager struct | AgentSession 持有但职责隔离 |

## 设计方案

### 架构

```
rozsa-core (不变)
  └── agent_loop(prompts, context, config, signal) → EventStream<AgentEvent>

rozsa-app
  ├── agent_session.rs        (持有 SubagentManager)
  ├── subagent/
  │   ├── mod.rs              (re-exports)
  │   ├── manager.rs          (SubagentManager — spawn/send/wait/abort/list)
  │   ├── runtime.rs          (SubagentRuntime — 单个 subagent 的运行时)
  │   └── scope.rs            (SubagentScope — 权限隔离)
  └── runtime_state.rs        (新增 viewing_subagent_id)

rozsa-tui
  ├── backend/native.rs       (switch_agent 接通; push_state 不拼 subagent JSON)
  └── components/sidebar.rs   (render 函数签名加 subagent 查询参数)
```

### 组件

#### SubagentScope (`rozsa-app/src/subagent/scope.rs`)

```rust
pub enum SubagentScopePreset {
    Inherit,           // 继承主 session 全部工具
    Readonly,          // 只允许 read/grep/find/ls
    Scoped(Vec<PathBuf>),  // 允许读写，限定路径
}

pub struct SubagentScope {
    pub allowed_tools: AllowedTools,  // All | Set<String>
    pub allowed_paths: Option<Vec<PathBuf>>,
    pub bash_prefixes: Option<Vec<String>>,
    pub allowed_skills: Option<Vec<String>>,
}

impl SubagentScope {
    pub fn from_preset(preset: SubagentScopePreset, cwd: &Path) -> Self;
    pub fn check_tool_allowed(&self, tool_name: &str, args: &Value) -> Result<(), String>;
}
```

#### SubagentRuntime (`rozsa-app/src/subagent/runtime.rs`)

```rust
pub struct SubagentInfo {
    pub id: String,
    pub name: String,
    pub status: SubagentStatus,  // Idle | Running | Aborted | Error
    pub model: Model,
    pub thinking_level: ThinkingLevel,
    pub created_at: i64,
    pub last_activity_at: i64,
    pub last_error: Option<String>,
}

struct SubagentRuntime {
    info: SubagentInfo,
    scope: SubagentScope,
    messages: Vec<AgentMessage>,
    cancel_token: CancellationToken,
    event_rx: Option<EventStream<AgentEvent>>,  // 当前活跃的 stream
}
```

核心：每次 `send` 时构建 `AgentContext` + `AgentLoopConfig`，调 `agent_loop`。
从主 session 继承：`model_stream`、`convert_to_llm`、`pre_tool_use`（外包 scope 检查）。

#### SubagentManager (`rozsa-app/src/subagent/manager.rs`)

```rust
pub struct SubagentManager {
    subagents: HashMap<String, SubagentRuntime>,
    next_id: u32,
    // 从主 session 借来的共享资源
    model_stream: Arc<...>,
    convert_to_llm: Arc<...>,
    main_tools: Vec<Arc<dyn Tool>>,
    main_pre_tool_use: ...,
}

impl SubagentManager {
    pub fn spawn(&mut self, config: SpawnConfig) -> &SubagentInfo;
    pub async fn send(&mut self, id: &str, text: &str, wait: bool) -> Result<()>;
    pub async fn wait(&self, id: &str) -> Result<()>;
    pub async fn abort(&mut self, id: &str) -> Result<()>;
    pub fn list(&self) -> Vec<&SubagentInfo>;
    pub fn snapshot(&self, id: &str) -> Option<SubagentSnapshot>;
    pub fn get_messages(&self, id: &str) -> Option<&[AgentMessage]>;
}
```

#### AgentSession 集成

```rust
// agent_session.rs 新增字段
pub struct AgentSession {
    // ...existing fields...
    subagent_manager: tokio::sync::Mutex<SubagentManager>,
}

impl AgentSession {
    pub async fn subagent_manager(&self) -> MutexGuard<SubagentManager>;
}
```

#### NativeBackend — switch_agent 实现

```rust
async fn switch_agent(&self, id: &str) -> BackendResult<()> {
    // 验证 subagent 存在（或 "main"）
    if id != "main" {
        let mgr = self.session.subagent_manager().await;
        if mgr.snapshot(id).is_none() {
            return Err(BackendError::Internal(format!("Subagent {id} not found")));
        }
    }
    // 更新 viewing id
    let mut state = self.session.runtime_state().await;
    state.viewing_subagent_id = if id == "main" { None } else { Some(id.to_string()) };
    drop(state);
    self.push_state().await;
    Ok(())
}
```

#### TUI sidebar — 直接查询

```rust
// 新增 trait 用于 sidebar 查询
pub trait SubagentView: Send + Sync {
    fn list_subagents(&self) -> Vec<SubagentInfoView>;
    fn viewing_subagent_id(&self) -> Option<&str>;
}

// render_sidebar 签名变更
pub fn render_sidebar(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    agents: &dyn SubagentView,  // 新增
)
```

### 数据流

```
[LLM calls subagent tool]
    → AgentSession.execute_tool("subagent", args)
    → SubagentManager.spawn(config) / send(id, text) / ...
    → agent_loop(prompts, context, config, cancel_token)
    → EventStream<AgentEvent> → SubagentRuntime 收集 messages, 更新 status

[TUI render]
    → render_sidebar(state, &session_as_subagent_view)
    → session.subagent_manager().list()  // 直接查询
    → 渲染 agent 列表

[用户按 Ctrl+] / /subagents]
    → backend.switch_agent(id)
    → runtime_state.viewing_subagent_id = Some(id)
    → push_state → TUI 切换显示该 agent 的消息流
```

### 错误处理

- `spawn` 超过上限（10个活跃 subagent）→ 返回错误，tool result 报告 limit reached
- `send` 到不存在的 id → BackendError::Internal
- subagent 内部 LLM 错误 → status 置为 Error，info.last_error 记录原因
- `abort` → cancel_token 取消，status 置为 Aborted
- scope 检查失败 → pre_tool_use 返回 block + reason

## 约束与边界

- **不做 session 持久化**：subagent 的消息暂存内存，不写 .jsonl（后续迭代加）
- **不做 subagent 嵌套**：subagent 的工具列表中排除 "subagent" tool
- **不做跨 session 恢复**：重启后 subagent 状态丢失
- **不改 rozsa-core**：agent_loop 接口不变，所有新增在 rozsa-app 和 rozsa-tui 层

## 下一步

直接进入 `/spec` — 需求已足够明确，可以出技术规格。
