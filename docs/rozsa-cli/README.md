# rozsa-cli — CLI Entry Point

## 概述

`rozsa-cli` 是 Rózsa AI 编码助手的二进制入口点，负责：

- 解析命令行参数
- 加载配置（settings、resources、CLAUDE.md）
- 初始化 model registry 与认证
- 创建 session manager
- 构建 `AgentSession`
- 启动 TUI 或执行单次 prompt

`rozsa-cli` 是应用程序的引导层，将 `rozsa-app`、`rozsa-core`、`rozsa-model` 和 `rozsa-tui` 组装成可运行的 CLI 工具。

---

## 启动流程

从 `main.rs` 开始，整体流程如下：

### 1. 解析命令行参数

```rust
let args = args::parse();  // 使用 clap::Parser
```

支持的参数见下方"命令行参数"章节。

### 2. 初始化 tokio 运行时

```rust
let runtime = tokio::runtime::Runtime::new()?;
runtime.block_on(async { run::run(&args).await })
```

所有异步逻辑由 `run()` 函数执行。

### 3. 注册内置 providers

```rust
rozsa_model::providers::register_builtin_providers();
```

注册 Anthropic、OpenAI 等 LLM provider。

### 4. 确定路径

- **当前工作目录** (`cwd`): 从 `std::env::current_dir()` 获取
- **agent 根目录** (`agent_dir`): `~/.rozsa/agent/`
- **全局配置** (`global_settings_path`): `~/.rozsa/agent/settings.json`
- **项目配置** (`project_settings_path`): `<cwd>/.claude/settings.json`

### 5. 加载配置

```rust
let settings_manager = SettingsManager::load(
    global_settings_path,
    Some(project_settings_path),
    None, // 本地配置可选
)
```

**优先级**: CLI 参数 > 项目配置 `.claude/settings.json` > 全局配置 `~/.rozsa/agent/settings.json`

失败时降级到空配置 (`/dev/null`)，确保不会因配置加载失败而崩溃。

### 6. 解析模型 (Model Registry)

```rust
let registry = ModelRegistry::from_generated_with_models_json_path(
    Some(&models_json_path),  // ~/.rozsa/agent/models.json
)?;
```

**模型选择逻辑**:

1. 如果指定 `--model <id>`，从 registry 中查找该 ID
2. 否则使用 `settings_manager` 的 `default_provider` + `default_model`
3. 否则使用 `registry.first_available()` (第一个可用模型)
4. 如果没有可用模型，报错并提示配置 API key

### 7. 加载 Resources (CLAUDE.md 等)

```rust
let resource_loader = ResourceLoader::new(cwd.clone(), agent_dir.clone());
let resources = resource_loader.load().await.unwrap_or_default();
let system_prompt = ResourceLoader::build_system_prompt(&resources);
```

从以下位置加载：

- `<cwd>/CLAUDE.md` (项目级)
- `~/.rozsa/agent/CLAUDE.md` (全局级)
- 其他自定义 resources

组装成最终的 system prompt。

### 8. 创建 Session Manager

```rust
let session_dir = agent_dir.join("sessions").join(format!("-{cwd_encoded}-"));
std::fs::create_dir_all(&session_dir)?;
let session_id = uuid::Uuid::new_v4().to_string();
let session_path = session_dir.join(format!("{session_id}.jsonl"));

let session_manager = SessionManager::create_lazy(
    &session_path,
    session_id,
    cwd.to_string_lossy().to_string(),
    None,
);
```

**Session 存储路径**: `~/.rozsa/agent/sessions/<cwd-encoded>/<uuid>.jsonl`

其中 `<cwd-encoded>` 是将路径中的 `/` 替换为 `-` 后的结果。

### 9. 设置 Permission System

```rust
let permission_mode = PermissionMode::parse(&settings_manager.resolved().permissions.mode)
    .unwrap_or(PermissionMode::OnRequest);

let policy = Arc::new(PermissionPolicy::new(
    permission_mode,
    auto_approve_patterns,
));
```

支持的 permission modes:

- `OnRequest` (默认): 每次工具调用都需要用户批准
- `Auto`: 使用 auto_approve_patterns 自动批准
- `Free`: 所有工具调用自动批准

通过 `pre_tool_use_hook` 在工具调用前插入权限检查逻辑。

### 10. 构建 AgentSession

```rust
let config = AgentSessionConfig {
    model,
    thinking_level,
    system_prompt,
    cwd: cwd.clone(),
    session_manager,
    settings_manager,
    resources,
    pre_tool_use: Some(pre_tool_use_hook),
};

let session = AgentSession::new(config);
session.register_default_tools(&cwd).await;
```

注册默认工具集 (Bash、Read、Edit、Write 等)。

### 11. 执行模式选择

**非交互模式** (提供了 prompt):

```rust
if let Some(ref prompt) = args.prompt {
    let events = session.prompt(prompt).await?;
    // 打印 assistant 的 text 响应
    return Ok(());
}
```

**交互模式** (未提供 prompt):

