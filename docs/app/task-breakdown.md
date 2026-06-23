# rozsa-app 任务拆分

本文定义 rozsa-app 迁移的完整任务清单，按依赖顺序组织，包含 gate checkpoint。

每个任务包含四件事：
- **参考点**: 迁移前以哪个现有实现为准
- **迁移动作**: 本任务实际改变什么
- **优化点**: 迁移后允许改进什么
- **完整性测试**: 如何证明没有破坏迁移前行为

相关文档：
- [主文档](./rozsa-app-migration.md)
- [Session 迁移](./session-migration.md)
- [Settings 迁移](./settings-migration.md)
- [Extensions 迁移](./extensions-migration.md)
- [Tools 迁移](./tools-migration.md)

## 依赖图

```text
Phase 1: 数据层 (无运行时依赖)
  APP-001 -> APP-002 -> APP-003  (Session persistence)
  APP-004 -> APP-005 -> APP-006  (Settings)
  APP-007                         (Product messages)

Phase 2: 执行层 (依赖 Phase 1)
  APP-008 -> APP-009 -> APP-010 -> APP-011  (Tools: read-only -> write -> bash)
  APP-012 -> APP-013                        (Permissions)
  APP-014                                    (Resources)

Phase 3: 编排层 (依赖 Phase 1 + 2)
  APP-015 -> APP-016 -> APP-017  (Extension infrastructure)
  APP-018                         (Compaction)
  APP-019                         (Skills)
  APP-020                         (AgentSession)

Phase 4: 集成 (依赖全部)
  APP-021  (Integration test suite)
  APP-022  (Backend switch)
  APP-023  (Local dogfood)
  APP-024  (Default Rust)
  APP-025  (Cleanup)
```

## 任务总表

| 任务 | 参考点 | 迁移动作 | 优化点 | 完整性测试 |
| --- | --- | --- | --- | --- |
| APP-001 | TS SessionEntry types | Rust session entry 类型 + serde | 编译时完备性 | JSONL round-trip |
| APP-002 | TS SessionManager read | Rust JSONL 读取 + tree build | HashMap index | fixture read parity |
| APP-003 | TS SessionManager write | Rust JSONL append + leaf update | atomic append | write + re-read |
| APP-004 | TS Settings interface | Rust settings schema types | type-safe get | fixture deserialize |
| APP-005 | TS SettingsManager merge | Rust merge 逻辑 | cached merge | merge fixture parity |
| APP-006 | TS FileSettingsStorage | Rust file I/O + locking | atomic write | concurrent access |
| APP-007 | TS custom messages | Rust product message types | opaque transport | serde round-trip |
| APP-008 | TS read tool | Rust ReadTool | mmap large files | output parity |
| APP-009 | TS ls/grep/find tools | Rust LsTool/GrepTool/FindTool | ignore crate | output parity |
| APP-010 | TS write/edit tools | Rust WriteTool/EditTool | atomic write | mutation parity |
| APP-011 | TS bash tool | Rust BashTool | process group kill | execution parity |
| APP-012 | TS permissions blacklist/whitelist | Rust PermissionGuard rules | compiled regex | rule match parity |
| APP-013 | TS permission decision flow | Rust full decision flow | trie matching | decision parity |
| APP-014 | TS ResourceLoader | Rust ResourceLoader | parallel file read | resource parity |
| APP-015 | TS extension hook types | Rust hook type definitions | enum completeness | type coverage |
| APP-016 | TS ExtensionRunner | Rust ExtensionRunner dispatch | async + timeout | dispatch parity |
| APP-017 | TS extension API | Plugin boundary protocol | batch optimization | round-trip latency |
| APP-018 | TS compaction logic | Rust CompactionEngine | incremental rebuild | compaction parity |
| APP-019 | TS skill matcher | Rust SkillMatcher | regex-based match | match parity |
| APP-020 | TS AgentSession | Rust AgentSession | tokio broadcast | lifecycle parity |
| APP-021 | 全部子系统 | 集成测试 suite | - | end-to-end parity |
| APP-022 | ROZSA_APP_BACKEND | backend 切换逻辑 | - | switch + fallback |
| APP-023 | 本地入口 | local dogfood | - | startup smoke |
| APP-024 | 默认 backend | 默认 Rust | - | regression gate |
| APP-025 | TS app runtime | 清理 TS 代码 | - | focused suite |

