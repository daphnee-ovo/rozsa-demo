# Settings 迁移计划

本文定义 settings 管理从 TypeScript 迁移到 Rust 的详细计划。

Settings 系统负责分层配置加载、合并、持久化和动态更新。

相关代码：
- TS: `packages/coding-agent/src/core/settings-manager.ts` (1268 行)
- TS: `packages/coding-agent/src/core/config.ts` (agent dir, paths)
- Rust: `crates/rozsa-app/src/settings/mod.rs` (TODO)

相关文档：
- [主文档](./rozsa-app-migration.md)
- [Session 迁移](./session-migration.md)

## Settings 层级

### 层级结构

TS 参考点: `settings-manager.ts` -> SettingsManager 构造、merge 逻辑

```text
优先级（高到低）：
1. Runtime override (code-set, not persisted)
2. Local settings (~/.claude/settings.local.json)
3. Project settings (.claude/settings.json)
4. Global settings (~/.claude/settings.json)
5. Default values (hardcoded)
```

每一层都是完整 Settings 对象的 partial overlay。高优先级覆盖低优先级。

### 文件路径

```text
Global:  ~/.rozsa-agent/settings.json     (或 ~/.claude/settings.json 兼容)
Project: {cwd}/.claude/settings.json
Local:   ~/.rozsa-agent/settings.local.json
```

### Merge 语义

- 标量字段: 高层覆盖低层
- 数组字段: 高层完全替代低层（不合并）
- 对象字段: 递归 merge（field-level override）
- `null` 值: 显式删除该 key

Rust 目标:

```rust
pub struct SettingsManager {
    global: PartialSettings,
    project: PartialSettings,
    local: PartialSettings,
    runtime: PartialSettings,
    merged: Settings,  // cached merged result
}

impl SettingsManager {
    pub fn get<T>(&self, key: SettingsKey) -> T { ... }
    pub fn set(&mut self, layer: SettingsLayer, key: SettingsKey, value: Value) { ... }
    pub fn persist(&self, layer: SettingsLayer) -> Result<()> { ... }
}
```

## Settings Schema

### 核心配置字段

