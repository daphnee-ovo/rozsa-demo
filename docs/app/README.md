# rozsa-app — Application Layer

## 概述

`rozsa-app` 是 Rózsa 的应用层 crate，以 `AgentSession` 为核心，连接 `rozsa-core` 的 agent loop 和 `rozsa-tui` 的前端。它封装了会话管理、工具注册、权限控制、skill 扩展、subagent 编排、运行时状态跟踪等应用层逻辑。

**核心职责**：
- 提供 `AgentSession`，管理对话历史、工具调用、取消令牌、事件流
- 通过 `SessionManager` 实现 JSONL 持久化（append-only tree structure）
- 通过 `ModelRegistry` 管理模型元数据、凭据发现、models.json 合并
- 通过 `SkillRegistry` 加载和注册 skill（project / user scope）
- 通过 `SubagentManager` 派生子 agent，实现 scope 隔离和并发编排
- 通过 `RuntimeState` 跟踪 edit mode、tool stats、permission mode
- 通过 `PermissionPolicy` 和 `PendingApprovals` 实现工具调用审批
- 通过 `ExtensionRunner` 支持生命周期钩子扩展

## 模块结构

```
rozsa-app/src/
├── agent_session.rs         # AgentSession 核心：prompt / continue / abort / compact
├── config_paths.rs          # ConfigRoots：全局/项目配置根与统一相对布局
├── session/
│   └── manager.rs           # JSONL 持久化：SessionManager / SessionEntry / SessionMeta
├── subagent/
│   ├── mod.rs
│   ├── manager.rs           # SubagentManager：spawn / send / wait / abort / list
│   ├── runtime.rs           # SubagentRuntime / SubagentInfo / SubagentStatus
│   └── scope.rs             # SubagentScope：inherit / readonly / scoped / custom
├── model_registry/
│   └── mod.rs               # ModelRegistry / ImageModelRegistry：发现模型、合并 models.json
├── skills/
│   ├── mod.rs               # SkillRegistry：加载、查找、格式化 system prompt 片段
│   └── loader.rs            # load_skills_from_dirs：frontmatter 解析、scope 优先级
├── permissions/
│   └── mod.rs               # PermissionMode / PermissionPolicy / PendingApprovals
├── runtime_state.rs         # EditMode / ToolCallStats / RuntimeState / RuntimeStateSnapshot
├── compaction/
│   └── mod.rs               # CompactionEngine：token 统计、summarize、执行 compaction
├── extensions.rs            # ExtensionRunner：on_init / on_prompt / on_message_end / on_end
├── messages.rs              # message 类型辅助函数
├── resources/
│   └── mod.rs               # LoadedResources：CLAUDE.md / AGENTS.md / user instructions
├── settings/                # 设置层：schema / merge / storage
├── tools/                   # 内置工具实现：read / write / edit / bash / ls / grep / find
└── slash_commands.rs        # slash command 解析（尚未充分实现）
```

## 核心类型

### AgentSession

**职责**：应用层 orchestrator，连接 core agent loop 与 TUI backend。管理工具、权限、扩展、compaction、runtime state、消息历史、取消令牌、事件广播。

**关键字段**：
- `static_config: StaticConfig` — 不变配置（system prompt / cwd / settings / resources）
- `runtime: Mutex<RuntimeParams>` — 运行时参数（model / thinking_level），可在 turn 之间更新
- `session_manager: Mutex<SessionManager>` — JSONL 持久化管理器
- `tools: Arc<Mutex<Vec<Arc<dyn Tool>>>>` — 注册的工具列表
- `messages: Mutex<Vec<AgentMessage>>` — 内存中的对话历史
- `event_tx: broadcast::Sender<AgentEvent>` — 事件广播通道（TUI 订阅）
- `cancel_token: Mutex<Option<CancellationToken>>` — 当前运行的取消令牌
- `is_running: AtomicBool` — 是否正在执行 agent loop
- `is_compacting: AtomicBool` — 是否正在压缩历史
- `runtime_state: Arc<Mutex<RuntimeState>>` — 运行时状态（edit mode / tool stats）
- `steering_queue / follow_up_queue` — 插入消息队列（steering 在工具调用间插入，follow-up 在所有工具结束后插入）
- `pre_tool_use_hook` — 外部权限检查钩子（由 backend 注入）
- `extension_runner` — 扩展生命周期钩子
- `skill_registry` — skill 注册表
- `subagent_manager` — 子 agent 管理器
- `viewing_subagent_id` — 当前 UI 查看的 subagent ID（None = main session）

