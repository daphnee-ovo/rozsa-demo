# tui-rs vs tui (TypeScript) — 差距分析

对比基准：`packages/tui/src/` 与 `packages/tui-rs/src/`，按影响程度排列。

---

## 严重（功能缺失或存在 bug）

### 1. Kitty 图片内存泄漏

**位置**：`terminal_image.rs:167`

`kitty_delete()` 函数已实现，但**从未被调用**。每次渲染新图片时 `IMAGE_ID_COUNTER` 累加，旧图从不清理。

TS 做法（`tui.ts`）：

```ts
// 每次重绘前跟踪上一帧的 Kitty image ID 集合
private previousKittyImageIds = new Set<number>();

// 差量计算后，对消失的 ID 发删除指令
buffer += this.deleteKittyImages(this.previousKittyImageIds);
```

**修复方向**：在 `ui.rs` 的渲染入口处维护 `previous_image_ids: HashSet<u32>`，每帧结束后对不再出现的 ID 调用 `kitty_delete()`，通过 `frame.buffer_mut()` 或在 `run_app` 中直接写 stdout。

---

### 2. 大段粘贴 UX（Paste Marker 缺失）

**位置**：`app.rs:handle_paste()`

TS 编辑器对超过阈值行数的粘贴内容，将其存储在独立 map 中，在输入框插入原子折叠标记 `[paste #1 +123 lines]`：

- 光标移动、词语删除、word-wrap 把标记当单个字符跳过
- submit 时自动展开为原始内容
- 用户可通过 backspace 整体删除一段大块粘贴

Rust 当前实现直接把所有行插入 `lines` vec，大段粘贴（如整个文件）会立即撑开输入框，无法折叠，删除也是逐行操作。

**修复方向**：

```rust
// InputState 添加
pub pastes: HashMap<u32, Vec<String>>,  // paste_id → 原始行
pub paste_id_counter: u32,

// handle_paste 中：超过 N 行时插入标记而非展开
const FOLD_THRESHOLD: usize = 5;
if paste_lines.len() > FOLD_THRESHOLD {
    let id = state.editor.paste_id_counter;
    state.editor.paste_id_counter += 1;
    let marker = format!("[paste #{} +{} lines]", id, paste_lines.len());
    state.editor.pastes.insert(id, paste_lines);
    // 在光标处插入 marker（单行）
} else {
    // 直接插入
}

// send 时展开所有 marker
fn expand_paste_markers(lines: &[String], pastes: &HashMap<u32, Vec<String>>) -> Vec<String>
```

---

### 3. Cell Size 硬编码

**位置**：`terminal_image.rs:128-129`

```rust
// 假设 cell 宽 8px 高 16px（标准终端字体比例）
let cell_width = 8u32;
let cell_height = 16u32;
```

TS 在启动时发 `CSI 16t` 查询终端实际像素 cell 尺寸，监听响应后动态更新：

```ts
// tui.ts
private queryCellSize(): void {
    this.terminal.write("\x1b[16t");
}

private consumeCellSizeResponse(data: string): boolean {
    const match = data.match(/^\x1b\[6;(\d+);(\d+)t$/);
    if (!match) return false;
    setCellDimensions({ heightPx: parseInt(match[1]), widthPx: parseInt(match[2]) });
    this.invalidate();
    this.requestRender();
    return true;
}
```

Rust 在 HiDPI 屏幕或非标准字体（如 16px 宽的字体）下图片宽高比计算偏差可导致图片拉伸或裁切。

**修复方向**：

1. 启动时向 stdout 写 `\x1b[16t`
2. 在 `term_events` 处理分支中检测 `Event::Key` 之外的原始转义序列（crossterm 目前不直接暴露 CSI 响应，需要通过 `Event::Unknown` 或在 raw 模式下提前读取）
3. 用 `std::sync::OnceLock<CellDimensions>` 存储结果，`terminal_image.rs` 读取

---

## 中等（特定场景可见的行为差异）

### 4. IME 光标定位精度

**位置**：`ui.rs:470-474`

```rust
// 当前：固定在输入框左上角
let cursor_x = area.x + 1 + ...;
let cursor_y = area.y + 1 + visible_cursor_row as u16;
frame.set_cursor_position((cursor_x, cursor_y));
```

TS 使用 `CURSOR_MARKER`（APC 转义序列 `\x1b_pi:c\x07`）机制：任意组件（包括 overlay 内部的编辑器）可在渲染输出中嵌入该标记，TUI 在帧末扫描所有行提取坐标，再用 `CSI G` 精确定位硬件光标：

```ts
export const CURSOR_MARKER = "\x1b_pi:c\x07";

// 在 doRender 末尾：
const cursorPos = this.extractCursorPosition(newLines, height);
this.positionHardwareCursor(cursorPos, newLines.length);
```

