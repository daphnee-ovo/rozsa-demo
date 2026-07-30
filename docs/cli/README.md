# rozsa-cli — CLI Entry Point

## 概述

`rozsa-cli` 是 Rózsa AI 编码助手的二进制入口点，负责：

- 解析命令行参数
- 加载配置（settings、resources、CLAUDE.md）
- 初始化 model registry 与认证
- 创建 session manager
- 构建 `AgentSession`
- 启动 GUI 或执行单次 prompt

`rozsa-cli` 是应用程序的引导层，将 `rozsa-app`、`rozsa-core`、`rozsa-model` 和 `rozsa-gui` 组装成可运行的 CLI 工具。

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
- **全局配置根**: `ROZSA_CONFIG_DIR`，默认 `~/.rozsa/`
- **项目配置根**: `ROZSA_PROJECT_CONFIG_DIR`，默认 `<cwd>/.rozsa/`
- **全局配置** (`global_settings_path`): `ROZSA_CONFIG_DIR/settings.json`
- **项目配置** (`project_settings_path`): `ROZSA_PROJECT_CONFIG_DIR/settings.json`

### 5. 加载配置

```rust
let settings_manager = SettingsManager::load(
    global_settings_path,
    Some(project_settings_path),
    None, // 本地配置可选
)
```

**优先级**: CLI 参数 > 项目配置 `ROZSA_PROJECT_CONFIG_DIR/settings.json` > 全局配置 `ROZSA_CONFIG_DIR/settings.json`

配置文件语法或内容无效时会直接报告对应路径和错误，不会静默降级。

### 6. 解析模型 (Model Registry)

```rust
let registry = ModelRegistry::load_from_dirs(&[
    &config_roots.model_dirs()[0],
    &config_roots.model_dirs()[1],
])?;
```

**模型选择逻辑**:

1. 如果指定 `--model <id>`，从 registry 中查找该 ID
2. 否则使用 `settings_manager` 的 `default_provider` + `default_model`
3. 否则使用 `registry.first_available()` (第一个可用模型)
4. 如果没有可用模型，报错并提示配置 API key

### 7. 加载 Resources (CLAUDE.md 等)

```rust
let resource_loader =
    ResourceLoader::new(cwd.clone(), config_roots.resource_dirs().to_vec());
let resources = resource_loader.load().await.unwrap_or_default();
let system_prompt = ResourceLoader::build_system_prompt(&resources);
```

从以下位置加载：

- `<cwd>/CLAUDE.md` (项目级)
- `ROZSA_CONFIG_DIR/AGENTS.md` 或 `CLAUDE.md`（全局级）
- `ROZSA_PROJECT_CONFIG_DIR/AGENTS.md` 或 `CLAUDE.md`（项目配置覆盖）
- 其他自定义 resources

组装成最终的 system prompt。

### 8. 创建 Session Manager

```rust
let session_dirs = config_roots.session_dirs(&cwd);
let session_dir = &session_dirs[1];
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

**Session 存储路径**: 新会话默认写入 `ROZSA_CONFIG_DIR/sessions/<cwd-encoded>/<uuid>.jsonl`；读取时同时合并 `ROZSA_PROJECT_CONFIG_DIR/sessions/<cwd-encoded>/`，同 ID 由项目层覆盖。

其中 `<cwd-encoded>` 是将路径中的 `/` 替换为 `-` 后的结果。

### 9. 设置 Permission System

```rust
let permission_mode = PermissionMode::parse(&settings_manager.resolved().permissions.mode)
    .unwrap_or(PermissionMode::OnRequest);