**主要方法**：

```rust
// 构造
pub fn new(config: AgentSessionConfig) -> Self

// 工具注册
pub async fn register_tool(&self, tool: Arc<dyn Tool>)
pub async fn register_default_tools(&self, cwd: &Path)

// 主循环
pub async fn prompt(&self, message: &str) -> Result<Vec<AgentEvent>>
pub async fn continue_session(&self) -> Result<Vec<AgentEvent>>
pub async fn abort(&self)
pub fn is_running(&self) -> bool

// 事件订阅
pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent>

// Compaction
pub fn is_compacting(&self) -> bool
pub async fn compact(&self) -> Result<CompactionResult>

// 会话管理
pub async fn session_manager(&self) -> MutexGuard<'_, SessionManager>
pub async fn switch_session(&self, path: impl AsRef<Path>) -> Result<String>

// 模型和配置
pub async fn model(&self) -> Model
pub async fn set_model(&self, model: Model)
pub async fn thinking_level(&self) -> ThinkingLevel
pub async fn set_thinking_level(&self, level: ThinkingLevel)
pub fn cwd(&self) -> &Path
pub fn settings_manager(&self) -> &SettingsManager

// 运行时状态
pub async fn runtime_state_snapshot(&self) -> RuntimeStateSnapshot
pub async fn cycle_edit_mode(&self) -> EditMode

// 消息队列
pub fn steer(&self, text: &str)
pub fn follow_up(&self, text: &str)
pub fn pending_messages(&self) -> Vec<String>

// Subagent
pub async fn subagent_manager(&self) -> MutexGuard<'_, SubagentManager>
pub fn subagent_manager_try_lock(&self) -> Option<MutexGuard<'_, SubagentManager>>
pub async fn viewing_subagent_id(&self) -> Option<String>
pub fn viewing_subagent_id_try_lock(&self) -> Option<String>
pub async fn set_viewing_subagent(&self, id: Option<String>)

// Skill
pub fn skill_registry(&self) -> RwLockReadGuard<'_, SkillRegistry>
pub fn reload_skills(&self) -> Vec<SkillDiagnostic>

// Extension
pub async fn register_extension(&self, extension: Box<dyn Extension>)

// Bash 直接执行（不经过 agent loop）
pub async fn execute_bash(&self, command: &str) -> Result<String>
```

**Skill command expansion**：
当用户输入 `/skill:<name> [args]` 时，`AgentSession::expand_skill_command` 会将其展开为 `<skill>` XML block + args，插入到 context 中。

### AgentSessionConfig

构建 `AgentSession` 需要的配置 bundle。

```rust
pub struct AgentSessionConfig {
    pub model: Model,
    pub thinking_level: ThinkingLevel,
    pub system_prompt: String,
    pub cwd: PathBuf,
    pub session_manager: SessionManager,
    pub settings_manager: SettingsManager,
    pub resources: LoadedResources,
    pub pre_tool_use: Option<Box<PreToolUseFn>>,
}
```

### SubagentManager

**职责**：派生和管理子 agent。每个 subagent 拥有自己的消息历史、scope（工具白名单 + 路径限制）、session file。

