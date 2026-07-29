# Rózsa

AI coding agent — Rust + Tauri 实现。

## 开发

- 检查：`cargo build && cargo clippy && cargo test`
- Release：`cargo build -p rozsa-cli --release`
- 链接器：mold（`.cargo/config.toml`）

## 架构约束

### Crate 分层

`rozsa-model` → `rozsa-core` → `rozsa-app` → `rozsa-gui` / `rozsa-cli`

- model：无状态，只定义 types + provider streaming
- core：agent loop + events + tool trait，不知道 session/settings
- app：AgentSession 编排层（session、permissions、model registry、settings），供 GUI 和 CLI 使用
- gui：唯一的交互式前端；cli：一次性 prompt 和 GUI 启动入口

### AgentSession 单实例限制

`AgentSession` 内部的 `session_manager` 是共享 mutable 的。`switch_session()` 直接替换 session_manager 指向。**如果 agent loop 还在跑，`persist_new_messages` 会写到新 session 的文件**。GUI 通过每个 session tab 使用独立实例来避免这个问题。

### AgentEvent 累积模型

- `AgentStart` → 标记 turn 起点
- `MessageUpdate` → 携带**完整累积**的 AssistantMessage（不是 delta）
- `AgentEnd` → 携带权威 messages 列表，必须 truncate 到 turn_base 再 extend（不能 append，否则重复）

### 权限系统

- `pre_tool_use` hook（`AgentSessionConfig`）→ `PermissionPolicy::evaluate` → `PolicyVerdict::NeedApproval`
- oneshot channel：hook await 阻塞 agent loop，UI 端 send 回应
- `AllowSession { trust_key }` 持久化到 settings.json 实现跨会话信任
- DashMap `PendingApprovals` 是共享的（CLI 创建，传给 UI 层）

### Session 持久化

JSONL 格式。SessionManager 管理读写。`entries()` 返回历史消息。新消息通过 `append_message` 追加。会话元数据（name、parent）通过 `append_session_info` 更新。

### GUI 多会话隔离

每个 session tab 有独立的 `AgentSession` 实例（懒加载）。三态：`Idle` → `Loaded` → `Active`。事件转发 per-tab，只在该 tab 为当前视图时 emit 给前端。权限请求跟 session 走。

### Tauri 注意事项

- `"withGlobalTauri": true` 在 `app` section，否则 `window.__TAURI__` 不注入
- `capabilities/default.json` 需要 `core:default` + `core:event:*`
- tokio runtime 内不能 `blocking_lock()`，用 `try_lock()` 或 async
- icon.png 必须是 RGBA PNG

## 关键文档

| 文档 | 用途 |
|------|------|
| `AGENTS.md` | 开发规则、Git 规范、核心原则 |
| `docs/TODO.md` | 长线开发规划 |
| `docs/gui/UI_USAGE_GUIDELINES.md` | GUI 设计规范（色板、组件、交互规则） |
| `docs/gui/ARCHITECTURE.md` | GUI 技术架构（IPC 协议、状态模型） |
| `docs/model/models-config.md` | 模型配置格式（providers JSON schema） |
| `docs/model/supported-providers.md` | 支持的 LLM 提供商列表 |
| `docs/model/oauth-architecture.md` | OAuth 登录流程设计 |
| `docs/MIGRATION_RESIDUE_AUDIT.md` | 已移除迁移残留及受控保留项的审计记录 |