---

## Phase 1: 数据层

### APP-001: Session entry 类型定义

参考点: `packages/coding-agent/src/core/session-manager.ts` -> SessionEntry, SessionHeader, SessionMessageEntry, SessionCompactionEntry, SessionModelChangeEntry, SessionCustomEntry, SessionLabelEntry, SessionBranchSummary

迁移动作:
- 在 `crates/rozsa-app/src/session/` 新增 `entry.rs`
- 定义 `SessionEntry` enum (Message, Compaction, ModelChange, Custom, Label, BranchSummary)
- 定义 `SessionHeader` struct (version, sessionId, createdAt)
- 定义各 entry variant struct
- 实现 `#[derive(Serialize, Deserialize)]` with camelCase rename
- 确保字段名与 TS JSON 输出完全一致

优化点:
- Rust enum 保证 match 完备性
- 类型化的 id/parentId 而非 raw string

完整性测试:
- 从真实 TS session JSONL 文件读取每一行
- 反序列化为 SessionEntry
- 重新序列化
- round-trip 字段无损（忽略 JSON key order）
- 验证: `cargo test -p rozsa-app`

### APP-002: SessionManager 读取

参考点: `session-manager.ts` -> load(), getAllEntries(), buildEntryTree(), buildSessionContext()

迁移动作:
- 在 `crates/rozsa-app/src/session/` 新增 `manager.rs`
- 实现 JSONL 文件按行读取
- 实现 entry HashMap (id -> entry)
- 实现 children index (parentId -> Vec<id>)
- 实现 leaf detection (没有 children 的最新 entry)
- 实现 `buildSessionContext(leafId)`:
  - 从 leaf 沿 parentId chain 回溯
  - compaction entry: 注入 summary message, 跳过 removed entries
  - branch summary entry: 注入 summary message
  - message entry: 收入 context
  - reverse 得到时间顺序

优化点:
- 使用 BufReader 逐行读取避免整体加载
- 缓存 parentId chain

完整性测试:
- fixture: 包含所有 entry type 的 session file
- buildSessionContext 结果与 TS 版本一致 (message 数量、顺序、内容)
- leaf detection 正确
- branch point 识别正确
- 验证: `cargo test -p rozsa-app`

### APP-003: SessionManager 写入

参考点: `session-manager.ts` -> appendMessage(), appendCompaction(), appendCustom(), appendLabel()

迁移动作:
- 实现 append 系列方法
- 每次 append: 生成 entry id, 设置 parentId = 当前 leaf, 序列化写入文件, 更新 leaf
- 实现 file append mode (不 read-write-truncate)
- 实现 id 生成 (UUID v4 或递增)
- 实现 leaf 更新

优化点:
- append-only file I/O (高性能)
- fd-lock 保证多进程安全
- fsync 保证持久化

完整性测试:
- append message 后 re-read, entry 存在
- parentId chain 正确 (新 entry parent = 旧 leaf)
- leaf 更新正确 (新 entry 成为 leaf)
- concurrent append 不 corrupt (fd-lock test)
- 验证: `cargo test -p rozsa-app`

### APP-004: Settings schema 类型

参考点: `settings-manager.ts` -> Settings interface, all sub-interfaces

迁移动作:
- 在 `crates/rozsa-app/src/settings/` 新增 `schema.rs`
- 定义 `Settings` struct (fully resolved, all fields required or defaulted)
- 定义 `PartialSettings` struct (all fields Option)
- 定义 `CompactionSettings`, `RetrySettings`, `TerminalSettings`, `PermissionSettings`
- 定义 `ProviderConfig`, `ProviderModelOverride`
- 定义 `SettingsLayer` enum (Global, Project, Local, Runtime)
- 实现 serde

优化点:
- derive macro for PartialSettings (减少 boilerplate)
- Default 实现作为 fallback

完整性测试:
- 用 TS 生成的 settings.json 反序列化
- 字段值与 TS SettingsManager.get() 一致
- 验证: `cargo test -p rozsa-app`

### APP-005: Settings merge 逻辑

