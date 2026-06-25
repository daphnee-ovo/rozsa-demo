# Rust TUI vs TypeScript TUI 功能对照表

> 生成时间：2026-05-30
> 对比基准：packages/tui-rs/src (Rust) vs packages/tui/src + packages/coding-agent/src (TS)
>
> **2026-05-31 更新：** 已完成 18 项 bug 修复和功能对齐

## 总览统计

| 指标 | 数量（更新前 → 更新后） |
|------|------|
| 对比功能总数 | 136 |
| ✅ 完整实现 | 38 → ~50 |
| ⚠️ 部分实现 | 35 → ~28 |
| ❌ 缺失 | 57 → ~52 |
| 🚀 Rust 更优 | 6 |

---

## 1. Slash Commands（35 项）

**关键发现**：Rust 的 `command/` 模块实际上是死代码——`dispatch_command()` 从未被调用。所有 slash command 都通过 `input.rs:816` 提交到 TS 后端处理。Rust 本地仅注册了 6 个命令用于自动补全展示。

| 功能 | 状态 | Rust 源码 | TS 源码 | 差距描述 |
|------|------|-----------|---------|----------|
| /help | ⚠️ | tui-rs/src/command/builtin.rs:12-29 | coding-agent/src/modes/native/native-builtins.ts:130-139 | Rust 硬编码 7 项帮助文本；TS 动态列出所有命令并支持 `/help <topic>` 详细帮助 |
| /hotkeys | ⚠️ | tui-rs/src/command/builtin.rs:32-53 | coding-agent/src/modes/native/native-builtins.ts:142-147 | Rust 硬编码；TS 从 NativeKeybindings 动态读取 |
| /clear | ✅ | tui-rs/src/command/builtin.rs:56-62 | — (TS 用 /new 代替) | Rust 仅清除本地 UI，不重置 session |
| /model | ⚠️ | tui-rs/src/command/builtin.rs:67-69 | native-builtins.ts:188-198 | Rust 仅发 ListModels；TS 支持 `/model <name>` 直接选择 |
| /compact | ⚠️ | tui-rs/src/command/builtin.rs:73-77 | native-builtins.ts | Rust 不传自定义指令参数（但 submit 路径可工作） |
| /session | ⚠️ | tui-rs/src/command/builtin.rs:80-83 | native-builtins.ts:165-177 | Rust 打开列表；TS 显示详细统计信息 |
| /settings | ❌ | — | native-builtins.ts:211-228 | 本地未注册，依赖后端处理 |
| /scoped-models | ❌ | — | native-builtins.ts | 同上 |
| /export | ❌ | — | native-builtins.ts | 支持 html/md/jsonl 导出 |
| /import | ❌ | — | native-builtins.ts | 从 JSONL 导入 session |
| /share | ❌ | — | native-builtins.ts | 分享为 GitHub Gist |
| /copy | ❌ | — | native-builtins.ts | 复制最后回复到剪贴板 |
| /name | ❌ | — | native-builtins.ts | 设置 session 名称 |
| /subagents | ❌ | — | native-builtins.ts | 列出/切换 subagent 视图 |
| /main | ❌ | — | native-builtins.ts | 切回主 agent 视图 |
| /changelog | ❌ | — | native-builtins.ts | 显示更新日志 |
| /fork | ❌ | — | native-builtins.ts | 从历史消息分叉 |
| /clone | ❌ | — | native-builtins.ts | 复制当前 session |
| /tree | ❌ | — | native-builtins.ts | 导航 session 分支树 |
| /graph | ❌ | — | native-mode.ts:430 | 可视化 session timeline |
| /login | ❌ | — | core/slash-commands.ts | native TUI 显示 "not supported" |
| /logout | ❌ | — | core/slash-commands.ts | 同上 |
| /new | ❌ | — | native-builtins.ts | 创建新 session |
| /permissions | ❌ | — | native-builtins.ts | 显示权限决策历史 |
| /resume | ❌ | — | native-builtins.ts | 恢复其他 session |
| /reload | ❌ | — | native-builtins.ts | 重新加载配置 |
| /search | ❌ | — | native-builtins.ts | 搜索消息内容 |
| /quit | ❌ | — | native-builtins.ts | 退出程序 |
| /gc | ❌ | — | native-builtins.ts | 清理无用 session |
| /lsp | ❌ | — | native-builtins.ts | LSP 诊断展示 |
| 命令注册机制 | ⚠️ | tui-rs/src/command/mod.rs:35 | — | 每次调用重建 registry（但实际未被调用） |
| Slash 自动补全 | ✅ | tui-rs/src/autocomplete.rs | tui/src/autocomplete.ts | 正常工作 |
| Provider 架构 | ⚠️ | tui-rs/src/autocomplete_provider.rs | — | BackendProvider 为空壳，CombinedProvider 未使用 |
| 命令参数补全 | ✅ | tui-rs/src/autocomplete.rs | — | 通过协议请求后端 |
| 弹出补全行为 | ✅ | tui-rs/src/autocomplete.rs | tui/src/autocomplete.ts | Enter 确认正常 |