```rust
rozsa_tui::app::run_native_with(
    session,
    rozsa_tui::backend::native::NativeBackendConfig {
        model_registry: Some(Arc::new(registry)),
        session_dir: Some(session_dir),
        global_settings_path: Some(global_settings_path),
        pending_approvals: Some(pending_approvals),
        permission_request_rx: Some(perm_req_rx),
    },
)
.await
```

启动基于 `rozsa-tui` 的交互式终端界面。

---

## 命令行参数

使用 `clap` 解析，定义在 `args.rs`:

```bash
rozsa [OPTIONS] [PROMPT]
```

### 位置参数

| 参数 | 说明 | 必填 |
|---|---|---|
| `[PROMPT]` | 初始 prompt (非交互模式) | 否 |

当提供 prompt 时，执行单次对话并打印响应后退出；未提供时启动交互式 TUI。

### 可选参数

| 参数 | 短选项 | 说明 | 示例 |
|---|---|---|---|
| `--model <ID>` | `-m` | 指定模型 ID | `--model claude-opus-4-8` |

---

## 配置加载

### 配置优先级

1. **CLI 参数** (最高优先级)
   - `--model` 覆盖 settings 中的默认模型
2. **项目配置** (`<cwd>/.claude/settings.json`)
   - 项目级别的 settings、permissions、model defaults
3. **全局配置** (`~/.rozsa/agent/settings.json`)
   - 用户级别的默认配置
4. **环境变量**
   - 通过 `ModelRegistry` 读取 `ANTHROPIC_API_KEY`、`OPENAI_API_KEY` 等

### 加载失败处理

如果配置文件不存在或格式错误，`SettingsManager::load` 会降级到空配置 (`/dev/null`)，**不会阻塞启动**。

---

## 文件布局

```
crates/rozsa-cli/
├── src/
│   ├── main.rs         # 入口：解析参数，创建 tokio runtime，调用 run()
│   ├── args.rs         # 命令行参数定义 (clap::Parser)
│   └── run.rs          # 核心启动逻辑：加载配置、组装 AgentSession、启动 TUI
└── Cargo.toml          # 依赖声明
```

### 各文件职责

| 文件 | 职责 | 核心类型/函数 |
|---|---|---|
| `main.rs` | 程序入口 | `fn main()` |
| `args.rs` | 命令行参数 | `struct Args`, `fn parse()` |
| `run.rs` | 启动编排 | `async fn run(&Args)` |

**`main.rs`**:

- 调用 `args::parse()` 获取参数
- 创建 tokio runtime
- 调用 `run::run()` 执行主逻辑

**`args.rs`**:

- 使用 `clap::Parser` 定义 `Args` 结构体
- 暴露 `parse()` 函数返回解析后的参数

**`run.rs`**:

- 加载 settings、resources、models
- 构建 `AgentSession` 及其 config
- 设置 permission system 及 hook
- 根据是否有 prompt 决定进入非交互/交互模式

---

## 与其他 crate 的关系

### 依赖关系图

```
rozsa-cli
  ├─ rozsa-app     (初始化 AgentSession, SessionManager, PermissionPolicy)
  ├─ rozsa-core    (AgentEvent, PreToolUseContext)
  ├─ rozsa-model   (ModelRegistry, providers, types::Message)
  └─ rozsa-tui     (run_native_with 启动 TUI)
```

### 调用链

```
main.rs
  └─ args::parse()
  └─ run::run()
      ├─ rozsa_model::providers::register_builtin_providers()
      ├─ SettingsManager::load()
      ├─ ModelRegistry::from_generated_with_models_json_path()
      ├─ ResourceLoader::new().load()
      ├─ SessionManager::create_lazy()
      ├─ PermissionPolicy::new()
      ├─ AgentSession::new()
      ├─ session.register_default_tools()
      └─ 如果无 prompt:
          └─ rozsa_tui::app::run_native_with(session, backend_config)
      └─ 如果有 prompt:
          └─ session.prompt(prompt).await
```

### 类型桥接

| 来源 crate | 类型 | 用途 |
|---|---|---|
| `rozsa-app` | `AgentSession`, `AgentSessionConfig` | 创建 agent session |
| `rozsa-app` | `PermissionPolicy`, `PermissionMode` | 权限管理 |
| `rozsa-app` | `SettingsManager`, `ResourceLoader` | 配置与资源加载 |
| `rozsa-model` | `ModelRegistry`, `Message` | 模型解析与消息类型 |
| `rozsa-core` | `AgentEvent`, `PreToolUseContext` | 事件流与 hook 上下文 |
| `rozsa-tui` | `run_native_with`, `NativeBackendConfig` | TUI 启动 |

---

## 环境变量

### LLM Provider API Keys

`ModelRegistry` 在初始化时自动读取以下环境变量：

| 环境变量 | Provider | 用途 |
|---|---|---|
| `ANTHROPIC_API_KEY` | Anthropic (Claude) | Claude 系列模型认证 |
| `OPENAI_API_KEY` | OpenAI | GPT 系列模型认证 |

如果未设置任何 API key 且未配置自定义模型，启动时会报错：

```
No model available. Configure a provider API key (ANTHROPIC_API_KEY, OPENAI_API_KEY, etc.) or specify --model.
```

### 其他环境变量