参考点: `settings-manager.ts` -> merge logic (scattered in get methods)

迁移动作:
- 在 `crates/rozsa-app/src/settings/` 新增 `merge.rs`
- 实现 `merge(layers: &[PartialSettings]) -> Settings`
- 标量: 最后一个 Some wins
- 数组: 最后一个 Some wins (全替换)
- 对象: 递归 merge (field-level)
- null: 显式清除
- 实现 `SettingsManager` struct (持有 4 layers + cached merged)

优化点:
- cached merged, invalidate on set()
- change events

完整性测试:
- fixture: 4 层 partial settings
- merge 后每个字段与 TS 一致
- edge cases: null override, empty array, nested merge, missing layer
- 验证: `cargo test -p rozsa-app`

### APP-006: Settings file I/O

参考点: `settings-manager.ts` -> FileSettingsStorage, file locking

迁移动作:
- 在 `crates/rozsa-app/src/settings/` 新增 `storage.rs`
- 实现 async read (tokio::fs)
- 实现 atomic write (tmp + rename)
- 实现 file locking (fd-lock 或 advisory lock)
- 实现 settings migration (deprecated field rename)
- 实现 path resolution (~/.rozsa-agent/settings.json, .claude/settings.json)

优化点:
- atomic write 避免 corruption on crash
- lock timeout 防 deadlock
- file watcher (detect external changes)

完整性测试:
- read existing settings file
- write + re-read round-trip
- concurrent write 不 corrupt
- lock timeout 触发 error
- migration 正确执行
- 验证: `cargo test -p rozsa-app`

### APP-007: Product-level messages

参考点: `agent-session.ts` -> custom message creation, compaction summary message, branch summary message

迁移动作:
- 在 `crates/rozsa-app/src/messages.rs` 实现
- 定义 product-level custom message constructors:
  - `create_compaction_summary_message(summary: &str) -> AgentMessage`
  - `create_branch_summary_message(summary: &str) -> AgentMessage`
  - `create_system_notification(text: &str) -> AgentMessage`
- 与 rozsa-core AgentMessage::Custom variant 集成

优化点:
- typed constructors 避免 raw JSON 构造

完整性测试:
- 构造后序列化与 TS 格式一致
- custom_type 字段正确
- display 字段正确
- 验证: `cargo test -p rozsa-app`

---

## Gate A: 数据层完整性

Phase 1 完成后的检查点。

通过标准:
- Session JSONL round-trip 无损
- Settings merge 与 TS 一致
- Session context build 与 TS 一致
- File I/O with locking 稳定
- Product messages 格式兼容
- `cargo test -p rozsa-app` 全通过

失败处理:
- 不进入 Phase 2
- 先修复数据层问题

---

## Phase 2: 执行层

### APP-008: read tool

参考点: `tools/read.ts`

迁移动作:
- 在 `crates/rozsa-app/src/tools/` 新增 `read.rs`
- 实现 `ReadTool` struct
- 实现 `Tool` trait (rozsa-core)
- 文件读取: tokio::fs::read_to_string
- 行号格式化: `"{line_number}\t{content}\n"` (cat -n style)
- offset/limit: 行范围读取
- size check: 超限 truncation
- binary detection: 非 UTF-8 报错
- error messages 与 TS 一致

优化点:
- BufReader 逐行读避免大文件整体加载
- mmap for very large files

完整性测试:
- normal file -> cat -n format output
- file not found -> error message matches TS
- binary file -> error
- large file -> truncation message
- offset=5, limit=10 -> lines 5-14
- 验证: `cargo test -p rozsa-app`

### APP-009: ls, grep, find tools

参考点: `tools/ls.ts`, `tools/grep.ts`, `tools/find.ts`

迁移动作:
- 新增 `ls.rs`, `grep.rs`, `find.rs`
- LsTool: readdir + sort + directory suffix
- GrepTool: regex match + context lines + file glob + truncation
- FindTool: walkdir + regex name match + type filter + skip patterns
- 输出格式与 TS 一致

优化点:
- grep: 使用 `ignore` crate 的 parallel walker
- find: 使用 `ignore` crate 尊重 .gitignore
- Rust regex 性能显著优于 TS

