# Tools 迁移计划

本文定义 built-in tools 从 TypeScript 迁移到 Rust 的详细计划。

Built-in tools 是 agent 与文件系统、shell 交互的能力层。每个 tool 实现 rozsa-core 的 `Tool` trait。

相关代码：
- TS: `packages/coding-agent/src/core/tools/` (16 files)
- TS: `packages/coding-agent/src/core/tools/bash.ts`
- TS: `packages/coding-agent/src/core/tools/read.ts`
- TS: `packages/coding-agent/src/core/tools/edit.ts`
- TS: `packages/coding-agent/src/core/tools/write.ts`
- TS: `packages/coding-agent/src/core/tools/grep.ts`
- TS: `packages/coding-agent/src/core/tools/find.ts`
- TS: `packages/coding-agent/src/core/tools/ls.ts`
- TS: `packages/coding-agent/src/core/tools/subagent.ts`
- Rust: `crates/rozsa-app/src/tools/mod.rs` (TODO)
- Rust: `crates/rozsa-core/src/tool.rs` (Tool trait)

相关文档：
- [主文档](./rozsa-app-migration.md)
- [Extensions 迁移](./extensions-migration.md)

## Tool 概览和复杂度评估

### Tool 清单

| Tool | TS Lines (est.) | 复杂度 | 迁移优先级 | 理由 |
| --- | --- | --- | --- | --- |
| read | ~200 | 低 | P0 | 纯文件读取，无副作用 |
| ls | ~100 | 低 | P0 | 目录列表，无副作用 |
| grep | ~250 | 低 | P0 | 正则搜索，无副作用 |
| find | ~200 | 低 | P0 | 文件查找，无副作用 |
| write | ~150 | 中 | P1 | 文件创建，有副作用 |
| edit | ~400 | 高 | P1 | 文件编辑（string replace + diff），有副作用 |
| bash | ~300 | 高 | P1 | shell 执行，安全敏感 |
| subagent | ~500 | 极高 | P2 (不迁) | 多进程编排，依赖 Agent 递归创建 |

### 复杂度维度

- **I/O 模式**: 只读 vs 有写入副作用
- **安全性**: 是否需要 permission check
- **并发**: 是否需要 streaming/update
- **依赖**: 是否依赖其他系统 (file mutation queue, path resolution, etc.)

## Tool 执行生命周期

### 完整执行流

TS 参考点: `agent-session.ts` tool 执行路径

```text
LLM 返回 tool_call
  -> rozsa-core agent_loop 调度
  -> before_tool_call hook (rozsa-core config)
    -> AgentSession.beforeToolCall callback
      -> PermissionGuard.check()         # permission 检查
      -> Extension tool_call hook        # extension 可 block
  -> Tool.execute()                      # 真正执行
  -> after_tool_call hook (rozsa-core config)
    -> AgentSession.afterToolCall callback
      -> Extension tool_result hook      # extension 可修改结果
      -> LSP post-processing             # LSP 诊断注入
  -> ToolResult 返回给 agent_loop
```

### rozsa-core Tool trait

```rust
// 已在 crates/rozsa-core/src/tool.rs 中定义
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> ToolSchema;
    fn execution_mode(&self) -> Option<ToolExecutionMode> { None }

    async fn execute(
        &self,
        tool_call_id: &str,
        args: Value,
        signal: Option<CancellationToken>,
        on_update: Option<UpdateCallback>,
    ) -> Result<ToolResult, ToolError>;
}
```

### rozsa-app Tool Factory

每个 tool 通过 factory 创建，注入工作目录和配置：

```rust
pub fn create_read_tool(cwd: PathBuf, config: ReadToolConfig) -> Arc<dyn Tool>;
pub fn create_bash_tool(cwd: PathBuf, config: BashToolConfig) -> Arc<dyn Tool>;
// ...
```

## Permission 集成

### Per-tool Risk Level

| Tool | Default Risk Level | 需要 Permission |
| --- | --- | --- |
| read | read | No (whitelist) |
| ls | read | No (whitelist) |
| grep | read | No (whitelist) |
| find | read | No (whitelist) |
| write | write | Yes |
| edit | write | Yes |
| bash | shell/destructive | Yes (always) |
| subagent | network | Depends |

### Permission 在 Tool 执行中的位置

Permission 检查不在 Tool.execute() 内部，而是在 rozsa-core 的 `before_tool_call` hook 中：

