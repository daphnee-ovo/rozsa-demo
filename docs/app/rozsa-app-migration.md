# rozsa-app 迁移计划

本文定义从 TypeScript coding-agent runtime 迁移到 Rust rozsa-app 的完整计划。

rozsa-app 是应用运行时层，位于 rozsa-core（纯状态机引擎）之上、rozsa-tui/rozsa-cli 之下。它组合 core engine 与产品逻辑：session 持久化、settings 管理、permission 执行、extension 生命周期、built-in tools、compaction、resources 加载、skills 匹配。

相关代码：
- Rust: `crates/rozsa-app/`
- TypeScript: `packages/coding-agent/src/core/`

相关文档：
- [rozsa-core 迁移计划](../core/rozsa-core-migration.md)
- [Session 迁移](./session-migration.md)
- [Settings 迁移](./settings-migration.md)
- [Extensions 迁移](./extensions-migration.md)
- [Tools 迁移](./tools-migration.md)
- [任务拆分](./task-breakdown.md)

## 目标

rozsa-app 迁移的目标是：将 TypeScript coding-agent runtime 中的产品逻辑逐步迁移到 Rust，使 Rust 成为 agent session 的完整运行时，TypeScript 仅作为过渡期兼容层。

最终运行方向：

```text
rozsa-cli (entry point)
  -> rozsa-app AgentSession (Rust)
    -> rozsa-core agent_loop (Rust)
    -> rozsa-model stream (Rust)
    -> Built-in Tools (Rust)
    -> PermissionGuard (Rust)
    -> SettingsManager (Rust)
    -> SessionManager (Rust, JSONL persistence)
    -> ExtensionRunner (Rust + plugin boundary)
    -> ResourceLoader (Rust)
    -> CompactionEngine (Rust, 调用 model stream)
  -> rozsa-tui (Rust, ratatui)
```

迁移不是一步到位。中间态保持 TypeScript AgentSession 作为 host，逐步将子系统切换到 Rust 实现。

必须保持：

- Agent session 完整生命周期（create, prompt, continue, abort, compact, export）
- Session JSONL 持久化格式兼容
- Settings 层级合并语义（global -> project -> local -> runtime）
- Permission 三模式行为（on-request, auto-permission, free-permission）
- Extension hooks 执行顺序和语义
- Built-in tools 行为（bash, read, edit, write, grep, find, ls, subagent）
- Compaction 触发条件和 summary 生成
- Resource loading（CLAUDE.md, AGENTS.md, system prompt fragments）
- Model registry 和 model resolution

## 范围

### 本次迁移包含

- AgentSession lifecycle（create, prompt, continue, abort, compact）
- SessionManager（JSONL 持久化、tree navigation、context build）
- SettingsManager（global/project/local 层级、动态更新）
- PermissionGuard（规则匹配、risk analysis、decision flow）
- Built-in tools（bash, read, edit, write, grep, find, ls）
- ResourceLoader（CLAUDE.md, AGENTS.md, system prompt assembly）
- CompactionEngine（trigger, prepare, summary, rebuild）
- RuntimeState（TUI snapshot）
- Model resolution（registry + auth + provider availability, 已部分完成）
- Product-level messages（custom message types）
- Skills matcher（system prompt assembly from skills）

### 不迁

| 不迁项 | 原因 |
| --- | --- |
| Extension runtime 的 TS module 加载 | 现有 extensions 是 TS 模块，需要 Node.js 运行时 |
| OAuth interactive flow | 依赖浏览器交互和 HTTP redirect |
| UI components（Ink/React） | 已由 rozsa-tui 替代 |
| Subagent orchestration | 复杂且依赖多进程调度，单独规划 |
| Package manager（npm/git loading） | 依赖 Node.js 生态 |
| HTML export | 低优先级辅助功能 |
| LSP client | 重量级，需独立迁移 |
| Native binary extension mode | IDE 集成协议，后续单独处理 |
| RPC mode | JSON-RPC server，后续单独处理 |