**注意**：大部分 "❌缺失" 命令实际上通过 submit 路径可用——Rust TUI 将未识别命令原文提交给 TS 后端处理。但本地无注册意味着：无自动补全提示、无本地帮助文本、可能出现 UX 不一致。

---

## 2. Settings/Configuration UI（24 项）

| 功能 | 状态 | Rust 源码 | TS 源码 | 差距描述 |
|------|------|-----------|---------|----------|
| Model Selector Overlay | ✅ | tui-rs/src/model_selector.rs | coding-agent/native-builtins.ts | 完整 |
| Permission Prompt UI | ✅ | tui-rs/src/permission.rs | — | 完整 |
| Settings Panel（综合设置面板）| ❌ | — | tui/src/components/settings-list.ts | TS 有循环选择菜单，Rust 无本地设置面板 |
| Theme Picker | ❌ | — | native-builtins.ts | Rust theme 为编译时静态单例 |
| Thinking Level | ⚠️ | tui-rs/src/protocol.rs (CycleThinking dead_code) | native-builtins.ts | 协议定义存在但从未发送 |
| Permission Mode | ⚠️ | tui-rs/src/sidebar.rs (只读展示) | native-builtins.ts | sidebar 展示但无法修改 |
| Auto-compact | ❌ | — | native-builtins.ts | 无对应设置 |
| Image Settings | ⚠️ | tui-rs/src/terminal_image.rs | native-builtins.ts | 硬编码 cell size，无运行时配置 |
| Transport Setting | ❌ | — | native-builtins.ts | — |
| HTTP Idle Timeout | ❌ | — | native-builtins.ts | — |
| Steering Mode | ❌ | — | native-builtins.ts | — |
| Follow-up Mode | ❌ | — | native-builtins.ts | — |
| UI Customization | ❌ | — | native-builtins.ts | 硬件光标、编辑器 padding、autocomplete 高度等 |
| Skill Commands | ❌ | — | native-builtins.ts | — |
| Double-Escape Action | ❌ | — | native-builtins.ts | — |
| Tree Filter Mode | ❌ | — | native-builtins.ts | — |
| Warnings | ❌ | — | native-builtins.ts | — |
| Permission Reviewer Model | ❌ | — | native-builtins.ts | — |
| Permission Prompt Location | ❌ | — | native-builtins.ts | — |
| Collapse Changelog | ❌ | — | native-builtins.ts | — |
| Quiet Startup | ❌ | — | native-builtins.ts | — |
| Install Telemetry | ❌ | — | native-builtins.ts | — |
| Settings Search/Filter | ❌ | — | native-builtins.ts | — |
| Overlay 定位系统 | ✅ | tui-rs/src/overlay.rs | tui/src/tui.ts | 完整 |

**关键缺陷**：Rust 协议层缺少 UpdateSetting/SetTheme/ChangePermissionMode 等消息类型，即使构建设置面板也无法与后端通信。

---

## 3. Input/Editor（15 项）