```rust
// rozsa-app 提供给 rozsa-core 的 before_tool_call callback
fn before_tool_call(ctx: &BeforeToolCallContext) -> Option<BeforeToolCallResult> {
    // 1. permission check
    let decision = permission_guard.check(&PermissionRequest {
        tool_name: &ctx.tool_name,
        args: &ctx.args,
        risk_level: infer_risk_level(&ctx.tool_name, &ctx.args),
    });

    match decision {
        PermissionDecision::Deny { reason } => {
            Some(BeforeToolCallResult { block: true, reason: Some(reason) })
        }
        PermissionDecision::Approve => None, // proceed
    }
}
```

## LSP Post-processing

TS 参考点: `agent-session.ts` -> LSP hook in afterToolCall

对于 edit/write 类 tool，执行完成后：
1. 检查是否有 LSP server 连接
2. 如果有，等待 LSP diagnostics (timeout 500ms)
3. 将 diagnostics 附加到 tool result（作为 details）
4. 这让 LLM 知道编辑后是否有类型错误

Rust 目标: LSP 集成后迁（不在第一阶段）。第一阶段 after_tool_call 不注入 LSP diagnostics。

## Dry-run 模式

TS 不直接有 dry-run，但 tool 执行可以被 permission block。

Rust 可以在 tool 层面支持 dry-run：

```rust
pub struct ToolConfig {
    pub dry_run: bool, // if true, write tools report what they would do
}
```

这是迁移后优化点，不是必须功能。

## 各 Tool 详细规格

### read tool

TS 参考点: `tools/read.ts`

**功能**: 读取文件内容，支持行号范围、byte limit、truncation。

**参数 Schema**:
```json
{
  "file_path": "string (required) - absolute path",
  "offset": "number (optional) - start line (0-based)",
  "limit": "number (optional) - max lines to read",
  "pages": "string (optional) - PDF page range"
}
```

**行为**:
- 文件不存在 -> 返回 error content
- 文件过大 -> truncate + 提示 "use offset/limit"
- 二进制文件 -> 返回 error "binary file"
- 支持 PDF (via external tool)
- 支持 image (返回 base64 content block)
- 输出格式: 行号 + tab + 内容 (cat -n style)

**Rust 实现要点**:
- `tokio::fs::read_to_string` for text
- Line counting for offset/limit
- Output format: `"{line_number}\t{content}\n"`
- Size check before full read

### ls tool

TS 参考点: `tools/ls.ts`

**功能**: 列出目录内容。

**参数 Schema**:
```json
{
  "path": "string (required) - directory path"
}
```

**行为**:
- 路径不存在 -> error
- 不是目录 -> error
- 正常 -> 列出文件名，标记目录 (/)
- 支持简单 metadata (size, modified time)

**Rust 实现要点**:
- `tokio::fs::read_dir`
- Sorted output
- Directory indicator suffix

### grep tool

TS 参考点: `tools/grep.ts`

**功能**: 搜索文件内容，支持 regex。

**参数 Schema**:
```json
{
  "pattern": "string (required) - regex pattern",
  "path": "string (optional) - search path",
  "include": "string (optional) - file glob filter",
  "context_lines": "number (optional) - lines of context"
}
```

**行为**:
- 使用 ripgrep-style 搜索
- 支持 regex pattern
- 支持 glob include/exclude
- 结果格式: `{file}:{line}:{content}`
- 结果截断 (max results)

**Rust 实现要点**:
- `grep` crate 或直接用 `regex` + `walkdir`
- Performance advantage over TS
- Output format parity

### find tool

TS 参考点: `tools/find.ts`

**功能**: 递归查找文件。

**参数 Schema**:
```json
{
  "path": "string (required) - search root",
  "pattern": "string (required) - regex for file name",
  "type": "string (optional) - 'f' for files, 'd' for dirs"
}
```

**行为**:
- 递归遍历目录
- 按 regex 匹配文件名
- 可过滤 file/directory
- 跳过 .git, node_modules 等
- 结果截断 (max results)

**Rust 实现要点**:
- `walkdir` crate
- `.gitignore` 尊重 (via `ignore` crate)
- Sorted output

### write tool

TS 参考点: `tools/write.ts`

**功能**: 创建新文件（不覆盖已有文件）。

**参数 Schema**:
```json
{
  "file_path": "string (required) - absolute path",
  "content": "string (required) - file content"
}
```

**行为**:
- 文件已存在 -> error "file exists, use edit instead"
- 目录不存在 -> 自动创建
- 写入后验证
- 返回成功消息

**Rust 实现要点**:
- Check existence first (no overwrite)
- `tokio::fs::create_dir_all` for parent dirs
- `tokio::fs::write` for content
- File mutation queue integration (serialized writes)