完整性测试:
- 每个 tool 的 normal/error/edge case 覆盖
- output format character-level match with TS
- 验证: `cargo test -p rozsa-app`

### APP-010: write, edit tools

参考点: `tools/write.ts`, `tools/edit.ts`

迁移动作:
- 新增 `write.rs`, `edit.rs`
- WriteTool: existence check + create_dir_all + write
- EditTool: read + find + replace + uniqueness check + write
- File mutation queue integration
- Diff output for edit

优化点:
- atomic write (tmp + rename)
- 编辑前 snapshot 用于 undo

完整性测试:
- write: new file, existing file error, parent dir creation
- edit: replacement, not found error, not unique error, replace_all
- edit output diff format
- file mutation queue serialization
- 验证: `cargo test -p rozsa-app`

### APP-011: bash tool

参考点: `tools/bash.ts`, `bash-executor.ts`

迁移动作:
- 新增 `bash.rs`
- BashTool: tokio::process::Command + /bin/bash -c
- stdout + stderr combined
- timeout: tokio::time::timeout
- abort: kill process on CancellationToken
- streaming: on_update callback with OutputAccumulator
- exit code in result
- output truncation
- 新增 `truncate.rs` (TruncationConfig, strategies)
- 新增 `output_accumulator.rs`

优化点:
- process group kill (避免 orphan)
- SIGTERM -> wait 5s -> SIGKILL escalation
- environment variable filtering

完整性测试:
- echo hello -> "hello\n", exit 0
- false -> "", exit 1, is_error=true
- sleep 999 with timeout 1s -> kill + error
- abort -> kill + error
- large output -> truncation
- streaming partial updates
- 验证: `cargo test -p rozsa-app`

### APP-012: Permission rules

参考点: `permissions.ts` -> hardcoded blacklist, default whitelist, risk level inference

迁移动作:
- 在 `crates/rozsa-app/src/permissions/` 新增 `blacklist.rs`, `whitelist.rs`, `risk.rs`
- 硬编码黑名单: rm -rf /, git reset --hard, git push --force, dd, mkfs, sudo 等
- 默认白名单: read, ls, grep, find, git status/log/diff
- Risk level inference:
  - tool type (read/write/shell/network)
  - command patterns (for bash)
  - path locations (system dirs = higher risk)
  - deep checks: $(subcommand), variable indirection

优化点:
- compiled regex (once_cell / LazyLock)
- trie-based command prefix matching

完整性测试:
- 所有 TS blacklist patterns -> Rust deny
- 所有 TS whitelist patterns -> Rust approve
- risk level inference 与 TS 一致
- edge cases: unicode commands, env var expansion
- 验证: `cargo test -p rozsa-app`

### APP-013: Permission decision flow

参考点: `permissions.ts` -> PermissionManager, decision flow, session approvals, audit

迁移动作:
- 新增 `guard.rs`, `decision.rs`
- PermissionGuard struct:
  - check(request) -> PermissionDecision
  - approve_session(trust_key) -> persist
  - audit_log(decision) -> append
- Decision flow:
  1. blacklist -> DENY
  2. read tools -> APPROVE
  3. free-permission -> APPROVE
  4. whitelist match -> APPROVE
  5. session approval match -> APPROVE
  6. auto-permission -> LLM reviewer (via callback)
  7. on-request -> user prompt (via callback)
- Session approval persistence (trust keys in project settings)
- Audit logging (.rozsa-agent/sessions/{id}.jsonl)

优化点:
- decision caching for identical requests
- async reviewer with timeout

完整性测试:
- 每个 decision branch 独立测试
- session approval persist + reload
- audit log format 与 TS 兼容
- 验证: `cargo test -p rozsa-app`

### APP-014: ResourceLoader

参考点: `resource-loader.ts`

迁移动作:
- 在 `crates/rozsa-app/src/resources/` 新增 `loader.rs`
- 实现 resource 发现:
  - CLAUDE.md (cwd, parent dirs, ~/.claude/)
  - AGENTS.md (cwd, parent dirs)
  - .claude/instructions.md (project)
- 实现 collision detection (多个同名 resource)
- 实现 content loading (async file read)
- 实现 system prompt assembly (header + resources + footer)

优化点:
- parallel file read (tokio::join!)
- resource caching with file watch invalidation