**SharedResources**：
- `model_stream: Arc<ModelStreamFn>` — 模型流工厂
- `convert_to_llm: Arc<ConvertToLlmFn>` — AgentMessage → Message 转换
- `main_tools: Arc<Mutex<Vec<Arc<dyn Tool>>>>` — 主 session 的工具列表（subagent 过滤使用）
- `main_model / main_thinking_level` — 主 session 的模型和推理级别（默认继承）
- `cwd: PathBuf` — 工作目录
- `session_dir: Option<PathBuf>` — subagent session 文件目录（`<session_dir>/<main_uuid>/subagent-N.jsonl`）
- `main_session_uuid / main_session_file` — 主 session 的 UUID 和路径（写入 header parentSession）

**API**：

```rust
pub fn new(shared: SharedResources) -> Self

pub async fn spawn(&mut self, config: SpawnConfig) -> Result<SubagentInfo, String>
pub async fn send(&self, id: &str, text: &str, wait: bool) -> Result<(), String>
pub async fn wait(&self, id: &str) -> Result<(), String>
pub async fn abort(&self, id: &str) -> Result<(), String>

pub async fn list(&self) -> Vec<SubagentInfo>
pub fn list_sync(&self) -> Vec<SubagentInfo>  // TUI 渲染用（不阻塞）
pub async fn get_messages(&self, id: &str) -> Option<Vec<AgentMessage>>
pub async fn snapshot(&self, id: &str) -> Option<SubagentSnapshot>
```

**SpawnConfig**：
```rust
pub struct SpawnConfig {
    pub name: Option<String>,
    pub system_prompt: String,
    pub model: Option<Model>,
    pub thinking_level: Option<ThinkingLevel>,
    pub scope: SubagentScope,
}
```

**限制**：
- 最多 `MAX_ACTIVE_SUBAGENTS = 10` 个并发运行的 subagent
- 子 agent 自动屏蔽 `subagent` 工具（防止递归）

### SubagentScope

**职责**：控制 subagent 可访问的工具和路径。

**构造函数**：
```rust
pub fn inherit() -> Self            // 继承全部权限（AllowedTools::All）
pub fn readonly() -> Self           // 只读工具（read / grep / find / ls）
pub fn scoped(paths: Vec<PathBuf>) -> Self  // 限定路径白名单
pub fn custom(
    tools: AllowedTools,
    paths: Option<Vec<PathBuf>>,
    bash_prefixes: Option<Vec<String>>,
    skills: Option<Vec<String>>,
) -> Self
```

**核心方法**：
```rust
pub fn check_tool_allowed(
    &self,
    tool_name: &str,
    args: &Value,
    cwd: &Path,
) -> Result<(), String>
```

**检查逻辑**：
1. 工具名白名单（AllowedTools::Only）
2. 文件类工具（read/write/edit/grep/find/ls）路径白名单
3. bash 命令前缀白名单
4. skill 名称白名单

### SubagentInfo / SubagentStatus

**SubagentStatus**：
```rust
pub enum SubagentStatus {
    Idle,      // 创建后未运行 或 运行结束
    Running,   // 正在执行 agent loop
    Error,     // 运行出错
    Aborted,   // 被 abort
}
```

**SubagentInfo**：
```rust
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

### RuntimeState / EditMode

**EditMode**：
```rust
pub enum EditMode {
    Normal,      // 正常模式：所有工具可用
    ThinkFirst,  // Think-first 模式：阻止 edit/write，限制 bash 只读命令
}

impl EditMode {
    pub fn cycle(self) -> Self
    pub fn check_tool_blocked(&self, tool_name: &str, args: &Value) -> Option<String>
}
```

**RuntimeState**：
```rust
pub struct RuntimeState {
    pub edit_mode: EditMode,
    pub permission_mode: String,
    pub tool_stats: HashMap<String, ToolCallStats>,
}