## 当前事实

### TypeScript 当前状态

TypeScript coding-agent runtime 是一个 4000+ 行的巨型 AgentSession 类，加上：
- session-manager.ts (1474 行): JSONL session 持久化 + tree navigation
- settings-manager.ts (1268 行): 分层配置管理
- permissions.ts (1294 行): 三模式权限系统
- model-registry.ts (964 行): model 发现 + auth 解析
- resource-loader.ts (928 行): 项目上下文加载
- compaction/compaction.ts (500 行): context compaction
- extensions/runner.ts (400 行): extension 生命周期
- tools/ (16 files): bash, read, edit, write, grep, find, ls, subagent 等

关键入口：
- `sdk.ts` 的 `createAgentSession()` 是 session 创建工厂
- `agent-session.ts` 的 `AgentSession` 类是核心运行时
- `agent-session-runtime.ts` 的 `_buildRuntime()` 负责 tool registry 和 system prompt 构建

### Rust 当前状态

- `rozsa-app/src/model_registry/mod.rs` (891 行): **已实现**。model 元数据、models.json 合并、NVIDIA 发现、provider auth 检测
- `rozsa-app/src/main.rs`: **已实现**。JSONL stdio bridge（ListModels, ListImageModels）
- `rozsa-app/src/session/mod.rs`: TODO 骨架
- `rozsa-app/src/permissions/mod.rs`: TODO 骨架
- `rozsa-app/src/settings/mod.rs`: TODO 骨架
- `rozsa-app/src/tools/mod.rs`: TODO 骨架
- `rozsa-app/src/skills/mod.rs`: TODO 骨架
- `rozsa-app/src/extensions/mod.rs`: TODO 骨架
- `rozsa-app/src/resources/mod.rs`: TODO 骨架
- `rozsa-app/src/compaction/mod.rs`: TODO 骨架
- `rozsa-app/src/messages.rs`: TODO 骨架
- `rozsa-app/src/runtime_state.rs`: TODO 骨架

### rozsa-core 当前状态（依赖项）

rozsa-core agent_loop 已实现完整功能：
- prompt 和 continue lifecycle
- stream 消费 (start, delta, done, error)
- abort (CancellationToken)
- steering queue 和 follow-up queue
- sequential 和 parallel tool scheduling
- before/after tool call hooks
- should_stop_after_turn 和 prepare_next_turn hooks
- AgentEvent 发射（完整序列化支持）

## 迁移参考点

### TypeScript 行为参考点

| 参考点 | TS 位置 | 迁移目标 | 必须保留 |
| --- | --- | --- | --- |
| createAgentSession | `sdk.ts` | `rozsa_app::session::AgentSession::new()` | model resolution、tool registry、system prompt build |
| AgentSession.prompt | `agent-session.ts` | `AgentSession::prompt()` | extension input hook、compaction check、message build |
| AgentSession._handleAgentEvent | `agent-session.ts` | `AgentSession` event subscription | session persistence、runtime state update |
| SessionManager | `session-manager.ts` | `rozsa_app::session::SessionManager` | JSONL format、tree structure、context build |
| SettingsManager | `settings-manager.ts` | `rozsa_app::settings::SettingsManager` | global/project/local merge、file locking |
| PermissionManager | `permissions.ts` | `rozsa_app::permissions::PermissionGuard` | 三模式、blacklist、whitelist、risk levels |
| ModelRegistry | `model-registry.ts` | `rozsa_app::model_registry` (已完成) | model lookup、auth resolution |
| ResourceLoader | `resource-loader.ts` | `rozsa_app::resources::ResourceLoader` | CLAUDE.md loading、collision detection |
| CompactionEngine | `compaction/compaction.ts` | `rozsa_app::compaction::CompactionEngine` | trigger logic、summary prompt、rebuild |
| ExtensionRunner | `extensions/runner.ts` | `rozsa_app::extensions::ExtensionRunner` | hook lifecycle、tool registration |
| Built-in tools | `tools/*.ts` | `rozsa_app::tools::*` | 所有 tool 行为不变 |
| RuntimeStateStore | `agent-session.ts` 内 | `rozsa_app::runtime_state::RuntimeState` | TUI snapshot fields |