| 功能 | 状态 | Rust 源码 | TS 源码 | 差距描述 |
|------|------|-----------|---------|----------|
| 多行编辑 | ⚠️ | tui-rs/src/input.rs | tui/src/editor-component.ts | Rust 用 `\n` 分割逻辑行，但 Up/Down 不考虑视觉换行 |
| 光标移动（word/line/home/end）| ⚠️ | tui-rs/src/input.rs | tui/src/keys.ts | Word 移动仅用空格边界，TS 区分标点 |
| Kill ring | ✅ | tui-rs/src/kill_ring.rs | tui/src/kill-ring.ts | 完整 |
| Undo/Redo | ⚠️ | tui-rs/src/undo.rs | tui/src/undo-stack.ts | 用 Vec::remove(0) O(n) 淘汰 |
| 选区处理 | ❌ | — | tui/src/editor-component.ts | 无文本选择功能 |
| IME/组合输入 | ❌ | — | tui/src/stdin-buffer.ts | 无 IME 支持 |
| 粘贴检测（bracketed paste）| ⚠️ | tui-rs/src/input.rs | tui/src/stdin-buffer.ts | 有基础支持但无 paste-burst 检测 |
| 快捷键自定义 | ✅ | tui-rs/src/keymap.rs | tui/src/keybindings.ts | 完整 |
| 历史导航 | ⚠️ | tui-rs/src/input.rs | tui/src/editor-component.ts | 仅 Up/Down 遍历，无搜索 |
| Emacs/Vim 模式 | ⚠️ | tui-rs/src/input.rs | tui/src/keybindings.ts | Rust 声明了 VimNormal/VimInsert 枚举但无实际实现 |
| Kitty 键盘协议 | ⚠️ | tui-rs/src/input.rs | tui/src/keys.ts | 支持但检测不如 TS 全面 |
| Grapheme 感知编辑 | ❌ | — | tui/src/editor-component.ts | 操作基于 char 而非 grapheme cluster |
| 跳转模式 | ⚠️ | tui-rs/src/input.rs | — | 有实现 |
| 代码折叠 | 🚀 | tui-rs/src/input.rs | — | Rust 独有功能 |
| Stdin 缓冲与序列完成 | ❌ | — | tui/src/stdin-buffer.ts | TS 有完整的转义序列缓冲组装 |

---

## 4. Markdown 渲染（13 项）

| 功能 | 状态 | Rust 源码 | TS 源码 | 差距描述 |
|------|------|-----------|---------|----------|
| 代码块 + 语法高亮 | ✅ | tui-rs/src/markdown.rs, highlight.rs | tui/src/components/markdown.ts | 完整 |
| 行内代码 | ✅ | tui-rs/src/markdown.rs | 同上 | 完整 |
| 标题 h1-h6 | ⚠️ | tui-rs/src/markdown.rs (parse_heading) | 同上 | **h4-h6 被静默丢弃**，渲染为带 `#` 的纯文本 |
| 列表（有序/无序/嵌套）| ⚠️ | tui-rs/src/markdown.rs | 同上 | 不支持 task list `- [x]` |
| 链接/超链接 | ✅ | tui-rs/src/hyperlink.rs | 同上 | OSC 8 完整支持 |
| 加粗/斜体/删除线 | ⚠️ | tui-rs/src/markdown.rs (parse_inline) | 同上 | 不支持嵌套格式、下划线格式 `_italic_` |
| 表格 | ⚠️ | tui-rs/src/markdown.rs | 同上 | 不对表格内容应用行内格式 |
| 引用块 | ⚠️ | tui-rs/src/markdown.rs | 同上 | 仅支持单行，不支持多段/嵌套引用 |
| 水平分割线 | ✅ | tui-rs/src/markdown.rs | 同上 | 固定 39 字符（不随终端宽度变化） |
| 图片引用 | ⚠️ | tui-rs/src/terminal_image.rs | tui/src/terminal-image.ts | 基础设施存在但 UI 渲染路径未调用 |
| 流式 markdown | ✅ | tui-rs/src/markdown.rs | 同上 | 支持部分渲染 |
| ANSI 颜色 | ✅ | tui-rs/src/ansi.rs | 同上 | 完整 |
| 文本换行 | ⚠️ | tui-rs/src/markdown.rs (wrap_line) | 同上 | 按字符而非单词边界换行 |

---

## 5. Session 管理（12 项）

| 功能 | 状态 | Rust 源码 | TS 源码 | 差距描述 |
|------|------|-----------|---------|----------|
| Session 列表/浏览器 | ✅ | tui-rs/src/session_selector.rs | — | 完整 |
| Session 搜索/过滤 | ⚠️ | tui-rs/src/session_search.rs | — | 基础文本搜索，无 fuzzy |
| Session 树导航 | ✅ | tui-rs/src/session_tree.rs | — | 完整 |
| Session 恢复/继续 | ⚠️ | tui-rs/src/session_selector.rs | — | 可选择但无 transcript 预览 |
| Session 创建 | ❌ | — | native-builtins.ts | /new 依赖后端 |
| Session Fork/Branch | ❌ | — | native-builtins.ts | /fork 依赖后端 |
| Session 元数据显示 | ✅ | tui-rs/src/session_selector.rs | — | 完整 |
| Selector 快捷键 | ⚠️ | tui-rs/src/session_selector.rs | — | 用裸 'r' 触发重命名（易误触） |
| Session 删除 | ⚠️ | tui-rs/src/session_selector.rs | — | 有实现 |
| Session 重命名 | ⚠️ | tui-rs/src/session_selector.rs | — | 有实现但快捷键设计有问题 |
| 加载进度指示 | ⚠️ | tui-rs/src/session_selector.rs | — | loading 状态在快速切换 scope 时有竞态 |
| Session 列表缓存 | ❌ | — | — | 切换 scope 时不缓存前次结果 |