完整性测试:
- fixture: project with CLAUDE.md + AGENTS.md
- discovered resources 与 TS 一致
- collision detection 行为一致
- system prompt assembly 格式一致
- 验证: `cargo test -p rozsa-app`

---

## Gate B: 执行层完整性

Phase 2 完成后的检查点。

通过标准:
- 所有 read-only tools 输出与 TS parity
- write/edit tools 行为与 TS parity
- bash tool 执行、timeout、abort 与 TS parity
- Permission blacklist 100% match
- Permission whitelist 100% match
- Permission decision flow 全分支覆盖
- ResourceLoader 发现与 TS 一致
- `cargo test -p rozsa-app` 全通过

失败处理:
- 不进入 Phase 3
- 先修复执行层问题

---

## Phase 3: 编排层

### APP-015: Extension hook 类型

参考点: `extensions/types.ts` -> all event type definitions

迁移动作:
- 在 `crates/rozsa-app/src/extensions/` 新增 `hooks.rs`
- 定义 `ExtensionHook` enum (所有 hook 类型)
- 定义 `HookResult` enum (所有返回类型)
- 定义 `HookType` enum (用于注册)
- 实现 serde 序列化 (用于 plugin boundary)

优化点:
- typed association: HookType -> HookResult (编译时正确性)
- 使用 macro 减少 boilerplate

完整性测试:
- 所有 TS hook types 有对应 Rust variant
- 序列化格式与 TS JSON 兼容
- 验证: `cargo test -p rozsa-app`

### APP-016: ExtensionRunner dispatch

参考点: `extensions/runner.ts` -> emit, sequential dispatch, first-result-wins

迁移动作:
- 在 `crates/rozsa-app/src/extensions/` 新增 `runner.rs`
- 实现 handler 注册 (per hook type)
- 实现 sequential dispatch
- 实现 first-result-wins (非 None 结果立即返回)
- 实现 cancel 语义 (cancellable hooks)
- 实现 timeout (prevent hang)

优化点:
- async dispatch
- per-handler timeout
- metrics (hook execution time)

完整性测试:
- 多 handler 按注册顺序
- first result wins
- cancel stops operation
- timeout skips slow handler
- 验证: `cargo test -p rozsa-app`

### APP-017: Plugin boundary (TS extension bridge)

参考点: 新增（无直接 TS 等价）

迁移动作:
- 定义 IPC protocol (JSON over stdio or unix socket)
- TS Extension Host:
  - 独立 Node.js 进程
  - 加载现有 TS extensions
  - 接收 hook JSON, dispatch to TS ExtensionRunner
  - 返回 HookResult JSON
- Rust PluginBoundary:
  - spawn TS extension host
  - forward hooks (serialize -> send)
  - receive results (receive -> deserialize)
  - handle: crash, timeout, shutdown

优化点:
- 只 forward 有注册 handler 的 hooks (减少 IPC)
- message_update batch (高频 hook 降频)
- connection pooling

完整性测试:
- hook round-trip: Rust -> TS -> Rust
- latency < 10ms (p99) for non-streaming hooks
- TS crash -> Rust graceful degradation (不 panic)
- shutdown -> cleanup
- 验证: `cargo test -p rozsa-app` + TS integration test

### APP-018: CompactionEngine

参考点: `compaction/compaction.ts`

迁移动作:
- 在 `crates/rozsa-app/src/compaction/` 新增 `engine.rs`
- 实现 trigger logic:
  - threshold: context 使用率达到 threshold
  - overflow: context 超过 max window
- 实现 prepare:
  - 选择 cut point (保留最近 N entries)
  - 构建 summary prompt (for LLM)
  - 收集被移除的 entries
- 实现 execute:
  - 调用 model stream (LLM 生成 summary)
  - 创建 compaction entry (summary, firstKeptEntryId)
  - append to session
- 实现 rebuild:
  - 基于 compaction entry 重建 context
  - 注入 summary message
  - 保留 firstKeptEntryId 之后的 entries

优化点:
- incremental rebuild (不重遍历整个 tree)
- summary 质量评估 (retry if too short)

