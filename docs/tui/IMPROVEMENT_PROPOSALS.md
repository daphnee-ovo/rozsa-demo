# rozsa-tui-rs 改进提案

> 基于 codex-rs/tui 架构对比分析，按优先级排列。
> 参考源码：`codex-rs/tui/src/` @ commit e5afe5bf

---

## P0：稳定性与基础保障

### 1. Panic Hook + Terminal Restore Guard

**问题**：当前 panic 会留下 raw mode + alternate screen，用户终端不可用。

**codex 做法**（`codex-rs/tui/src/tui.rs:465`）：

```rust
fn set_panic_hook() {
    let hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_after_exit(); // 先恢复终端
        hook(panic_info);             // 再输出 panic 信息
    }));
}
```

加上 RAII guard（`lib.rs:1801`）：

```rust
struct TerminalRestoreGuard { active: bool }

impl Drop for TerminalRestoreGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = tui::restore_after_exit();
        }
    }
}
```

**pai 当前**（`app.rs:126`）：

```rust
pub async fn run() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, ...)?;
    // ... 如果中间 panic，终端永远留在 raw mode
    let result = run_app(...).await;
    disable_raw_mode()?; // panic 时到不了这里
}
```

**改进方案**：

```rust
pub async fn run() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    execute!(...)?;

    // RAII guard — 任何退出路径（包括 panic）都恢复终端
    let _guard = TerminalRestoreGuard::new();

    // panic hook — 确保 panic 信息在正常终端下输出
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
        prev(info);
    }));

    run_app(...).await
}
```

---

### 2. 异步写入 — 消除 `Arc<Mutex<UnixStream>>` 阻塞

**问题**：`socket.rs` 在 tokio async 上下文中使用同步 `std::os::unix::net::UnixStream` + `Mutex`，如果后端处理慢或 buffer 满，会阻塞整个事件循环。

**codex 做法**：全 async channel 架构，TUI 侧通过 `mpsc::UnboundedSender` 发消息，由独立 task 负责写入。

**改进方案**：

```rust
// 方案 A：tokio UnixStream + 写入 task
let (write_tx, mut write_rx) = mpsc::unbounded_channel::<ClientMessage>();

tokio::spawn(async move {
    let mut stream = tokio::net::UnixStream::connect(path).await?;
    while let Some(msg) = write_rx.recv().await {
        let bytes = serde_json::to_vec(&msg)?;
        stream.write_all(&bytes).await?;
        stream.write_all(b"\n").await?;
    }
});

// 方案 B（最小改动）：保留 std UnixStream 但用 spawn_blocking
fn send_async(writer: Writer, msg: ClientMessage<'static>) {
    tokio::task::spawn_blocking(move || {
        let guard = writer.lock().unwrap();
        // write...
    });
}
```

---

## P1：渲染性能

### 3. 按需重绘 — 去掉 50ms 固定 tick

**问题**：当前 `tick_interval = 50ms` 无条件触发 `terminal.draw()`，即使没有任何状态变化。CPU 空转、笔记本耗电。

**codex 做法**（`tui/src/tui/frame_requester.rs`）：

```
事件驱动：
  Key/Backend 事件 → set dirty flag → request_frame()
  FrameRequester 合并多次 request，MIN_FRAME_INTERVAL (16ms) 内最多一帧
  无事件时完全休眠
```

**改进方案**：

```rust
// 用 tokio::sync::Notify 替代固定 tick
let redraw_notify = Arc::new(tokio::sync::Notify::new());

// 事件处理后触发重绘
fn mark_dirty(notify: &Notify) { notify.notify_one(); }

// 主循环
loop {
    terminal.draw(|f| render(f, &state, &editor))?;
    tokio::select! {
        key = term_events.next() => { handle_key(...); mark_dirty(&redraw_notify); }
        msg = event_rx.recv() => { apply_backend_event(...); mark_dirty(&redraw_notify); }
        _ = redraw_notify.notified() => {} // 合并多次 notify
        // 动画场景（retry countdown、spinner）可加 conditional interval
    }
}
```

**附加**：streaming 期间可启用 16ms interval 做动画；idle 时完全休眠。

---

### 4. 消息渲染缓存优化

**问题**（`ui.rs:30-65`）：

```rust
// 每次都 serde_json::to_string 再 hash — 对大消息开销大
let s = v.to_string(); // ← 每次 O(n) 序列化
s.hash(&mut hasher);

// 溢出时全清 — 丢失所有热缓存
if cache.len() > 500 { cache.clear(); }
```

**改进方案**：

```rust
// 1. 用消息索引 (role + index) 做 key，避免重复序列化
type CacheKey = (usize, u64); // (message_index, content_hash_at_commit)

// 2. 增量 hash：只对 content 部分 hash，而非整个 JSON
fn message_cache_key(idx: usize, msg: &Value) -> CacheKey {
    let content = msg.get("content").map(|v| v.to_string()).unwrap_or_default();
    (idx, hash_string(&content))
}

// 3. LRU 淘汰替代全清
use std::collections::VecDeque;
struct LruCache<K, V> {
    map: HashMap<K, V>,
    order: VecDeque<K>,
    capacity: usize,
}
```