- `HOME` / `~`: 用于定位 `~/.rozsa/agent/` 目录
  - 通过 `dirs_next::home_dir()` 获取
  - 如果无法获取，降级到当前目录 `.`

---

## 使用示例

### 交互模式

```bash
# 使用默认模型启动 TUI
rozsa

# 指定模型启动 TUI
rozsa --model claude-opus-4-8
```

### 非交互模式

```bash
# 执行单次 prompt 并退出
rozsa "解释这段代码的作用"

# 指定模型执行 prompt
rozsa --model claude-opus-4-8 "重构这个函数"
```

### 配置 API Key

```bash
# 临时设置
export ANTHROPIC_API_KEY="sk-ant-..."
rozsa

# 持久化到 shell 配置
echo 'export ANTHROPIC_API_KEY="sk-ant-..."' >> ~/.bashrc
```

---

## 常见问题

### 1. 启动时报错 "No model available"

**原因**: 未设置任何 LLM provider 的 API key。

**解决**:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
# 或
export OPENAI_API_KEY="sk-..."
```

### 2. 配置文件加载失败

`rozsa-cli` 会降级到空配置，**不会阻塞启动**。可以通过以下方式检查配置：

```bash
cat ~/.rozsa/agent/settings.json
cat .claude/settings.json
```

### 3. Session 文件存储位置

Session 历史存储在 `~/.rozsa/agent/sessions/<cwd-encoded>/<uuid>.jsonl`。

查看当前 session:

```bash
ls ~/.rozsa/agent/sessions/
```

### 4. Permission mode 配置

在 `settings.json` 中配置：

```json
{
  "permissions": {
    "mode": "OnRequest",  // 或 "Auto", "Free"
    "auto_approve_patterns": ["Bash:read_*", "Read"]
  }
}
```

---

## 扩展点

### 添加新的 CLI 参数

在 `args.rs` 中扩展 `Args` 结构体：

```rust
#[derive(Parser, Debug)]
pub struct Args {
    pub prompt: Option<String>,

    #[arg(short, long)]
    pub model: Option<String>,

    // 新增参数
    #[arg(long)]
    pub debug: bool,
}
```

### 添加新的 pre_tool_use hook

在 `run.rs` 中的 `pre_tool_use_hook` 闭包中添加逻辑：

```rust
let hook: Box<dyn Fn(PreToolUseContext) -> ...> = Box::new(move |ctx| {
    // 自定义权限检查逻辑
});
```

### 自定义 Resource Loader

替换 `ResourceLoader` 的实现，加载自定义资源格式。

---

## 相关文档

- [rozsa-app API 文档](../rozsa-app/README.md) — `AgentSession`、`PermissionPolicy` 等核心组件
- [rozsa-model API 文档](../rozsa-model/README.md) — `ModelRegistry`、Provider 注册
- [rozsa-tui API 文档](../rozsa-tui/README.md) — TUI 启动与 Backend
- [rozsa-core API 文档](../rozsa-core/README.md) — Agent Loop、事件流

---

## 代码示例

### 最小化启动流程 (去除权限系统)

```rust
use rozsa_app::agent_session::{AgentSession, AgentSessionConfig};
use rozsa_app::model_registry::ModelRegistry;
use rozsa_app::resources::ResourceLoader;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::SettingsManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rozsa_model::providers::register_builtin_providers();

    let cwd = std::env::current_dir()?;
    let agent_dir = dirs_next::home_dir()
        .unwrap()
        .join(".rozsa")
        .join("agent");

    let settings_manager = SettingsManager::load(
        agent_dir.join("settings.json"),
        None,
        None,
    )?;

    let registry = ModelRegistry::from_generated_with_models_json_path(
        Some(&agent_dir.join("models.json")),
    )?;
    let model = registry.first_available().unwrap();

    let resource_loader = ResourceLoader::new(cwd.clone(), agent_dir.clone());
    let resources = resource_loader.load().await.unwrap_or_default();
    let system_prompt = ResourceLoader::build_system_prompt(&resources);

    let session_manager = SessionManager::create_lazy(
        &agent_dir.join("sessions").join("test.jsonl"),
        "test-session".to_string(),
        cwd.to_string_lossy().to_string(),
        None,
    );

    let config = AgentSessionConfig {
        model,
        thinking_level: None,
        system_prompt,
        cwd: cwd.clone(),
        session_manager,
        settings_manager,
        resources,
        pre_tool_use: None,
    };

    let session = AgentSession::new(config);
    session.register_default_tools(&cwd).await;

    let events = session.prompt("Hello!").await?;
    // 处理 events...

    Ok(())
}
```

---

## 总结

`rozsa-cli` 是整个 Rózsa 项目的**引导层**，职责清晰：

1. ✅ 解析 CLI 参数
2. ✅ 加载配置与资源
3. ✅ 初始化 model registry
4. ✅ 组装 `AgentSession`
5. ✅ 设置 permission system
6. ✅ 根据模式启动 TUI 或执行单次 prompt

所有核心逻辑下沉到 `rozsa-app`、`rozsa-core`、`rozsa-model`，`rozsa-cli` 只负责**组装**与**启动**。