完整性测试:
- trigger at threshold
- trigger at overflow
- summary message 注入位置正确
- rebuild 后 context 正确 (= summary + kept entries)
- 不触发时不执行
- 验证: `cargo test -p rozsa-app`

### APP-019: SkillMatcher

参考点: `agent-session-runtime.ts` -> skill matching, system prompt injection

迁移动作:
- 在 `crates/rozsa-app/src/skills/` 新增 `matcher.rs`
- 实现 skill 定义加载 (from resources)
- 实现 skill matching (keyword/regex based)
- 实现 system prompt assembly:
  - base system prompt
  - active skills section
  - custom instructions (CLAUDE.md)
  - tool descriptions
  - mode-specific sections

优化点:
- skill 索引 for fast matching
- lazy system prompt rebuild (only when skills change)

完整性测试:
- skill 加载正确
- keyword match 触发 skill injection
- system prompt 包含 matched skills
- no match -> no injection
- 验证: `cargo test -p rozsa-app`

### APP-020: AgentSession

参考点: `agent-session.ts` -> AgentSession class (完整生命周期)

迁移动作:
- 在 `crates/rozsa-app/src/session/` 新增 `agent_session.rs`
- AgentSession struct 组合:
  - rozsa-core Agent (via agent_loop)
  - SessionManager
  - SettingsManager
  - PermissionGuard
  - ExtensionRunner
  - ResourceLoader
  - CompactionEngine
  - SkillMatcher
  - RuntimeState
  - tools: Vec<Arc<dyn Tool>>
- 实现 prompt():
  - extension input hook
  - compaction check
  - build messages
  - call agent_loop
  - consume event stream
  - post-run hooks (retry, compaction)
- 实现 continue_run():
  - call agent_loop_continue
  - consume event stream
- 实现 abort():
  - CancellationToken cancel
- 实现 compact():
  - manual compaction trigger
- 实现 event subscription (tokio::sync::broadcast)
- 实现 RuntimeState update (per event)

优化点:
- broadcast channel for event fanout (TUI + session persist + extensions)
- structured error types
- retry with exponential backoff

完整性测试:
- no-tool prompt: event sequence match TS
- single tool prompt: event sequence match TS
- abort: stop_reason = Aborted
- continue: from tool result
- compaction trigger: threshold reached
- model switch: persist + clamp
- 验证: `cargo test -p rozsa-app`

---

## Gate C: 编排层完整性

Phase 3 完成后的检查点。

通过标准:
- AgentSession prompt lifecycle 与 TS parity
- Extension hooks dispatch 正确
- Plugin boundary TS extensions 可调用
- Compaction trigger + rebuild 与 TS parity
- Skills injection 正确
- 所有子系统组合后 end-to-end 工作
- `cargo test -p rozsa-app` 全通过

失败处理:
- 不进入 Phase 4
- 先修复编排层问题

---

## Phase 4: 集成

### APP-021: Integration test suite

参考点: 所有子系统的组合行为

迁移动作:
- 新增 `tests/integration/app/` 目录
- 端到端 parity tests:
  - no-tool prompt (full event sequence)
  - single tool prompt (with permission approve)
  - permission deny scenario
  - extension tool_call hook block
  - extension tool_result hook modify
  - compaction trigger and rebuild
  - session persist and restore
  - model switch mid-session
  - abort during stream
  - abort during tool
  - settings change mid-session
- 使用 fake model stream (no real provider)
- 使用 fake tools where possible (for isolation)

优化点:
- fixture-based testing (golden event sequences)
- property-based testing for edge cases

完整性测试:
- 所有 scenarios 通过
- event sequences match TS golden fixtures
- 验证: `cargo test --test integration_app`

### APP-022: Backend 切换逻辑

参考点: rozsa-core ROZSA_CORE_BACKEND 模式

迁移动作:
- 实现 `ROZSA_APP_BACKEND=ts|rust` 环境变量读取
- ts mode: 继续使用 TS AgentSession (现有路径)
- rust mode: 使用 Rust AgentSession
- invalid value: fail fast with error message
- 缺省值: ts (Phase 4 初期)
- bridge protocol (如果需要 TS -> Rust 通信)

优化点:
- 子系统级 backend (更细粒度)
- backend 报告 (启动时 log 当前 backend)