### edit tool

TS 参考点: `tools/edit.ts`, `tools/edit-diff.ts`

**功能**: 修改已有文件内容。

**参数 Schema**:
```json
{
  "file_path": "string (required) - absolute path",
  "old_string": "string (required) - text to replace",
  "new_string": "string (required) - replacement text",
  "replace_all": "boolean (optional) - replace all occurrences"
}
```

**行为**:
- 文件不存在 -> error
- old_string 不存在 -> error "old_string not found"
- old_string 不唯一 (且 replace_all=false) -> error "not unique"
- 正常 -> 替换并写回
- 返回修改前后 diff

**Rust 实现要点**:
- Read file -> find old_string -> replace -> write back
- Uniqueness check
- Diff generation for output
- File mutation queue (serialized edits to same file)

### bash tool

TS 参考点: `tools/bash.ts`, `bash-executor.ts`

**功能**: 执行 shell 命令。

**参数 Schema**:
```json
{
  "command": "string (required) - shell command",
  "timeout": "number (optional) - timeout in ms"
}
```

**行为**:
- 启动 child process (/bin/bash -c "...")
- 捕获 stdout + stderr (combined)
- 支持 timeout (默认 120s)
- 支持 abort (kill process)
- 支持 streaming output (partial updates)
- 输出截断 (max bytes)
- 返回 exit code + output

**Rust 实现要点**:
- `tokio::process::Command`
- Combined stdout/stderr capture
- Timeout via `tokio::time::timeout`
- Kill on abort via `CancellationToken`
- Streaming via `on_update` callback
- Output truncation

**安全考虑**:
- bash tool 是安全性最敏感的 tool
- 所有 bash 命令必须经过 PermissionGuard
- 硬编码黑名单：rm -rf /, git reset --hard, dd, mkfs, sudo 等
- 深度检查: $(subcommand), 变量间接引用, 数据外泄

### subagent tool

不在第一阶段迁移范围。保持 TS 实现。

理由：
- 需要递归创建 Agent（复杂生命周期）
- 需要多进程调度
- 需要 session isolation
- 依赖整个 AgentSession 创建流程

## File Mutation Queue

TS 参考点: `tools/file-mutation-queue.ts`

多个 tool 可能并发修改同一文件（如 parallel batch 中多个 edit）。File mutation queue 保证同一文件的修改串行化。

```rust
pub struct FileMutationQueue {
    locks: HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>,
}

impl FileMutationQueue {
    pub async fn with_lock<F, T>(&self, path: &Path, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        let lock = self.locks.entry(path.to_path_buf())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        f()
    }
}
```

## Output 处理

### Truncation

TS 参考点: `tools/truncate.ts`

Tool 输出可能非常大（如 bash 输出、read 大文件）。truncation 策略：

```rust
pub struct TruncationConfig {
    pub max_lines: usize,      // default: 1000
    pub max_bytes: usize,      // default: 100_000
    pub strategy: TruncateStrategy,
}

pub enum TruncateStrategy {
    Head,     // keep first N lines
    Tail,     // keep last N lines
    Middle,   // keep head + tail, omit middle
}
```

### Output Accumulator

TS 参考点: `tools/output-accumulator.ts`

Bash tool 的 streaming output 通过 accumulator 缓冲：

```rust
pub struct OutputAccumulator {
    buffer: Vec<u8>,
    last_emit: Instant,
    emit_interval: Duration,
}

impl OutputAccumulator {
    pub fn push(&mut self, data: &[u8]) -> Option<String> {
        self.buffer.extend_from_slice(data);
        if self.last_emit.elapsed() >= self.emit_interval {
            self.last_emit = Instant::now();
            Some(String::from_utf8_lossy(&self.buffer).to_string())
        } else {
            None
        }
    }
}
```

## 迁移优先级和顺序

```text
Phase 1 (P0): Read-only tools — 零风险，可立即迁移验证
  TOOL-001: read
  TOOL-002: ls
  TOOL-003: grep
  TOOL-004: find

Phase 2 (P1): Write tools — 需要 permission 和 file mutation queue
  TOOL-005: write
  TOOL-006: edit
  TOOL-007: bash

Phase 3 (P2): 不迁
  subagent (保持 TS)
```

## 迁移任务

### TOOL-001: read tool

参考点: `tools/read.ts`

迁移动作:
- 实现 ReadTool struct
- 实现 Tool trait
- 文件读取 + 行号格式化
- offset/limit 支持
- size check + truncation
- binary file detection
- 错误消息格式与 TS 一致