---

## P2：Markdown 渲染质量

### 5. 引入 pulldown-cmark 替代手写解析器

**问题**（`markdown.rs:24`）：逐行状态机无法正确处理：

```markdown
<!-- pai 当前会错误解析的场景 -->

- Item with `code | pipe` inside    ← 误识别为表格
- Nested list:
  - Sub-item with **bold _and italic_**  ← 嵌套格式丢失
  > Blockquote inside list            ← 无法处理

[link with `code`](url)             ← inline code 内的格式丢失
```

**codex 做法**（`markdown_render.rs`）：

```rust
use pulldown_cmark::{Parser, Options, Event, Tag};

let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | ...;
let parser = Parser::new_ext(source, opts);

for event in parser {
    match event {
        Event::Start(Tag::Heading { level, .. }) => { /* push style */ }
        Event::Text(text) => { /* append styled span */ }
        Event::Code(code) => { /* inline code style */ }
        Event::Start(Tag::Table(alignments)) => { /* start table state */ }
        // ... 完整 AST 事件覆盖
    }
}
```

**迁移策略**：

```toml
# Cargo.toml 添加
pulldown-cmark = "0.12"
```

```rust
// markdown.rs — 保留 highlight_code 现有实现，替换解析层
pub fn parse_markdown(text: &str, width: usize) -> Vec<Line<'static>> {
    let opts = Options::all();
    let parser = Parser::new_ext(text, opts);
    let mut renderer = MarkdownRenderer::new(width);
    for event in parser {
        renderer.process(event);
    }
    renderer.finish()
}
```

保留你现有的 `highlight_code()` + `render_table()` 作为渲染子组件，只替换**解析入口**。

---

### 6. 流式输出分块渲染

**问题**：当前每次 State 推送都是全量 messages 替换，streaming 时高频全量刷新：

```
Backend: state{messages:[...50条完整消息...]}  ← 50ms 发一次完整列表
TUI: 全部重新格式化 + 重新渲染
```

**codex 做法**（`streaming/`）：

- `chunking.rs` — 流式文本按 chunk 追加，不重建历史
- `commit_tick.rs` — 动画 tick 驱动逐字/逐词显示
- `table_holdback.rs` — 检测到表格 `|` 开头时暂缓渲染，等表格完整后一次性布局

**改进方案（分两步）**：

```
第一步：协议侧增加增量消息
  HostMessage::AppendChunk { message_idx: usize, delta: String }
  → TUI 只 append 到最后一条消息的 content，只重渲染该消息

第二步：渲染侧增加 active cell 概念
  - 已完成消息 → 缓存渲染结果（不变）
  - 最后一条 streaming 消息 → 每帧从 buffer 取 chunk 追加渲染
```

---

## P3：功能补全

### 7. Inline Viewport 模式（非 Alternate Screen）

**问题**：`EnterAlternateScreen` 导致退出后看不到对话历史。

**codex 做法**（`lib.rs:1840`）：

```rust
fn determine_alt_screen_mode(no_alt_screen: bool, config: AltScreenMode) -> bool {
    if no_alt_screen { return false; }
    config != AltScreenMode::Never
}
```

配合 ratatui 的 inline viewport（`CustomTerminal::with_options_and_cursor_position`），已完成的消息写入终端 scrollback。

**改进方案**：

```rust
// 1. 添加 CLI flag
#[arg(long)]
no_alt_screen: bool,

// 2. 使用 ratatui Viewport::Inline
let terminal = if no_alt_screen {
    Terminal::with_options(backend, TerminalOptions {
        viewport: Viewport::Inline(height),
    })?
} else {
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(backend)?
};

// 3. 已完成消息通过 insert_before() 写入 scrollback
// ratatui 0.29 支持 terminal.insert_before(lines_count, |buf| { ... })
```

---

### 8. Job Control (Ctrl+Z Suspend/Resume)

**问题**：用户按 Ctrl+Z 时进程 suspend，但终端状态未恢复；resume 后 raw mode 可能丢失。

**codex 做法**（`tui/job_control.rs`）：

```rust
// 注册 SIGTSTP handler
signal(SignalKind::from_raw(libc::SIGTSTP))

// 收到 SIGTSTP 前：
restore_keep_raw(); // 恢复终端显示，保留 raw mode
kill(getpid(), SIGSTOP); // 真正 suspend

// 收到 SIGCONT 后：
set_modes();  // 重新启用 raw + keyboard enhancement
terminal.clear()?; // 重绘
```

**改进方案**：