impl RuntimeState {
    pub fn new(permission_mode: &str) -> Self
    pub fn record_tool_call(&mut self, tool_name: &str, is_error: bool)
    pub fn snapshot(&self) -> RuntimeStateSnapshot
}
```

**RuntimeStateSnapshot**（可序列化，供 TUI 使用）：
```rust
pub struct RuntimeStateSnapshot {
    pub edit_mode: EditMode,
    pub permission_mode: String,
    pub tool_stats: Vec<ToolCallStats>,
}
```

### SessionManager

**职责**：管理 JSONL 会话文件的 append-only tree structure。每个 session 有一个 header + N 个 entry（message / compaction / model_change / thinking_level_change / custom / label / session_info）。

**Entry 类型**：
- `SessionEntry::Message` — 用户或助手消息
- `SessionEntry::Compaction` — compaction 摘要（记录被删除的消息、tokens_before、first_kept_entry_id）
- `SessionEntry::ModelChange` — 模型切换
- `SessionEntry::ThinkingLevelChange` — 推理级别切换
- `SessionEntry::Custom` — 扩展自定义数据
- `SessionEntry::Label` — 为某个 entry 打标签
- `SessionEntry::SessionInfo` — 记录会话名称（最新的 session_info name 胜出）

**关键 API**：
```rust
// 创建 / 打开
pub fn create(path, session_id, cwd, parent_session) -> Result<Self>
pub fn create_lazy(path, session_id, cwd, parent_session) -> Self
pub fn open(path: impl AsRef<Path>) -> Result<Self>

// 追加 entry
pub fn append_message(&mut self, message: Message) -> Result<String>
pub fn append_compaction(&mut self, ...) -> Result<String>
pub fn append_model_change(&mut self, provider, model_id) -> Result<String>
pub fn append_thinking_level_change(&mut self, level) -> Result<String>
pub fn append_custom(&mut self, custom_type, payload) -> Result<String>
pub fn append_label(&mut self, target_id, label) -> Result<String>
pub fn append_session_info(&mut self, name: Option<String>) -> Result<String>

// 状态查询
pub fn session_id(&self) -> &str
pub fn session_file(&self) -> &Path
pub fn leaf_id(&self) -> Option<&str>
pub fn entries(&self) -> Vec<SessionEntry>
pub fn current_name(&self) -> Option<String>

// 文件管理
pub fn delete(path) -> Result<()>
pub fn rename(path, new_name) -> Result<()>
pub fn list_dir(dir) -> Result<Vec<SessionMeta>>
```

**SessionMeta**（轻量级元数据，用于 Sessions selector UI）：
```rust
pub struct SessionMeta {
    pub path: PathBuf,
    pub id: String,
    pub cwd: String,
    pub name: Option<String>,
    pub parent_session_path: Option<String>,
    pub created: String,        // RFC3339
    pub modified: String,       // RFC3339
    pub message_count: u32,
    pub first_message: String,
    pub all_messages_text: String,  // 用于 fuzzy search
}
```

### ModelRegistry

**职责**：
1. 加载 `models.generated.json`（checked-in 元数据）
2. 合并 `models.json`（用户配置，支持 provider overrides / model overrides / custom models）
3. 动态发现 NVIDIA 模型（当 `NVIDIA_API_KEY` 配置时）
4. 解析 API key（env var / models.json `apiKey` 字段 / shell command `!cmd`）

**核心 API**：
```rust
pub fn from_generated() -> Result<Self>
pub fn from_generated_with_models_json_path(path: Option<&Path>) -> Result<Self>

pub fn all(&self) -> Vec<Model>
pub fn all_json(&self) -> Value
pub fn find(&self, provider: &str, model_id: &str) -> Option<Model>
pub fn find_by_id(&self, model_id: &str) -> Option<Model>
pub fn first_available(&self) -> Option<Model>
pub fn is_user_configured(&self, provider: &str, model_id: &str) -> bool

pub fn apply_models_config_file(&mut self, path: &Path) -> Result<()>
pub fn merge_nvidia_models_if_configured(&mut self) -> Result<()>