优化点:
- memory-mapped file for large files
- 无需 Node.js buffer 分配

完整性测试:
- normal file read
- file not found -> error
- binary file -> error
- large file -> truncation
- offset/limit -> correct range
- output format matches TS (cat -n style)

### TOOL-002: ls tool

参考点: `tools/ls.ts`

迁移动作:
- 实现 LsTool struct
- 实现 Tool trait
- 目录列表 + 排序
- 目录标记 (/)
- 错误消息格式

优化点:
- 直接使用 readdir，无需额外 stat calls

完整性测试:
- normal directory listing
- non-existent path -> error
- file (not dir) -> error
- sorted output
- directory suffix (/)

### TOOL-003: grep tool

参考点: `tools/grep.ts`

迁移动作:
- 实现 GrepTool struct
- 实现 Tool trait
- regex pattern matching
- file glob filtering
- context lines
- result truncation
- output format: `{file}:{line}:{content}`

优化点:
- Rust regex 性能远优于 TS
- 使用 `ignore` crate 尊重 .gitignore

完整性测试:
- simple pattern match
- regex pattern match
- context lines
- glob include filter
- result truncation
- no match -> empty result (not error)
- invalid regex -> error

### TOOL-004: find tool

参考点: `tools/find.ts`

迁移动作:
- 实现 FindTool struct
- 实现 Tool trait
- recursive directory walk
- regex filename matching
- file/dir type filter
- skip patterns (.git, node_modules)
- result truncation

优化点:
- 使用 `ignore` crate (respects .gitignore)
- 使用 `walkdir` for efficient traversal

完整性测试:
- find files by pattern
- type filter (files only, dirs only)
- skip .git and node_modules
- result truncation
- non-existent root -> error

### TOOL-005: write tool

参考点: `tools/write.ts`

迁移动作:
- 实现 WriteTool struct
- 实现 Tool trait
- file existence check (no overwrite)
- parent directory creation
- content writing
- success message format

优化点:
- atomic write (tmp + rename) for safety
- file mutation queue integration

完整性测试:
- create new file
- file already exists -> error
- parent dir created automatically
- content matches exactly
- permission denied -> error (from PermissionGuard)

### TOOL-006: edit tool

参考点: `tools/edit.ts`

迁移动作:
- 实现 EditTool struct
- 实现 Tool trait
- file read -> find old_string -> replace -> write
- uniqueness check
- replace_all mode
- diff generation for output
- file mutation queue

优化点:
- line-level diff for readable output
- pre-check before write (no partial corruption)

完整性测试:
- successful replacement
- old_string not found -> error
- old_string not unique (replace_all=false) -> error
- replace_all=true -> all occurrences replaced
- output shows diff
- file not found -> error
- permission check integration

### TOOL-007: bash tool

参考点: `tools/bash.ts`, `bash-executor.ts`

迁移动作:
- 实现 BashTool struct
- 实现 Tool trait
- child process spawn (/bin/bash -c)
- stdout + stderr combined capture
- timeout support
- abort support (kill process)
- streaming output (on_update callback)
- output truncation
- exit code in result

优化点:
- signal handling (SIGTERM -> SIGKILL escalation)
- process group kill (avoid orphan processes)
- environment sanitization

完整性测试:
- simple command execution
- exit code != 0 -> is_error=true
- timeout -> kill + error
- abort -> kill + error
- streaming output partial updates
- large output -> truncation
- command not found -> error
- permission check integration (most critical)

### TOOL-008: File mutation queue

参考点: `tools/file-mutation-queue.ts`

迁移动作:
- 实现 FileMutationQueue
- per-file lock (tokio::sync::Mutex)
- integrate with write/edit tools
- timeout on lock acquisition

优化点:
- lock cleanup (remove lock after last user)
- deadlock detection

完整性测试:
- concurrent edits to same file -> serialized
- concurrent edits to different files -> parallel
- lock timeout -> error (not hang)

### TOOL-009: Truncation utilities

参考点: `tools/truncate.ts`, `tools/output-accumulator.ts`

迁移动作:
- 实现 TruncationConfig
- 实现 truncate_output (head/tail/middle strategies)
- 实现 OutputAccumulator (for bash streaming)
- 截断消息模板 ("... output truncated ...")

优化点:
- byte-aware truncation (not line-only)
- UTF-8 safe truncation (不在 char boundary 中间切)

完整性测试:
- head truncation at limit
- tail truncation at limit
- middle truncation preserves head+tail
- accumulator emit interval
- UTF-8 safe (no broken chars)