Rust 当前方案在有 overlay 遮盖输入框时，`set_cursor_position` 的坐标仍指向输入框区域，IME 候选框会出现在错误位置（被 overlay 遮住）。

**修复方向**：在 `render_input` 中向当前行字符串注入类似标记，在 `run_app` 帧末扫描 buffer 提取并用 `execute!(stdout, MoveTo(col, row))` 重定位光标。

---

### 5. Termux 高度变化处理

**位置**：`app.rs`，crossterm resize 事件处理路径

TS 对 Termux 环境（`TERMUX_VERSION` 环境变量存在）做特殊处理：高度变化时**不触发**全屏重绘。原因是 Android 软键盘弹出/收起会连续触发 `SIGWINCH`，全绘会导致整个历史消息在终端 scrollback 中重放。

```ts
if (heightChanged && !isTermuxSession()) {
    fullRender(true);
    return;
}
```

Rust 依赖 ratatui 的 resize 处理，在 Termux 上键盘切换时可能出现闪烁。

**修复方向**：在 `run_app` 中检测 `TERMUX_VERSION`，收到 `Event::Resize` 后仅在非 Termux 或宽度变化时强制全量重绘。

---

### 6. Overlay `visible()` 回调缺失

**位置**：`overlay.rs`，`ui.rs` 中 overlay 渲染逻辑

TS overlay 系统支持条件可见性回调：

```ts
showOverlay(component, {
    visible: (termWidth, termHeight) => termWidth >= 80 && termHeight >= 24,
});
```

终端缩小到阈值以下时 overlay 自动隐藏，无需调用方主动管理生命周期。Rust overlay 无此机制，终端极小时 overlay 可能遮满全屏。

**修复方向**：在 `OverlayState`（或对应结构）中添加 `visible: Option<Box<dyn Fn(u16, u16) -> bool>>`，`render` 入口处按终端尺寸过滤。

---

### 7. Sidebar 宽度阈值硬编码

**位置**：`ui.rs:83`

```rust
let shell = if frame.area().width >= 108 {
    // 显示 sidebar
} else {
    // 隐藏
};
```

108 列偏保守。TS 的 `dockRightMinMainWidth` 默认 80，可由调用方配置，使 sidebar 在中等宽度终端（80~107 列）也能出现。

**修复方向**：将阈值提取为常量或从 `AppState` / 环境变量读取，默认值改为 `SIDEBAR_MIN_MAIN_WIDTH = 80`，sidebar 宽度 24 列，gap 2 列，合计触发阈值 `>= 106`。

---

## 低（调试/工程质量）

### 8. Debug 与 Crash 日志机制缺失

TS 提供三个诊断入口：

| 环境变量 | 行为 |
|----------|------|
| `ROZSA_DEBUG_REDRAW=1` | 每次全量重绘写日志到 `~/.rozsa/agent/rozsa-debug.log` |
| `PI_TUI_DEBUG=1` | 每帧将 newLines/previousLines/buffer 写到 `/tmp/tui/` |
| 渲染行宽溢出 | 写 `rozsa-crash.log`（含所有行宽诊断），清理终端后抛出有意义的 Error |

Rust 目前渲染 bug 只能靠 eprintln 或 panic，无法事后追踪。

**修复方向**：在 `run_app` 顶部检测上述环境变量，对应路径写日志文件；在 `render()` 出口加行宽断言（debug build）。

---

### 9. SettingsList 组件缺失

TS 的 `SettingsList`（`components/settings-list.ts`）提供：

- fuzzy 搜索过滤
- 当前值展示 + Enter 循环切换
- submenu 回调（打开嵌套组件）
- 键盘导航 + Escape 取消

Rust 目前无对应组件。如果后续要在 TUI 内做设置界面（主题切换、模型参数等），需要从头实现。

**修复方向**：参考 `model_selector.rs` 结构，实现 `settings_selector.rs`，支持 `Vec<SettingItem>` + fuzzy filter。

---

## 优先级建议

| 优先级 | 项目 | 理由 |
|--------|------|------|
| P0 | #1 Kitty 泄漏 | 函数已有，只缺调用，修复成本极低 |
| P0 | #3 Cell size 查询 | 图片质量直接可见，影响所有 Kitty/iTerm2 用户 |
| P1 | #2 Paste marker | 大段粘贴是高频操作，UX 差距明显 |
| P1 | #4 IME 光标 | 影响所有中文/日文/韩文用户 |
| P2 | #5 Termux | 影响面窄但 Termux 用户无 workaround |
| P2 | #7 Sidebar 阈值 | 一行改动 |
| P3 | #6 Overlay visible | 防御性，非常规场景 |
| P3 | #8 Debug 日志 | 工程质量，不影响用户 |
| P3 | #9 SettingsList | 按需，目前无调用方 |