### Rust 落点参考点

| Rust 落点 | 当前状态 | 目标 |
| --- | --- | --- |
| `rozsa_app::session` | TODO | AgentSession + SessionManager |
| `rozsa_app::settings` | TODO | SettingsManager + schema types |
| `rozsa_app::permissions` | TODO | PermissionGuard + PermissionMode |
| `rozsa_app::tools` | TODO | bash/read/edit/write/grep/find/ls |
| `rozsa_app::extensions` | TODO | ExtensionRunner + hook dispatch |
| `rozsa_app::resources` | TODO | ResourceLoader |
| `rozsa_app::compaction` | TODO | CompactionEngine |
| `rozsa_app::skills` | TODO | SkillMatcher + prompt assembly |
| `rozsa_app::runtime_state` | TODO | RuntimeState snapshot |
| `rozsa_app::messages` | TODO | Product custom message types |
| `rozsa_app::model_registry` | **已完成** | model metadata + auth detect |

## 迁移原则

1. **依赖顺序迁移**：先迁 leaf 模块（settings、session persistence），再迁中间模块（permissions、tools），最后迁 orchestration（AgentSession）。
2. **先数据后行为**：先迁类型定义和 schema，再迁读写逻辑，最后迁运行时行为。
3. **保持格式兼容**：session JSONL 格式、settings.json 格式必须与 TS 双向兼容。
4. **显式 backend 切换**：每个子系统都通过环境变量控制 ts/rust backend。不引入 auto。
5. **独立可验证闭环**：每个任务独立验证，不需要整体迁移才能跑。
6. **core 是纯引擎**：rozsa-core 不含产品逻辑。session persistence、permission check、extension hooks 全部在 rozsa-app。
7. **extension 后迁**：extension 系统是最复杂的部分，也是唯一需要跨语言 boundary 的部分。第一阶段保持 TS extension runtime。
8. **tool 与 permission 共迁**：tool 和 permission 高度耦合，一起迁移避免 boundary 来回跳。

## 目标架构

```text
rozsa-app
├── session/
│   ├── AgentSession       # 核心运行时，组合 core Agent + 产品服务
│   ├── SessionManager     # JSONL 持久化 + tree navigation
│   └── SessionEntry       # entry 类型定义
├── settings/
│   ├── SettingsManager    # global/project/local 合并
│   ├── Settings           # schema structs
│   └── FileStorage        # file I/O with locking
├── permissions/
│   ├── PermissionGuard    # 统一入口
│   ├── PermissionMode     # on-request / auto / free
│   ├── RiskAnalyzer       # risk level 推断
│   ├── Whitelist          # 默认和用户白名单
│   └── Blacklist          # 硬编码黑名单
├── tools/
│   ├── bash.rs            # shell 执行
│   ├── read.rs            # 文件读取
│   ├── edit.rs            # 文件编辑
│   ├── write.rs           # 文件创建
│   ├── grep.rs            # 内容搜索
│   ├── find.rs            # 文件查找
│   └── ls.rs              # 目录列表
├── extensions/
│   ├── ExtensionRunner    # hook dispatch
│   ├── ExtensionHook      # hook 类型定义
│   └── PluginBoundary     # TS extension bridge (过渡期)
├── compaction/
│   ├── CompactionEngine   # trigger + prepare + rebuild
│   └── BranchSummary      # branch summarization
├── resources/
│   ├── ResourceLoader     # CLAUDE.md / AGENTS.md loading
│   └── SystemPrompt       # prompt assembly
├── skills/
│   └── SkillMatcher       # skill discovery + prompt injection
├── model_registry/        # 已实现
│   └── ModelRegistry      # model metadata + auth + NVIDIA
├── messages.rs            # product custom message types
└── runtime_state.rs       # TUI snapshot
```