pub fn provider_available(&self) -> HashMap<String, ProviderAvailable>
```

**ProviderAvailable**：
```rust
pub struct ProviderAvailable {
    pub configured: bool,       // 是否配置了 API key
    pub source: Option<String>, // "environment" / "models_json_key" / "models_json_command"
}
```

### SkillRegistry

**职责**：从 `ROZSA_CONFIG_DIR/skills`（默认 `~/.rozsa/skills`）和 `ROZSA_PROJECT_CONFIG_DIR/skills`（默认 `<project>/.rozsa/skills`）加载 skill，按 scope 优先级去重（Project > User），格式化为 system prompt 片段。

**Skill**：
```rust
pub struct Skill {
    pub name: String,
    pub description: String,
    pub file_path: PathBuf,
    pub base_dir: PathBuf,
    pub scope: SkillScope,  // Project / Agents / User
}
```

**API**：
```rust
pub fn load_from_defaults(cwd: &Path) -> Self
pub fn load_from_defaults_with_diagnostics(cwd: &Path) -> (Self, Vec<SkillDiagnostic>)

pub fn find_by_name(&self, name: &str) -> Option<&Skill>
pub fn list(&self) -> &[Skill]
pub fn is_empty(&self) -> bool
pub fn format_for_prompt(&self) -> String
pub fn slash_command_names(&self) -> Vec<String>
```

**SkillDiagnostic**：
```rust
pub struct SkillDiagnostic {
    pub path: PathBuf,
    pub error: String,
}
```

### Permissions

**PermissionMode**：
```rust
pub enum PermissionMode {
    OnRequest,       // 每次工具调用都需要审批
    AutoApprove,     // 匹配 auto-approve 模式的调用自动通过，其余需审批
    FreePermission,  // 所有调用直接通过
}
```

**PermissionPolicy**：
```rust
pub struct PermissionPolicy {
    mode: PermissionMode,
    blacklist: Vec<(Regex, &'static str)>,  // 硬编码黑名单（rm -rf / / sudo / git reset --hard 等）
    auto_approve_patterns: Vec<Regex>,
    session_approvals: Mutex<HashSet<String>>,  // 会话级审批记忆
}

impl PermissionPolicy {
    pub fn new(mode: PermissionMode, auto_approve_patterns: Vec<String>) -> Self
    pub fn evaluate(&self, tool_name: &str, args: &Value) -> PolicyVerdict
    pub fn record_session_approval(&self, trust_key: String)
}
```

**PolicyVerdict**：
```rust
pub enum PolicyVerdict {
    Allow,                          // 直接放行
    Block { reason: String },       // 拒绝
    NeedApproval { info: ApprovalInfo },  // 需要审批
}
```

**ApprovalInfo**：
```rust
pub struct ApprovalInfo {
    pub tool_name: String,
    pub args_summary: String,
    pub risk: RiskLevel,        // Read / Write / Shell / Destructive
    pub trust_key: String,      // "{tool_name}:{first_arg_prefix}"
}
```

**PendingApprovals**：
```rust
pub type PendingApprovals = Arc<DashMap<String, oneshot::Sender<PermissionResponse>>>;

pub enum PermissionResponse {
    Allow,                          // 仅本次允许
    AllowSession { trust_key: String },  // 本会话内自动允许
    Deny,                           // 拒绝
}
```

## 与其他 crate 的关系

### rozsa-app → rozsa-core
- **依赖**：`agent_loop` / `agent_loop_continue` / `AgentContext` / `AgentLoopConfig` / `Tool`
- **关系**：`AgentSession::prompt` 和 `continue_session` 调用 `rozsa_core::agent_loop`，传入 `AgentContext`（system prompt + messages + tools）和 `AgentLoopConfig`（hook + stream config）。
- **边界**：core 提供纯 agent loop 逻辑，app 负责 session 状态、工具注册、权限、扩展、持久化。

### rozsa-app → rozsa-model
- **依赖**：`Model` / `Message` / `SimpleStreamOptions` / `EventStream` / `stream::stream_simple`
- **关系**：`AgentSession` 持有 `Model`，调用 `rozsa_model::stream::stream_simple` 生成 LLM 流。`SessionManager` 序列化 `Message` 到 JSONL。
- **边界**：model 提供 LLM provider 抽象和事件流，app 负责模型元数据注册、会话生命周期。

### rozsa-tui → rozsa-app
- **依赖**：`NativeBackend` 持有 `AgentSession`，订阅 `AgentEvent`，驱动 `prompt` / `abort` / `cycle_edit_mode` / `subagent_manager`。
- **关系**：TUI 是 app 的消费者和驱动者，通过 `AgentSession` API 与 app 层交互。
- **边界**：app 提供应用逻辑和状态，TUI 负责 UI 渲染、用户输入、权限审批弹窗。

### rozsa-cli → rozsa-app
- **依赖**：CLI 初始化 `AgentSessionConfig`，创建 `AgentSession`，注入到 `NativeBackend`。
- **关系**：CLI 是应用入口，负责启动 TUI + backend + agent session。
- **边界**：CLI 处理命令行参数和初始化流程，app 和 TUI 分别处理应用逻辑和 UI。

## 使用示例

### 创建 AgentSession

```rust
use rozsa_app::agent_session::{AgentSession, AgentSessionConfig};
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::SettingsManager;
use rozsa_app::resources::LoadedResources;
use rozsa_model::types::{Model, ThinkingLevel};

// 1. 创建 session file
let session_manager = SessionManager::create(
    "/tmp/session.jsonl",
    uuid::Uuid::new_v4().to_string(),
    std::env::current_dir()?.to_string_lossy().to_string(),
    None,
)?;

// 2. 加载 settings 和 resources
let settings_manager = SettingsManager::load_from_defaults(&cwd)?;
let resources = LoadedResources::load_from_cwd(&cwd)?;

// 3. 构造 config
let config = AgentSessionConfig {
    model: Model::default(),  // 或从 ModelRegistry 获取
    thinking_level: ThinkingLevel::Normal,
    system_prompt: resources.system_prompt(),
    cwd: cwd.clone(),
    session_manager,
    settings_manager,
    resources,
    pre_tool_use: None,  // backend 注入权限检查
};

// 4. 创建 AgentSession
let session = AgentSession::new(config);

// 5. 注册工具
session.register_default_tools(&cwd).await;
```

### 发送 prompt

```rust
// 订阅事件流
let mut event_rx = session.subscribe();
tokio::spawn(async move {
    while let Ok(event) = event_rx.recv().await {
        println!("Event: {:?}", event);
    }
});

// 发送用户消息
let events = session.prompt("List all Rust files in src/").await?;

// 或继续会话（无新用户输入）
let events = session.continue_session().await?;
```

### Spawn subagent

```rust
use rozsa_app::subagent::{SpawnConfig, SubagentScope};

let mut manager = session.subagent_manager().await;

// 创建 readonly subagent
let config = SpawnConfig {
    name: Some("readonly-search".to_string()),
    system_prompt: "You are a read-only search agent.".to_string(),
    model: None,  // 继承主 session 的 model
    thinking_level: None,
    scope: SubagentScope::readonly(),
};

let info = manager.spawn(config).await?;

// 发送任务
manager.send(&info.id, "Find all TODO comments in src/", false).await?;

// 等待完成
manager.wait(&info.id).await?;

// 获取消息历史
let messages = manager.get_messages(&info.id).await;
```

### Edit mode cycle

```rust
// 切换 edit mode（Normal ↔ ThinkFirst）
let new_mode = session.cycle_edit_mode().await;
println!("Switched to: {}", new_mode);

// 获取 runtime state snapshot
let snapshot = session.runtime_state_snapshot().await;
println!("Current mode: {:?}", snapshot.edit_mode);
println!("Tool stats: {:?}", snapshot.tool_stats);
```

### Compaction

```rust
// 检查是否可 compact
if !session.is_compacting() {
    match session.compact().await {
        Ok(result) => {
            println!("Compacted: removed {} messages, saved ~{} tokens",
                     result.removed_count, result.tokens_saved);
        }
        Err(e) => {
            eprintln!("Compaction failed or not needed: {}", e);
        }
    }
}

// Compaction 后自动 continue_session（在 NativeBackend 中实现）
```

### ModelRegistry

```rust
use rozsa_app::model_registry::ModelRegistry;

// 加载 generated models + models.json
let mut registry = ModelRegistry::from_generated_with_models_json_path(
    Some(&cwd.join(".rozsa").join("models.json"))
)?;

// 动态发现 NVIDIA 模型（如果配置了 NVIDIA_API_KEY）
registry.merge_nvidia_models_if_configured().await?;

// 查找模型
let model = registry.find("anthropic", "claude-sonnet-4-20250514")
    .ok_or("Model not found")?;

// 获取首个可用模型
let first = registry.first_available()
    .ok_or("No configured models")?;

// 检查 provider 可用性
let available = registry.provider_available();
for (provider, info) in available {
    println!("{}: configured={}, source={:?}",
             provider, info.configured, info.source);
}
```

### SkillRegistry

```rust
use rozsa_app::skills::SkillRegistry;

// 加载 skills
let (registry, diagnostics) = SkillRegistry::load_from_defaults_with_diagnostics(&cwd);

// 打印加载错误
for diag in diagnostics {
    eprintln!("Failed to load skill at {}: {}", diag.path.display(), diag.error);
}

// 查找 skill
if let Some(skill) = registry.find_by_name("deep-research") {
    println!("Found skill: {} ({})", skill.name, skill.description);
}

// 格式化为 system prompt 片段
let prompt_fragment = registry.format_for_prompt();
println!("{}", prompt_fragment);
```

### PermissionPolicy

```rust
use rozsa_app::permissions::{PermissionMode, PermissionPolicy, PolicyVerdict};
use serde_json::json;

// 创建 policy
let policy = PermissionPolicy::new(
    PermissionMode::AutoApprove,
    vec![
        r"^Read:.*\.md$".to_string(),  // 自动允许读 markdown
        r"^Grep:".to_string(),         // 自动允许 grep
    ],
);

// 评估工具调用
let args = json!({ "command": "ls -la" });
match policy.evaluate("Bash", &args) {
    PolicyVerdict::Allow => {
        println!("Allowed");
    }
    PolicyVerdict::Block { reason } => {
        println!("Blocked: {}", reason);
    }
    PolicyVerdict::NeedApproval { info } => {
        println!("Need approval: {} (risk: {:?})", info.args_summary, info.risk);
        // 调用 backend approval UI...
        // policy.record_session_approval(info.trust_key);
    }
}
```

---

## 相关文档

- [rozsa-core API](../rozsa-core/README.md)
- [rozsa-model API](../rozsa-model/README.md)
- [rozsa-tui API](../rozsa-tui/README.md)
- [Session 文件格式](../specs/session-format.md)
- [Compaction 设计](../specs/compaction.md)
- [Native TUI Gap Audit](../NATIVE_TUI_GAP_AUDIT.md)

## 设计原则

1. **Separation of concerns**：agent loop logic（core）、应用状态（app）、UI（tui）清晰分离
2. **Async-first**：所有 I/O 和状态访问都是 async，锁粒度小（Mutex 只保护必要的共享状态）
3. **Event-driven**：通过 `broadcast::channel` 向 TUI 推送 `AgentEvent`，解耦 session 和 UI
4. **Append-only persistence**：JSONL session file 永不修改历史，只追加（tree structure via parent_id）
5. **Scope isolation**：subagent 通过 `SubagentScope` 限制工具访问，防止逃逸
6. **Lazy materialization**：session file 在首次 append 时才创建（`create_lazy`），减少空会话文件

## 未来扩展点

- [ ] Extension hooks 完整实现（目前只有骨架）
- [ ] Slash commands 完整实现（目前只解析 `/skill:name`）
- [ ] Subagent 嵌套（目前只支持一层）
- [ ] Session branching UI（利用 parent_id tree structure）
- [ ] Compaction 策略配置（目前只支持全局 threshold / target）
- [ ] Tool usage analytics（基于 `RuntimeState.tool_stats`）