---

## 6. Permission/Overlay 系统（10 项）

| 功能 | 状态 | Rust 源码 | TS 源码 | 差距描述 |
|------|------|-----------|---------|----------|
| Permission 请求展示 | ✅ | tui-rs/src/permission.rs | tui/src/tui.ts | 完整 |
| 批准/拒绝 UI | ✅ | tui-rs/src/permission.rs | 同上 | 完整 |
| Always allow (trust) | ✅ | tui-rs/src/permission.rs | 同上 | 完整 |
| Permission 超时自动拒绝 | ✅ | tui-rs/src/permission.rs | 同上 | 完整 |
| Overlay 边框/定位 | ⚠️ | tui-rs/src/overlay.rs | tui/src/tui.ts | OverlayStack 定义但从未实例化（死代码） |
| 多 overlay 堆叠 | ⚠️ | tui-rs/src/overlay.rs | 同上 | 定义了但未使用 |
| Overlay 键盘导航 | ✅ | tui-rs/src/permission.rs | 同上 | 完整 |
| Non-capturing overlay | ⚠️ | tui-rs/src/overlay.rs | tui/src/tui.ts | 定义但未接入 |
| 终端尺寸条件可见性 | ✅ | — | tui/src/tui.ts | 完整 |
| Overlay handle API | ⚠️ | tui-rs/src/overlay.rs | tui/src/tui.ts | hide/show/focus 已定义但未使用 |

---

## 7. Terminal 渲染/性能（14 项）

| 功能 | 状态 | Rust 源码 | TS 源码 | 差距描述 |
|------|------|-----------|---------|----------|
| 渲染策略 | ⚠️ | tui-rs/src/ui.rs | tui/src/terminal.ts | Rust 用固定 50ms tick，TS 用事件驱动 + 16ms 节流 |
| 图片协议（Kitty/iTerm2）| ⚠️ | tui-rs/src/terminal_image.rs | tui/src/terminal-image.ts | 代码存在但 UI 渲染路径未调用 |
| 终端能力检测 | ✅ | tui-rs/src/terminal_caps.rs | tui/src/terminal.ts | 完整 |
| Theme 系统 | ⚠️ | tui-rs/src/theme.rs | — | 静态 LazyLock 单例，运行时无法切换 |
| Graph/Progress 可视化 | ✅ | tui-rs/src/graph.rs | — | 完整 |
| Viewport 管理 | ⚠️ | tui-rs/src/ui.rs | tui/src/terminal.ts | 基础可用 |
| 滚动行为 | ✅ | tui-rs/src/ui.rs | tui/src/tui.ts | 完整 |
| 性能优化（缓存）| ⚠️ | tui-rs/src/ui.rs (MSG_CACHE) | — | 缓存满时全量清除（非 LRU） |
| Unicode/宽字符 | ✅ | tui-rs/src/ui.rs | tui/src/terminal.ts | 完整 |
| Overlay/Dialog | ⚠️ | tui-rs/src/overlay.rs | tui/src/tui.ts | 同上 overlay 问题 |
| Dock/Sidebar | ✅ | tui-rs/src/sidebar.rs | — | 完整 |
| 超链接 OSC 8 | ✅ | tui-rs/src/hyperlink.rs | — | 完整 |
| Cell Size 查询 (CSI 16t) | ❌ | — | tui/src/terminal.ts | 硬编码 8x16 |
| Kitty 键盘协议 | ✅ | tui-rs/src/terminal_caps.rs | tui/src/keys.ts | 完整 |

---

## 8. Protocol/Backend 通信（13 项）

