# SPEC: Multi-agent 运行时 — SubagentManager + switch_agent 接通

## Goal

实现 TS 对等的 subagent 运行时，使 `switch_agent` 可用。包含：
1. `rozsa-app` 层新增 `SubagentManager`（spawn/send/wait/abort/list + scope 权限隔离）
2. `rozsa-tui` 层 `NativeBackend::switch_agent` 接通真实逻辑
3. TUI sidebar 直接查询 `AgentSession` 获取 subagent 列表（不走 JSON 中转）

## Design

### 模块布局

```
crates/rozsa-app/src/subagent/
├── mod.rs          // re-exports
├── manager.rs      // SubagentManager
├── runtime.rs      // SubagentRuntime（单个 subagent 内部状态）
└── scope.rs        // SubagentScope 权限隔离
```

### 核心类型 (`scope.rs`)

```rust
pub enum AllowedTools {
    All,
    Only(HashSet<String>),
}

pub struct SubagentScope {
    pub allowed_tools: AllowedTools,
    pub allowed_paths: Option<Vec<PathBuf>>,
    pub bash_prefixes: Option<Vec<String>>,
    pub allowed_skills: Option<Vec<String>>,
}

impl SubagentScope {
    /// 从 tool call args 检查是否允许执行
    pub fn check_tool_allowed(&self, tool_name: &str, args: &Value, cwd: &Path) -> Result<(), String>;

    /// 预设构造
    pub fn inherit() -> Self;
    pub fn readonly() -> Self;
    pub fn scoped(paths: Vec<PathBuf>) -> Self;
    pub fn custom(tools: Option<Vec<String>>, paths: Option<Vec<PathBuf>>, bash_prefixes: Option<Vec<String>>, skills: Option<Vec<String>>) -> Self;
}
```

### 核心类型 (`runtime.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SubagentStatus { Idle, Running, Aborted, Error }

#[derive(Debug, Clone, Serialize)]
pub struct SubagentInfo {
    pub id: String,
    pub name: String,
    pub status: SubagentStatus,
    pub model_id: String,
    pub model_provider: String,
    pub thinking_level: ThinkingLevel,
    pub created_at: i64,
    pub last_activity_at: i64,
    pub last_error: Option<String>,
    pub message_count: usize,
    pub session_file: Option<PathBuf>,
}
```

内部 `SubagentRuntime`（非 pub）：
- `info: SubagentInfo`
- `scope: SubagentScope`
- `messages: Vec<AgentMessage>`
- `cancel_token: CancellationToken`
- `system_prompt: String`
- `model: Model`
- `thinking_level: ThinkingLevel`
- `tools: Vec<Arc<dyn Tool>>` — 从主 session 过滤后的子集

### 核心类型 (`manager.rs`)

```rust
pub struct SpawnConfig {
    pub name: Option<String>,
    pub system_prompt: String,
    pub model: Option<Model>,          // None = 继承主 session
    pub thinking_level: Option<ThinkingLevel>,  // None = 继承
    pub scope: SubagentScope,
}

pub struct SubagentSnapshot {
    pub info: SubagentInfo,
    pub messages: Vec<AgentMessage>,
}

pub struct SubagentManager { /* 内部字段 */ }

impl SubagentManager {
    pub fn new(shared: SharedResources) -> Self;
    pub fn spawn(&mut self, config: SpawnConfig) -> Result<SubagentInfo, String>;
    pub async fn send(&mut self, id: &str, text: &str, wait: bool) -> Result<(), String>;
    pub async fn wait(&self, id: &str) -> Result<(), String>;
    pub async fn abort(&mut self, id: &str) -> Result<(), String>;
    pub fn list(&self) -> Vec<SubagentInfo>;
    pub fn get_messages(&self, id: &str) -> Option<&[AgentMessage]>;
    pub fn snapshot(&self, id: &str) -> Option<SubagentSnapshot>;
}
```

`SharedResources` 包含从主 `AgentSession` 借来的：
- `model_stream: Arc<dyn Fn(...) -> EventStream<StreamEvent> + Send + Sync>`
- `convert_to_llm: Arc<dyn Fn(&[AgentMessage]) -> Vec<Message> + Send + Sync>`
- `main_tools: Vec<Arc<dyn Tool>>`
- `main_model: Model`
- `main_thinking_level: ThinkingLevel`
- `cwd: PathBuf`
- `session_dir: PathBuf` — subagent .jsonl 文件存放目录
- `edit_mode: Arc<...>` — 用于 think_first 继承

### AgentSession 集成

```rust
// agent_session.rs 新增
pub struct AgentSession {
    // ...existing...
    subagent_manager: tokio::sync::Mutex<SubagentManager>,
    viewing_subagent_id: tokio::sync::Mutex<Option<String>>,
}

impl AgentSession {
    pub async fn subagent_manager(&self) -> MutexGuard<SubagentManager>;
    pub async fn viewing_subagent_id(&self) -> Option<String>;
    pub async fn set_viewing_subagent(&self, id: Option<String>);
}
```

### NativeBackend 变更

```rust
// switch_agent 接通
async fn switch_agent(&self, id: &str) -> BackendResult<()> {
    if id == "main" {
        self.session.set_viewing_subagent(None).await;
    } else {
        let mgr = self.session.subagent_manager().await;
        if mgr.snapshot(id).is_none() {
            return Err(BackendError::Internal(format!("Subagent '{id}' not found")));
        }
        drop(mgr);
        self.session.set_viewing_subagent(Some(id.to_string())).await;
    }
    self.push_state().await;
    Ok(())
}
```

### TUI sidebar 变更

```rust
// 新增 trait
pub trait SubagentView: Send + Sync {
    fn list_subagents_sync(&self) -> Vec<SubagentInfo>;
    fn viewing_subagent_id_sync(&self) -> Option<String>;
}