## Backend 切换策略

### 子系统级 backend

不同于 rozsa-core 的单一 `ROZSA_CORE_BACKEND`，rozsa-app 采用子系统级 backend 切换：

```text
ROZSA_APP_SESSION_BACKEND=ts|rust
ROZSA_APP_SETTINGS_BACKEND=ts|rust
ROZSA_APP_TOOLS_BACKEND=ts|rust
ROZSA_APP_PERMISSIONS_BACKEND=ts|rust
```

理由：app 子系统相互依赖但可独立迁移。settings 迁完不需要等 permissions 完成才用。

### 整体切换

当所有子系统都稳定后，合并为单一环境变量：

```text
ROZSA_APP_BACKEND=ts|rust
```

### Rollout 阶段

| 阶段 | 默认 backend | 说明 |
| --- | --- | --- |
| Phase 1: 数据层 | ts | settings、session persistence 可独立设 rust |
| Phase 2: 执行层 | ts | tools、permissions 可独立设 rust |
| Phase 3: 编排层 | ts | AgentSession 可设 rust |
| Phase 4: 默认 Rust | rust | 保留 ts 回滚 |
| Phase 5: 清理 | rust | 稳定后删除 TS runtime |

## 完成标准

rozsa-app 迁移完成需要同时满足：

- Rust AgentSession 是默认 runtime
- TypeScript AgentSession 保留至少一个迭代用于回滚
- Session JSONL 格式双向兼容（Rust 写的 TS 能读，反之亦然）
- Settings 合并语义不变
- Permission 三模式行为不变
- 所有 built-in tools 行为不变
- Extension hooks 行为不变（通过 plugin boundary）
- Compaction 触发和 rebuild 行为不变
- Resource loading 不变
- Model resolution 不变（已完成）
- TUI 可正常显示 session 状态
- focused parity tests 通过
- `cargo test -p rozsa-app` 通过
- `npm run check` 通过（TS 兼容层）

## 风险和控制

### Session 格式不兼容

风险：Rust session manager 写的 JSONL 与 TS 版本不兼容，导致已有 session 无法恢复。

控制：
- 先用 TS 现有 session files 作为 golden fixtures
- Rust 读写必须 round-trip 通过 fixture
- 不改变 entry 字段名
- 新增字段必须 optional 且有默认值

### Permission 绕过

风险：Rust tool 执行跳过 permission check。

控制：
- PermissionGuard 是 tool 执行的唯一入口
- 没有绕过 PermissionGuard 的 tool.execute() 路径
- permission reject fixture 是强制验收项
- 硬编码黑名单不可配置

### Settings 合并语义偏差

风险：Rust settings 合并与 TS 不同，导致用户配置失效。

控制：
- 用 TS 现有 settings merge 行为作为 golden fixtures
- 所有 merge 场景（override, append, remove）都有 parity test
- 文件锁语义对齐

### Extension 断裂

风险：Rust AgentSession 不再调用 TS extension hooks。

控制：
- 过渡期保持 TS extension runtime
- 通过 plugin boundary（stdio/IPC）调用 TS extensions
- extension hook 行为 parity 测试覆盖所有 hooks
- extension 不是第一阶段迁移目标

### Tool 行为差异

风险：Rust tool 实现与 TS 实现行为不同（output format、error message、truncation）。

控制：
- 每个 tool 有独立 parity fixture
- fixture 覆盖 normal、error、edge case
- output format 精确到字符串模板

### Compaction 数据丢失

风险：compaction 后丢失关键 session context。

控制：
- compaction 只在用户确认或自动触发时执行
- compaction 前检查 entry tree 完整性
- compaction 后 rebuild 的 context 与预期一致
- compaction fixture 覆盖 branch summary 注入

### 迁移范围膨胀

风险：app 迁移扩大成 full-stack 重写。

控制：
- 明确不迁列表
- extension runtime 保持 TS
- subagent 不在范围
- LSP 不在范围
- 每个 PR 只涉及一个子系统