let policy = Arc::new(PermissionPolicy::new(permission_mode));
```

支持的 permission modes:

- `on-request`（默认）：敏感工具调用按 `deny > ask > allow` 规则处理，未覆盖项请求用户批准
- `auto-approve`：预留给 small-model reviewer；当前设置接口会明确报未实现且不保存
- `yolo`：跳过普通审批，但不能绕过内置破坏性命令保护

通过 `pre_tool_use_hook` 在工具调用前插入权限检查逻辑。

### 10. 构建 AgentSession

```rust
let config = AgentSessionConfig {
    model,
    thinking_effort,
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
rozsa_gui::run(rozsa_gui::GuiConfig { /* GUI runtime resources */ }).await
```

启动 GUI。

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

当提供 prompt 时，执行单次对话并打印响应后退出；未提供时启动交互式 GUI。

### 可选参数

| 参数 | 短选项 | 说明 | 示例 |
|---|---|---|---|
| `--model <ID>` | `-m` | 指定模型 ID | `--model claude-opus-4-8` |

---

## 配置加载

### 配置优先级

1. **CLI 参数** (最高优先级)
   - `--model` 覆盖 settings 中的默认模型
2. **项目配置** (`ROZSA_PROJECT_CONFIG_DIR/settings.json`，默认 `<cwd>/.rozsa/settings.json`)
   - 项目级别的 settings、permissions、model defaults
3. **全局配置** (`ROZSA_CONFIG_DIR/settings.json`，默认 `~/.rozsa/settings.json`)
   - 用户级别的默认配置
4. **环境变量**
   - 通过 `ModelRegistry` 读取 `ANTHROPIC_API_KEY`、`OPENAI_API_KEY` 等

### 加载失败处理

配置文件不存在时使用内置默认值；文件存在但读取、JSON 解析或校验失败时，`SettingsManager::load` 会报告错误并阻止使用错误配置启动。

### 配置根布局

全局根和项目根使用完全一致的相对布局。读取顺序始终为全局后项目，因此项目同名项覆盖全局；不读取旧 `agent/` 层级。

```text
ROZSA_CONFIG_DIR/                  ROZSA_PROJECT_CONFIG_DIR/
├── models/                       ├── models/
├── themes/                       ├── themes/
├── settings.json                 ├── settings.json
├── sessions/                     ├── sessions/
├── skills/                       ├── skills/
├── AGENTS.md / CLAUDE.md         ├── AGENTS.md / CLAUDE.md
└── extensions/ ...               └── extensions/ ...
```

`agent/` 目录保留给未来用途。

---

## 文件布局

```
crates/rozsa-cli/
├── src/
│   ├── main.rs         # 入口：解析参数，创建 tokio runtime，调用 run()
│   ├── args.rs         # 命令行参数定义 (clap::Parser)
│   └── run.rs          # 核心启动逻辑：加载配置、组装 AgentSession、启动 GUI 或执行 prompt
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
  └─ rozsa-gui     (run 启动 GUI)
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
          └─ rozsa_gui::run(gui_config)
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
| `rozsa-gui` | `run`, `GuiConfig` | GUI 启动 |

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

- `ROZSA_CONFIG_DIR`: 覆盖全局配置根，默认通过 `HOME` 定位 `~/.rozsa/`
- `ROZSA_PROJECT_CONFIG_DIR`: 覆盖项目配置根，默认 `<cwd>/.rozsa/`
- `HOME` / `~`: 未设置 `ROZSA_CONFIG_DIR` 时用于定位全局配置根
  - 通过 `dirs_next::home_dir()` 获取
  - 如果无法获取且没有显式设置 `ROZSA_CONFIG_DIR`，启动会报告错误

---

## 使用示例

### 交互模式

```bash
# 使用默认模型启动 GUI
rozsa

# 指定模型启动 GUI
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

`rozsa-cli` 会报告配置文件路径及失败原因。可以通过以下方式检查配置：

```bash
cat "${ROZSA_CONFIG_DIR:-$HOME/.rozsa}/settings.json"
cat "${ROZSA_PROJECT_CONFIG_DIR:-.rozsa}/settings.json"
```

### 3. Session 文件存储位置

Session 历史从全局与项目 `sessions/<cwd-encoded>/` 合并读取，新会话默认写入全局配置根。

查看当前 session:

```bash
ls "${ROZSA_CONFIG_DIR:-$HOME/.rozsa}/sessions/"
```

### 4. Permission mode 配置

在 `settings.json` 中配置：

```json
{
  "permission": {
    "mode": "on-request",
    "deny": ["Bash(git push *)"],
    "ask": ["Edit(src/*)"],
    "allow": ["Read(*)"]
  },
  "tools": {
    "bash": false
  },
  "skills": {
    "release-check": true
  }
}
```

权限规则只使用 `deny`、`ask`、`allow` 三组 `Tool(target)` 条目。
`Tool(*)` 覆盖该工具的每一次调用，即使调用参数没有命令或文件路径。
禁止 `*(*)`；需要让所有工具跳过普通审批时使用 `yolo`。

规则列表按层覆盖：项目层声明某一列表时替换该全局列表，未声明时继承全局。
默认全局 allow 为 `ls(*)`、`grep(*)`、`find(*)`、`subagent(*)` 和
`askUserQuestion(*)`。普通 pattern 使用 glob；路径 `*` 匹配一层而 `**` 递归匹配。
项目路径相对项目根，全局文件路径规则必须以 `$HOME/` 开头且不能通过 `..` 逃逸。
`regex:` pattern 使用完整 RegExp 匹配；Bash 对每个拆分后的命令段匹配。

`tools` 与 `skills` 是按名称合并的布尔映射：项目
`ROZSA_PROJECT_CONFIG_DIR/settings.json` 只覆盖它声明的名称，未声明项继承全局层，
两层都未声明时默认启用。新 session 自动读取最新配置；已有 GUI session 使用
`/reload` 重新加载。

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

- [rozsa-app API 文档](../app/README.md) — `AgentSession`、`PermissionPolicy` 等核心组件
- [rozsa-model API 文档](../model/README.md) — `ModelRegistry`、Provider 注册
- [GUI 使用文档](../gui/UI_USAGE_GUIDELINES.md) — GUI 交互约定
- [rozsa-core API 文档](../core/README.md) — Agent Loop、事件流

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
    let config_roots = rozsa_app::config_paths::ConfigRoots::discover(&cwd)?;
    let [global_settings, project_settings] = config_roots.settings_paths();

    let settings_manager = SettingsManager::load(
        global_settings,
        Some(project_settings),
        None,
    )?;

    let [global_models, project_models] = config_roots.model_dirs();
    let registry = ModelRegistry::load_from_dirs(&[&global_models, &project_models])?;
    let model = registry.first_available().unwrap();

    let resource_loader =
        ResourceLoader::new(cwd.clone(), config_roots.resource_dirs().to_vec());
    let resources = resource_loader.load().await.unwrap_or_default();
    let system_prompt = ResourceLoader::build_system_prompt(&resources);

    let session_dir = config_roots.writable_session_dir(&cwd);
    let session_manager = SessionManager::create_lazy(
        &session_dir.join("test.jsonl"),
        "test-session".to_string(),
        cwd.to_string_lossy().to_string(),
        None,
    );

    let config = AgentSessionConfig {
        model,
        thinking_effort: None,
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
6. ✅ 根据模式启动 GUI 或执行单次 prompt

所有核心逻辑下沉到 `rozsa-app`、`rozsa-core`、`rozsa-model`，`rozsa-cli` 只负责**组装**与**启动**。