完整性测试:
- ROZSA_APP_BACKEND=ts -> TS session works
- ROZSA_APP_BACKEND=rust -> Rust session works
- ROZSA_APP_BACKEND=invalid -> fail fast
- unset -> default (ts)
- 验证: startup smoke

### APP-023: Local dogfood

参考点: rozsa-core CORE-022

迁移动作:
- 主启动入口: ROZSA_APP_BACKEND=rust
- TS 启动入口: 保持 ts
- 文档: 写清切换方式和回滚方法
- 监控: error rate, crash rate, latency

优化点:
- gradual rollout (开发者先用)
- crash report 自动收集

完整性测试:
- 主入口正常启动
- prompt -> response 正常
- session persist 正常
- 回滚到 ts 正常
- 验证: 手动 dogfood + startup smoke

### APP-024: 默认 Rust

参考点: rozsa-core CORE-023

前置条件:
- APP-021 integration tests 全通过
- Local dogfood 稳定一个迭代
- 无 blocker issue
- 回滚路径已验证

迁移动作:
- 默认 ROZSA_APP_BACKEND=rust
- 保留 ROZSA_APP_BACKEND=ts 回滚
- release notes 写清变更

优化点:
- 性能基线对比 (Rust vs TS)
- 资源使用对比 (memory, CPU)

完整性测试:
- 不设环境变量 -> Rust session
- 设置 ts -> 回滚正常
- `cargo test -p rozsa-app` 通过
- `npm run check` 通过
- startup smoke 通过

### APP-025: 清理 TS app runtime

参考点: rozsa-core CORE-024

前置条件:
- Rust default 稳定至少一个迭代
- 无 blocker issue
- 用户确认可以删除

迁移动作:
- 删除或降级 TS AgentSession
- 删除 TS SessionManager (or keep as read-only migration helper)
- 删除 TS SettingsManager (or keep as migration helper)
- 删除 TS permission 纯规则代码 (Rust 替代)
- 保留: TS extensions runtime (plugin boundary keeps it)
- 更新文档

优化点:
- 减少 npm dependencies
- 减少 TS build time
- 减少 binary size

完整性测试:
- Rust session 是唯一 runtime
- TS extensions 仍通过 plugin boundary 工作
- `cargo test -p rozsa-app` 通过
- `npm run check` 通过 (reduced scope)

---

## Gate D: 最终完整性

Phase 4 完成后的最终检查点。

通过标准:
- Rust AgentSession 是默认且唯一 runtime
- 所有 integration tests 通过
- Session JSONL 双向兼容已验证
- Permission 行为 100% parity
- Tool 行为 100% parity
- Extension hooks 通过 plugin boundary 正常
- Compaction 正常
- Settings 正常
- 性能不低于 TS (latency, memory)
- 无已知 regression

失败处理:
- 回滚到 ROZSA_APP_BACKEND=ts
- 修复后重新验证

---

## 时间估算

| Phase | 任务数 | 估计工时 | 说明 |
| --- | --- | --- | --- |
| Phase 1 | 7 | 2-3 weeks | 类型定义 + I/O，相对机械 |
| Phase 2 | 7 | 3-4 weeks | Tools 多但相似，permission 复杂 |
| Phase 3 | 6 | 4-5 weeks | Extension bridge 最复杂 |
| Phase 4 | 5 | 2-3 weeks | 集成 + dogfood + 切换 |
| 总计 | 25 | 11-15 weeks | |

关键路径: APP-017 (plugin boundary) 是最大风险点，因为涉及跨进程通信和 TS extension 兼容性。建议在 Phase 3 初期就开始原型验证。

## 并行化机会

以下任务可以并行开发：

- Phase 1 内: APP-001/002/003 (session) 与 APP-004/005/006 (settings) 并行
- Phase 2 内: APP-008/009 (read-only tools) 与 APP-012 (permission rules) 并行
- Phase 3 内: APP-015/016 (extension infra) 与 APP-018 (compaction) 并行
- APP-014 (resources) 与 APP-019 (skills) 有顺序依赖但可部分并行

建议分配:
- 开发者 A: Session + Compaction
- 开发者 B: Settings + Permissions
- 开发者 C: Tools
- 开发者 D: Extensions (Plugin boundary)