```rust
#[cfg(unix)]
fn setup_job_control(redraw_notify: Arc<Notify>) {
    tokio::spawn(async move {
        let mut sigtstp = tokio::signal::unix::signal(SignalKind::from_raw(libc::SIGTSTP)).unwrap();
        loop {
            sigtstp.recv().await;
            let _ = disable_raw_mode();
            let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
            unsafe { libc::kill(libc::getpid(), libc::SIGSTOP); }
            // --- resume 后执行 ---
            let _ = enable_raw_mode();
            let _ = execute!(std::io::stdout(), EnterAlternateScreen);
            redraw_notify.notify_one(); // 触发全量重绘
        }
    });
}
```

---

### 9. 粘贴性能优化

**问题**（`app.rs:413-429`）：逐字符插入，对大段粘贴 O(n × 行长)。

```rust
// 当前实现 — 1000 字符粘贴 = 1000 次 Vec 操作
for ch in data.chars() {
    if ch == '\n' { ... }
    else {
        let mut chars: Vec<char> = line.chars().collect(); // ← 每次重建
        chars.insert(idx, ch);                              // ← O(n) 移动
        *line = chars.into_iter().collect();                // ← 再重建 String
    }
}
```

**改进方案**：

```rust
fn handle_paste(data: &str, editor: &mut InputState) {
    editor.push_undo(); // 原子 undo

    let paste_lines: Vec<&str> = data.split('\n').collect();
    if paste_lines.len() == 1 {
        // 单行：直接 String::insert_str
        editor.lines[editor.cursor_row].insert_str(
            byte_offset_for_col(&editor.lines[editor.cursor_row], editor.cursor_col),
            paste_lines[0],
        );
        editor.cursor_col += paste_lines[0].chars().count();
    } else {
        // 多行：分割当前行 + 批量插入中间行 + 拼接尾行
        let current = &editor.lines[editor.cursor_row];
        let (head, tail) = current.split_at(
            byte_offset_for_col(current, editor.cursor_col)
        );
        let first_line = format!("{}{}", head, paste_lines[0]);
        let last_line = format!("{}{}", paste_lines.last().unwrap(), tail);

        editor.lines[editor.cursor_row] = first_line;
        let middle: Vec<String> = paste_lines[1..paste_lines.len()-1]
            .iter().map(|s| s.to_string()).collect();
        let insert_pos = editor.cursor_row + 1;
        editor.lines.splice(insert_pos..insert_pos, middle);
        editor.lines.insert(insert_pos + paste_lines.len() - 2, last_line);

        editor.cursor_row += paste_lines.len() - 1;
        editor.cursor_col = paste_lines.last().unwrap().chars().count();
    }
}
```

---

## P4：协议演进

### 10. 从全量 State 推送改为增量事件

**当前协议**：

```json
{"type":"state","state":{"messages":[...全部消息...],"isStreaming":true,...}}
```

每次 state 变化（哪怕只是 streaming 追加一个 token）都重传所有消息。

**目标协议**（参考 codex `ServerNotification`）：

```json
// 全量初始化（仅连接时一次）
{"type":"state","state":{...}}

// 增量更新
{"type":"patch","ops":[
  {"op":"replace","path":"/isStreaming","value":true},
  {"op":"append","path":"/messages/-/content/-","value":{"type":"text","text":" world"}}
]}

// 或者更简单的领域事件
{"type":"stream_delta","message_idx":5,"delta":"new tokens here"}
{"type":"tool_start","message_idx":6,"tool":"bash","command":"ls"}
{"type":"tool_done","message_idx":6,"exit_code":0,"output":"..."}
```

**TUI 侧配合修改**：

```rust
enum BackendEvent {
    FullState(NativeUiState),         // 初始化 / reconnect
    StreamDelta { idx: usize, text: String },
    MessageCommitted { idx: usize },  // streaming 结束，可缓存
    FieldUpdate { field: &str, value: Value },
    // ...
}
```

---

## 实施路线图

```
Phase 1 (稳定性) — 1~2 天
  ├── #1 Panic hook + restore guard
  ├── #2 异步写入
  └── #9 粘贴优化

Phase 2 (性能) — 2~3 天
  ├── #3 按需重绘
  ├── #4 缓存优化
  └── #6 流式分块（协议侧 stream_delta）

Phase 3 (渲染质量) — 3~5 天
  └── #5 pulldown-cmark 迁移

Phase 4 (功能) — 按需
  ├── #7 Inline viewport
  ├── #8 Job control
  └── #10 增量协议完整实现
```

---

## 附录：codex TUI 值得学习但 **不建议现阶段引入** 的能力

| 能力 | 原因 |
|------|------|
| 宠物动画 (pets/) | 趣味功能，非核心 |
| 多 app-server 模式 (Embedded/Daemon/Remote) | pai 当前只需 socket 模式 |
| 完整 vim mode | 编辑器复杂度高，可后续作为插件 |
| Onboarding 流程 | pai 有自己的 onboarding，无需照搬 |
| Session resume picker | 已有 session_selector，够用 |