TS 参考点: `settings-manager.ts` -> Settings interface

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    // Model
    pub model: Option<String>,
    pub small_model: Option<String>,
    pub thinking_level: Option<ThinkingLevel>,

    // Compaction
    pub compaction: Option<CompactionSettings>,

    // Retry
    pub retry: Option<RetrySettings>,

    // Terminal
    pub terminal: Option<TerminalSettings>,

    // Permissions
    pub permissions: Option<PermissionSettings>,

    // Extensions
    pub extensions: Option<Vec<String>>,

    // Theme
    pub theme: Option<String>,

    // Provider overrides
    pub providers: Option<HashMap<String, ProviderConfig>>,

    // Custom settings (extension-defined)
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionSettings {
    pub enabled: Option<bool>,
    pub threshold: Option<f64>,  // 0.0-1.0, fraction of context window
    pub overflow_strategy: Option<OverflowStrategy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrySettings {
    pub enabled: Option<bool>,
    pub max_retries: Option<u32>,
    pub delay_ms: Option<u64>,
    pub backoff_multiplier: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSettings {
    pub line_limit: Option<usize>,
    pub byte_limit: Option<usize>,
    pub truncate_strategy: Option<TruncateStrategy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionSettings {
    pub mode: Option<PermissionMode>,
    pub whitelist: Option<Vec<WhitelistRule>>,
    pub auto_reviewer_model: Option<String>,
}
```

当前 Rust 实现使用 `defaultModel` 和 `smallModel`。`smallModel` 是可选的
辅助模型 id；session title 请求固定使用 Low reasoning，不继承主会话的
thinking level。session 自动命名对少于 8 个词且满足字符限制的输入直接
使用规范化原文，长输入才并发调用 `smallModel`。未配置、模型不可用或
请求失败时保留首条消息 preview，不回退到主模型。

### Partial Settings (per layer)

```rust
/// 每一层的 partial settings，所有字段都是 Option
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialSettings {
    pub model: Option<String>,
    pub small_model: Option<String>,
    pub thinking_level: Option<ThinkingLevel>,
    pub compaction: Option<CompactionSettings>,
    pub retry: Option<RetrySettings>,
    pub terminal: Option<TerminalSettings>,
    pub permissions: Option<PermissionSettings>,
    pub extensions: Option<Vec<String>>,
    pub theme: Option<String>,
    pub providers: Option<HashMap<String, ProviderConfig>>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}
```

## Dynamic Settings (Mid-Session Changes)

### Model Switch

TS 参考点: `agent-session.ts` -> `setModel()`, `cycleModel()`

当用户 mid-session 切换 model 时：
1. 验证 auth availability
2. 更新 agent.state.model
3. Clamp thinking level to model capabilities
4. 持久化到 SessionManager (model change entry)
5. 持久化到 SettingsManager (记住选择)
6. Emit extension event

### Thinking Level Change

TS 参考点: `agent-session.ts` -> `setThinkingLevel()`

1. Clamp to model capabilities (Off, Low, Medium, High)
2. 更新 agent config
3. Emit extension event

### Runtime Override

某些 settings 只在当前 session 有效，不持久化：
- active tool list (enable/disable)
- compaction override
- streaming behavior

## Provider Settings

### Provider Config

TS 参考点: `model-registry.ts` -> ProviderConfigInput

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub api_key: Option<String>,          // 直接 key 或 env: 引用
    pub base_url: Option<String>,
    pub max_tokens_override: Option<usize>,
    pub extra_headers: Option<HashMap<String, String>>,
    pub models: Option<Vec<ProviderModelOverride>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelOverride {
    pub id: String,
    pub display_name: Option<String>,
    pub context_window: Option<usize>,
    pub max_tokens: Option<usize>,
    pub cost: Option<ModelCost>,
}
```

### API Key Resolution

TS 参考点: `model-registry.ts` -> `getApiKeyAndHeaders()`

Resolution order:
1. `providers.{name}.apiKey` in settings (supports `env:VAR_NAME` syntax)
2. Environment variable (provider-specific, e.g., `ANTHROPIC_API_KEY`)
3. OAuth token (from AuthStorage, if configured)

```rust
impl SettingsManager {
    pub fn resolve_api_key(&self, provider: &str) -> Option<String> {
        // 1. Check provider config in merged settings
        if let Some(providers) = &self.merged.providers {
            if let Some(config) = providers.get(provider) {
                if let Some(key) = &config.api_key {
                    return Some(resolve_config_value(key));
                }
            }
        }
        // 2. Check env var
        get_env_api_key(provider)
    }
}

fn resolve_config_value(value: &str) -> String {
    if let Some(var) = value.strip_prefix("env:") {
        std::env::var(var).unwrap_or_default()
    } else {
        value.to_string()
    }
}
```

## File Storage

### File Locking

TS 参考点: `settings-manager.ts` -> `FileSettingsStorage.withLock()`

TS 使用 `proper-lockfile` 库进行文件锁定。

Rust 目标: `fd-lock` crate 或 `file-lock` crate

```rust
pub struct FileSettingsStorage {
    path: PathBuf,
}

impl FileSettingsStorage {
    pub async fn read(&self) -> Result<PartialSettings> {
        let content = tokio::fs::read_to_string(&self.path).await?;
        Ok(serde_json::from_str(&content)?)
    }

    pub async fn write(&self, settings: &PartialSettings) -> Result<()> {
        let content = serde_json::to_string_pretty(settings)?;
        // atomic write: write to tmp + rename
        let tmp = self.path.with_extension("tmp");
        tokio::fs::write(&tmp, &content).await?;
        tokio::fs::rename(&tmp, &self.path).await?;
        Ok(())
    }
}
```

### Settings Migration

TS 参考点: `settings-manager.ts` -> migration logic

旧版 settings 可能有已弃用的字段名，需要 migration：
- `model_name` -> `model`
- `auto_permission` -> `permissions.mode = "auto-permission"`

```rust
fn migrate_settings(raw: &mut Value) {
    if let Some(obj) = raw.as_object_mut() {
        // rename deprecated fields
        if let Some(v) = obj.remove("model_name") {
            obj.entry("model").or_insert(v);
        }
        // ... other migrations
    }
}
```

## 迁移任务

### SETTINGS-001: Settings schema 类型定义

参考点: `settings-manager.ts` -> Settings interface, CompactionSettings, RetrySettings, etc.

迁移动作:
- 定义 Settings struct (所有字段)
- 定义 PartialSettings struct (所有 Option)
- 定义 CompactionSettings, RetrySettings, TerminalSettings, PermissionSettings
- 定义 ProviderConfig, ProviderModelOverride
- 定义 SettingsLayer enum (Global, Project, Local, Runtime)
- 实现 serde JSON 序列化

优化点:
- 使用 Rust enum 保证字段完备性
- Settings merge 用类型系统保证不漏字段

完整性测试:
- 用 TS 现有 settings.json files 作为 fixture
- Rust 读取后字段值与 TS SettingsManager.get() 一致

### SETTINGS-002: Settings merge 逻辑

参考点: `settings-manager.ts` -> merge logic

迁移动作:
- 实现 PartialSettings merge (global + project + local + runtime)
- 标量: 高层覆盖
- 数组: 高层替代
- 对象: 递归 merge
- null: 删除

优化点:
- cached merged result, invalidate on layer change

完整性测试:
- fixture: 4 层 settings，merge 后与 TS 结果一致
- edge case: null override, empty array, nested object merge

### SETTINGS-003: File I/O with locking

参考点: `settings-manager.ts` -> FileSettingsStorage

迁移动作:
- 实现 async file read
- 实现 atomic write (tmp + rename)
- 实现 file lock (cross-process safety)
- 实现 file watch (detect external changes)

优化点:
- atomic write 避免 corruption
- lock timeout 避免 deadlock

完整性测试:
- concurrent read/write 不 corrupt
- external edit detected and reloaded
- lock timeout triggers error (not hang)

### SETTINGS-004: Settings migration

参考点: `settings-manager.ts` -> migration logic

迁移动作:
- 实现 deprecated field rename
- 实现 format upgrade
- migration 在 read 时自动执行
- migration 后 write back (if changed)

优化点:
- migration 幂等
- migration version tracked

完整性测试:
- old format settings 经 migration 后与 TS 结果一致
- already-migrated settings 不变

### SETTINGS-005: SettingsManager public API

参考点: `settings-manager.ts` -> get(), set(), persist()

迁移动作:
- 实现 get(key) -> typed value
- 实现 set(layer, key, value)
- 实现 persist(layer) -> write to file
- 实现 reload(layer) -> re-read from file
- 实现 on_change callback

优化点:
- type-safe get (返回具体类型而非 Value)
- change notification for reactive update

完整性测试:
- get 返回 merged value
- set 后 get 返回新值
- persist 后 re-read 一致
- layer isolation (project set 不影响 global)

### SETTINGS-006: Dynamic settings (model switch, thinking level)

参考点: `agent-session.ts` -> setModel(), setThinkingLevel()

迁移动作:
- 实现 model switch (validate + clamp + persist)
- 实现 thinking level change (clamp to model caps)
- 实现 settings event emission

优化点:
- model switch validation 包含 auth check

完整性测试:
- model switch 后 settings 和 session 都记录
- invalid model 返回 error (不 silent fallback)
- thinking level clamp 到 model supported levels