| 功能 | 状态 | Rust 源码 | TS 源码 | 差距描述 |
|------|------|-----------|---------|----------|
| HostMessage 消息类型 | ✅ | tui-rs/src/protocol.rs | coding-agent protocol | 完整 |
| ClientMessage 消息类型 | ✅ | tui-rs/src/protocol.rs | 同上 | 完整 |
| Unix Socket IPC | ✅ | tui-rs/src/backend/socket.rs | — | 完整 |
| 消息序列化（NDJSON）| ✅ | tui-rs/src/backend/socket.rs | — | 完整 |
| AgentBackend trait 抽象 | 🚀 | tui-rs/src/backend/mod.rs | — | Rust 有清晰 trait 抽象 |
| BackendEvent push 模型 | 🚀 | tui-rs/src/backend/mod.rs | — | 异步 stream 模型更优 |
| 类型化错误模型 | 🚀 | tui-rs/src/backend/mod.rs | — | BackendError 枚举 |
| 连接管理与生命周期 | ⚠️ | tui-rs/src/backend/socket.rs | — | connect 内用阻塞 IO |
| 消息解析错误恢复 | ✅ | tui-rs/src/backend/socket.rs | — | 静默跳过（但无日志） |
| Mock Backend 测试 | 🚀 | tui-rs/src/backend/mock.rs | — | 比 TS 更完整 |
| 图片 payload | ✅ | tui-rs/src/protocol.rs | — | 完整 |
| NativeUiState | ✅ | tui-rs/src/protocol.rs | — | 完整 |
| Dialog selected 字段 | ✅ | tui-rs/src/protocol.rs | — | 完整 |

---

## 潜在 Bug 与问题汇总

### P0 — 影响核心功能

1. **command/ 模块为死代码** — `dispatch_command()` 从未被调用（tui-rs/src/command/mod.rs:35）
2. **图片渲染未接入** — terminal_image.rs 基础设施完整但 ui.rs 仅输出 "image hidden"（ui.rs:1196-1198）
3. **Grapheme 感知缺失** — 多码点 emoji 被 backspace/delete 错误分割（input.rs）
4. **OverlayStack 死代码** — 定义但从未实例化到 AppState（overlay.rs）
5. **SocketBackend.connect() 阻塞 tokio** — 在 async fn 中使用阻塞 UnixStream::connect

### P1 — 影响体验

6. **h4-h6 标题静默丢弃** — parse_heading 将其渲染为带 `#` 的纯文本（markdown.rs）
7. **MSG_CACHE 全量清除** — 超 500 条时 cache.clear() 导致所有消息重格式化（ui.rs:74-76）
8. **固定 50ms tick** — 空闲时浪费 20 次/秒绘制，不如事件驱动（app.rs:336）
9. **Word 移动仅用空格边界** — 与 TS 的标点区分不一致（input.rs）
10. **Up/Down 不考虑视觉换行** — 跳过整行而非视觉行（input.rs）
11. **Session selector 快速切换竞态** — loading 状态不追踪 scope 来源（session_selector.rs:308）
12. **Cell size 硬编码 8x16** — HiDPI/非标准字体下图片尺寸错误（terminal_image.rs）

### P2 — 小问题

13. **Undo Vec::remove(0)** — O(n) 淘汰策略（undo.rs）
14. **Permission 导航不循环** — clamp 而非 wrap around（permission.rs）
15. **Permission 选项数硬编码 .min(3)** — 脆弱（permission.rs）
16. **Reader 线程静默丢弃解析错误** — 无日志（backend/socket.rs）
17. **send_msg() 每次 try_clone()** — 高频发送时潜在 FD 泄漏（backend/socket.rs）
18. **Rename 用裸 'r' 键** — 搜索后易误触（session_selector.rs:339）
19. **水平分割线固定 39 字符** — 宽终端显示不佳（markdown.rs）
20. **表格内无行内格式** — bold/code 在表格内显示原始 markdown（markdown.rs）
21. **Handle_key 重复绑定** — keybinding manager 和硬编码 match 冲突（input.rs）
22. **/login /logout 显示 "not supported"** — 但出现在自动补全列表中（UX 混淆）
23. **CycleThinking dead_code** — 存在但从未发送（protocol.rs）

---

## Rust TUI 独有优势

| 功能 | 源码 | 描述 |
|------|------|------|
| AgentBackend trait | tui-rs/src/backend/mod.rs | 清晰的后端抽象接口 |
| BackendEvent stream | tui-rs/src/backend/mod.rs | 异步 push 模型优于 TS 的回调 |
| 类型化错误 | tui-rs/src/backend/mod.rs | BackendError 枚举 |
| Mock Backend | tui-rs/src/backend/mock.rs | 完整测试支持 |
| 代码折叠 | tui-rs/src/input.rs | TS 无此功能 |
| /clear 快速清屏 | tui-rs/src/command/builtin.rs | 不重置 session |