// render_sidebar 签名
pub fn render_sidebar(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    agents: Option<&dyn SubagentView>,
)
```

`NativeBackend` impl `SubagentView`（内部用 `try_lock` 非阻塞获取）。无 subagent 时传 `None`，sidebar 退化为当前行为。

### Scope 在 pre_tool_use 中的集成

`SubagentManager::send` 构建 `AgentLoopConfig` 时，将 scope 检查嵌入 `pre_tool_use`：

```rust
let scope = runtime.scope.clone();
let cwd = self.shared.cwd.clone();
let edit_mode = self.shared.edit_mode.clone();

config.pre_tool_use = Some(Box::new(move |ctx| {
    Box::pin(async move {
        // 1. edit_mode think_first 继承
        if let Some(reason) = edit_mode.check_tool_blocked(&ctx.tool_name, &ctx.args) {
            return Some(PreToolUseResult { block: true, reason: Some(reason) });
        }
        // 2. scope 工具/路径/bash 限制
        if let Err(reason) = scope.check_tool_allowed(&ctx.tool_name, &ctx.args, &cwd) {
            return Some(PreToolUseResult { block: true, reason: Some(reason) });
        }
        None
    })
}));
```

### 消息持久化

每个 subagent spawn 时在 main session 同名子目录下创建独立 .jsonl 文件：

```
<session_dir>/
├── <main-session-uuid>.jsonl
└── <main-session-uuid>/          ← 同名子目录
    ├── subagent-0.jsonl
    └── subagent-1.jsonl
```

通过 `SessionManager::create` 创建，`parent_session` 设为 main session 路径（header 冗余校验）。

```rust
// SubagentRuntime 内部
struct SubagentRuntime {
    // ...existing...
    session_manager: SessionManager,  // 独立的 .jsonl 写入
}
```

- 写入时机：每次 `AgentEvent::MessageEnd` 时 append
- 重启后不自动恢复运行状态，但历史消息可通过子目录回看
- `SubagentInfo` 增加 `session_file: Option<PathBuf>` 字段
- 删除 main session 时 `rm <uuid>.jsonl && rm -r <uuid>/` 即可清理

### 前置依赖

- ISSUE-I008: TUI 公共组件抽取（TabBar / HintsBar）— graph subagent tab bar 依赖此 issue 完成

### Graph 中的 subagent 展示

**主时间线：**
- subagent spawn 和 end 显示为独立的 agent 节点（icon 区分，如 `⊕` spawn / `⊗` end）
- agent 节点始终可见（不随 tool 节点的 `o` 键隐藏）
- 并行 spawn 时每个 spawn 一个独立节点
- 选中 agent 节点时，tab bar 中对应 subagent 高亮，hints 提示 Tab 跳转

**Tab bar：**
- 顶部 tab 显示 `main | subagent-name-1 | subagent-name-2 | ...`
- 切换方式：左右键 或 Tab / Shift+Tab
- 超出宽度时两端显示 `‹` `›`，当前 tab 始终可见
- 每个 tab 是独立时间线视图

**数据源：**
- main tab → `session_manager.entries()` 或 `live.messages`
- subagent tab → `subagent_manager.get_messages(id)`

### 约束

- **不做运行状态恢复** — 重启后 subagent 列表为空，.jsonl 仅供回看
- **不做 subagent 嵌套** — spawn 时过滤掉 "subagent" tool
- **活跃上限 10** — spawn 时检查，超限返回错误
- **不改 rozsa-core** — agent_loop 接口不变

## Acceptance

- SPEC-AC-001: `NativeBackend::switch_agent("subagent-0")` 在 subagent 存在时成功返回 Ok，push_state 更新 viewing id
- SPEC-AC-002: `SubagentManager::spawn` 创建 subagent 后 `list()` 能看到 id、name、status=Idle
- SPEC-AC-003: `SubagentManager::send` 后 subagent status 变为 Running → Idle，messages 积累
- SPEC-AC-004: `SubagentManager::abort` 后 status 变为 Aborted
- SPEC-AC-005: spawn 超过 10 个时返回错误
- SPEC-AC-006: scope=readonly 时 subagent 执行 "write" tool 被 pre_tool_use block
- SPEC-AC-007: scope=scoped(["/tmp/a"]) 时 read "/etc/passwd" 被 block
- SPEC-AC-008: TUI sidebar `render_sidebar` 传入 SubagentView 后正确渲染 agent 列表
- SPEC-AC-009: switch_agent("main") 恢复主 agent 视图

## Test Plan

- 单元测试：`SubagentScope::check_tool_allowed` 覆盖 inherit/readonly/scoped/custom 各场景
- 单元测试：`SubagentManager` spawn/send/abort/list 基本流程（mock model_stream）
- 集成测试：`NativeBackend::switch_agent` 从 stub 到真实切换
- TUI 测试：验证 sidebar 在有/无 subagent 时的渲染输出

## Self Check

- [x] Goal is clear — 实现 subagent 运行时 + 接通 switch_agent
- [x] Acceptance criteria are testable — 每条都可自动化验证
- [x] Matches current mode — fast mode，无冗余章节
- [x] 不改 rozsa-core — 全部新增在 app/tui 层
- [x] 模块边界清晰 — scope/runtime/manager 各司其职
