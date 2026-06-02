# Rust TUI 改进路线图

> 基于 TS 对等分析与 Codex 竞品分析，为 `packages/tui-rs` 制定的可执行改进计划。
>
> **最后更新：2026-05-31** — 全部 TS 对标优化已完成
>
> 优先级定义：
> - **P0** — 阻塞发布 / 影响核心可用性
> - **P1** — 重要改进，显著提升体验或架构可维护性
> - **P2** — 锦上添花 / 面向未来的投资

---

## 1. 功能补全优先级（TS 对等）

需要从 TS TUI 补齐的功能缺失，按优先级和工作量排列。

| 优先级 | 功能 | 来源文件 | 工作量 | 状态 | 描述 |
|--------|------|----------|--------|------|------|
| P0 | Grapheme-aware 编辑 | `input.rs` | M | ✅ | 引入 `unicode-segmentation`，全面改造 |
| P0 | 删除 command/ 死代码 | `command/` | S | ✅ | 清理 dispatch_command + registry |
| P0 | /help 动态化 | `command/builtin.rs` | S | ✅ | 扩展命令列表为 27+ 项 |
| P0 | 按词换行（word wrap） | `ui.rs` | M | ✅ | word-boundary 断行 |
| P1 | Settings 面板 | `protocol.rs` | L | ✅ | UpdateSetting 协议 + overlay 系统就绪 + /settings 通过 submit 触发 |
| P1 | Thinking level 选择器 | `input.rs` | M | ✅ | CycleThinking 已激活发送 |
| P1 | IME/Composition 支持 | `input.rs` | M | ✅ | grapheme-aware + 正确光标定位 |
| P1 | Visual line map + 粘性列 | `input.rs` | L | ✅ | grapheme-aware 光标计算 |
| P1 | Markdown heading h4-h6 | `markdown.rs` | S | ✅ | 支持 h1-h6 |
| P1 | Markdown 嵌套内联格式 | `markdown.rs` | M | ✅ | 递归解析器 |
| P1 | Markdown 表格内行内格式 | `markdown.rs` | M | ✅ | cell 调用 parse_inline |
| P1 | Markdown blockquote 嵌套 | `markdown.rs` | M | ✅ | 递归嵌套引用 |
| P1 | CSI 16t 单元格尺寸查询 | `terminal_image.rs` | S | ✅ | TIOCGWINSZ ioctl |
| P1 | Image 实际渲染 | `ui.rs` | M | ✅ | 接入 terminal_image |
| P1 | 大文本粘贴 marker | `app.rs` | M | ✅ | grapheme-aware 原子粘贴 |
| P1 | Session list caching | `session_selector.rs` | S | ✅ | loading_scope 追踪 |
| P1 | Fuzzy 匹配 | `session_search.rs` | M | ✅ | 已接入 fuzzy_match |
| P2 | /hotkeys 动态化 | `command/builtin.rs` | S | ✅ | 扩展帮助文本 |
| P2 | Task list 支持 | `markdown.rs` | S | ✅ | ☑ / ☐ 渲染 |
| P2 | Underscore 强调支持 | `markdown.rs` | S | ✅ | `_italic_` + `__bold__` |
| P2 | HR 宽度自适应 | `markdown.rs` | S | ✅ | min(width, 80) |
| P2 | Theme 运行时切换 | `theme.rs` | L | ✅ | RwLock + dark/light 双主题 |
| P2 | Overlay 系统接入 | `overlay.rs` | M | ✅ | OverlayStack 接入 AppState |
| P2 | 链接 text==href 优化 | `hyperlink.rs` | S | ✅ | text==url 时不重复显示 |
| P2 | 文本选区 | `input.rs` | L | ✅ | Shift+方向键 + 选区操作 |

---

## 2. 竞品学习优先级（Codex）

从 Codex TUI 中值得学习并引入的架构和功能。

| 优先级 | 功能 | Codex 来源 | 我方目标文件 | 工作量 | 描述 |
|--------|------|------------|-------------|--------|------|
| P0 | 事件驱动渲染 (Frame Requester) | `tui/frame_requester.rs` + `frame_rate_limiter.rs` | `app.rs` 事件循环 | M | 替换固定 50ms tick 为按需请求 + 120FPS 限流。空闲时零 CPU，burst 时平滑。直接影响性能和能耗 |
| P0 | Job Control (Ctrl+Z 挂起/恢复) | `tui/job_control.rs` | `app.rs` | M | 当前 Ctrl+Z 会损坏终端状态。需正确离开 alt-screen、恢复终端模式、处理 SIGTSTP |
| P0 | Event Stream 暂停/恢复 | `tui/event_stream.rs` EventBroker | `app.rs` | M | 外部编辑器 (Ctrl+G) 启动时 crossterm reader 仍窃取 stdin。需 drop/recreate EventStream 的能力 |
| P0 | 键盘模式兼容 (WSL/tmux/VSCode) | `tui/keyboard_modes.rs` | `app.rs` 键盘初始化 | M | 当前无 WSL+VSCode 兼容、tmux csi-u 探测、crash 后硬重置。实际用户会遇到按键异常 |
| P1 | Typed Event Bus (AppEvent enum) | `app_event.rs` (~150 variants) | 新建 `event.rs` | L | 解耦 widget 与 transport。当前通过 Arc<Mutex<UnixStream>> 直接通信，耦合严重 |
| P1 | App 子模块分解 | `app/session_lifecycle.rs` 等 20+ 模块 | `app.rs` 拆分为 `app/` 目录 | L | 当前 app.rs 单文件承载所有逻辑。按职责拆分为 event_dispatch、session、config、resize 等子模块 |
| P1 | 流式渲染 pipeline (Stable+Tail) | `streaming/controller.rs` + `table_holdback.rs` | 新建 `streaming/` | L | 两区域模型：已提交稳定区 + 可变尾部。表格 holdback 防止部分渲染的表格提交到 scrollback |
| P1 | Newline-gated markdown 收集器 | `markdown_stream.rs` | `ui.rs` 流式处理 | M | 按换行边界提交源文本，防止半成品 markdown 结构（半个 heading、半个 fence）引起视觉闪烁 |
| P1 | 桌面通知 (OSC9 + BEL) | `notifications/mod.rs` | 新建 `notifications.rs` | S | 长时间操作完成时通知用户。auto-detect 终端能力，支持 tmux passthrough。简单高 ROI |
| P1 | 多类型审批 + 队列 | `approval_overlay.rs` | `permission.rs` | L | 区分 exec/patch/network/MCP 四类审批，FIFO 队列防止丢失。渐进式信任：命令前缀记忆、host 级策略 |
| P1 | Auto-review 拒绝追踪 + 追溯审批 | `auto_review_denials.rs` | 新建 `auto_review.rs` | M | 记录近 10 条自动拒绝，用户可浏览并追溯审批。半自治运行的关键 UX |
| P1 | Session resume 全文预览 | `resume_picker/transcript.rs` | `session_selector.rs` | L | 恢复会话前显示对话全文预览，避免恢复错误的会话 |
| P1 | Vim mode 实现 | `textarea/vim.rs` | `input.rs` + 新建 `vim.rs` | L | EditorMode::VimNormal/VimInsert 已声明但无实现。需 operator/motion/text-object 状态机 |
| P1 | Ctrl+R 逆向历史搜索 | `chat_composer/history_search.rs` | `input.rs` | M | bash 风格 reverse-i-search：footer 变搜索输入，高亮匹配，双向遍历，取消恢复草稿 |
| P1 | Paste burst 检测 | `paste_burst.rs` | 新建 `paste_burst.rs` | M | 基于时序的状态机检测无 bracketed paste 的快速输入（Windows/旧终端），防止逐字触发 autocomplete 和误提交 |
| P1 | Enum-based Command Registry | `slash_command.rs` (50+ variants) | `command/` 重建 | M | 替换 HashMap<str, fn> 为 strum enum + 元数据方法（description, supports_args, available_during_task），编译期保证穷尽处理 |
| P1 | Feature-gated 命令可见性 | `slash_commands.rs` BuiltinCommandFlags | `autocomplete.rs` | M | 命令弹出菜单仅显示当前上下文可用的命令，减少认知负担 |
| P2 | Adaptive chunking (Smooth/CatchUp) | `streaming/chunking.rs` | `streaming/` | M | 双档自适应策略 + 滞环防抖：低负载逐行动画，高负载批量追赶 |
| P2 | Diff 渲染 + 行号 + 语法高亮 | `diff_render.rs` | 新建 `diff_render.rs` | L | 工具输出中的 diff 当前仅前景着色。需行号、gutter sign、语法高亮、wrap 保色 |
| P2 | HistoryCell trait 对话项类型化 | `history_cell/mod.rs` | 新建 `history_cell.rs` | L | 对话条目类型化（Message/Exec/Patch/Notice），支持单条目滚动/复制/搜索/resize reflow |
| P2 | Skills 管理 UI (toggle view) | `skills_toggle_view.rs` | 新建 `skills_view.rs` | M | 全屏技能列表，搜索/滚动/space 切换启停，退出时汇报变更摘要 |
| P2 | VT100 Test Backend | `test_backend.rs` | `tests/` | M | 用 vt100::Parser 模拟真终端做渲染断言，覆盖当前 mock 无法验证的视觉输出 |
| P2 | Transcript reflow on resize | `transcript_reflow.rs` | `ui.rs` | M | 终端缩放时重新换行已渲染内容（debounce 75ms），解决缩窗后文字截断问题 |
| P2 | macOS stderr 抑制 | `terminal_stderr.rs` | 新建 `stderr_guard.rs` | S | dup2 重定向 stderr 到 /dev/null 防止 macOS framework 诊断损坏 TUI 画面 |
| P2 | 语音输入 | `voice.rs` | 未来规划 | XL | cpal 录音 + 24kHz PCM16 + 电平指示。差异化特性但工作量极大 |

---

## 3. Bug 修复清单

按严重程度排列。

### P0 — 阻塞/数据正确性

| # | 文件 | 问题描述 | 状态 |
|---|------|----------|------|
| 1 | `input.rs` | Grapheme 操作用 char 级别，emoji/flag/skin-tone 会被错误分割 | ✅ 已修复 |
| 2 | `backend/socket.rs` | `send_msg()` 每次调用 `try_clone()` 克隆 fd | ✅ 已修复 |
| 3 | `backend/socket.rs` | `connect()` 在 async fn 内使用阻塞 `UnixStream::connect` | ✅ 已修复 |
| 4 | `app.rs` | Ctrl+Z (SIGTSTP) 无处理，终端状态损坏 | 已有基础处理 |

### P1 — 功能/体验缺陷

| # | 文件 | 问题描述 | 状态 |
|---|------|----------|------|
| 5 | `ui.rs` L1196 | Image 渲染只有 stub，terminal_image.rs 未接入 | ✅ 已修复 |
| 6 | `markdown.rs` | h4-h6 heading 静默丢弃 | ✅ 已修复 |
| 7 | `markdown.rs` | 嵌套内联格式 (`**bold *italic***`) 误解析 | ✅ 已修复 |
| 8 | `markdown.rs` | 表格单元格内的内联格式显示原始 markdown | ✅ 已修复 |
| 9 | `markdown.rs` | blockquote 不支持嵌套引用 | ✅ 已修复 |
| 10 | `markdown.rs` | 链接解析拒绝含空格 URL | ✅ 已修复 |
| 11 | `ui.rs` wrap_line_with_prefix | 按字符断行，英文单词被劈开 | ✅ 已修复 |
| 12 | `terminal_image.rs` | `calculate_cell_size()` 硬编码 8x16 | ✅ 已修复 |
| 13 | `input.rs` | Up/Down 按逻辑行移动 | ✅ 已修复（grapheme-aware） |
| 14 | `input.rs` | word movement 仅 whitespace 分词 | ✅ 已修复 |
| 15 | `session_selector.rs` L339 | Rename 绑定裸 'r' 键误触 | ✅ 已修复 |
| 16 | `session_selector.rs` L308 | 快速切换 scope 时 loading 竞态 | ✅ 已修复 |
| 17 | `protocol.rs` | `CycleThinking` 从未发送 | ✅ 已激活 |
| 18 | `permission.rs` | 导航不循环 | ✅ 已修复 |
| 19 | `permission.rs` | `selected.min(3)` 硬编码选项数 | ✅ 已修复（wrap 逻辑） |
| 20 | `permission.rs` | 从 `serde_json::Value` 非类型化提取字段 | 待实现 |

### P2 — 代码卫生/性能

| # | 文件 | 问题描述 | 状态 |
|---|------|----------|------|
| 21 | `command/mod.rs` | `dispatch_command()` 死代码 | ✅ 已清理 |
| 22 | `command/mod.rs` L35 | CommandRegistry 每次重建 HashMap | ✅ 已删除 |
| 23 | `undo.rs` | UndoStack eviction 用 `Vec::remove(0)` (O(n)) | ✅ 已修复 |
| 24 | `ui.rs` L74 | MSG_CACHE 全清 | ✅ 已修复（LRU） |
| 25 | `app.rs` | 固定 50ms tick | ✅ 已修复 |
| 26 | `input.rs` | matches_action 与硬编码 match 重复绑定 | ✅ 已清理 |
| 27 | `autocomplete_provider.rs` | BackendProvider/CombinedProvider 死代码 | ✅ 已清理 |
| 28 | `markdown.rs` | 不支持 underscore 强调 | ✅ 已修复 |
| 29 | `markdown.rs` | HR 固定 39 字符 | ✅ 已修复 |
| 30 | `backend/socket.rs` | reader 静默丢弃错误 | ✅ 已修复 |
| 31 | `backend/socket.rs` | `writer()` 也调 try_clone() | 保留（兼容现有 protocol::send） |

---

## 4. 已有优势维护

以下是 Rust TUI 已有的优势，在改进过程中应当保持甚至强化。

| 领域 | 优势 | 文件 | 说明 |
|------|------|------|------|
| 后端抽象 | AgentBackend trait | `backend/mod.rs` | 20 个 async 方法 + typed BackendError + BackendEvent enum，比 Codex 和 TS 的耦合设计都更清晰 |
| 测试基础 | MockBackend | `backend/mock.rs` | 支持预设事件注入 + 调用记录，实现完全隔离的单元测试 |
| 编辑器 | Kill Ring 多条目旋转 | `kill_ring.rs` | Emacs 风格多条目 kill ring + yank-pop，比 Codex 单条目 buffer 更完整 |
| 编辑器 | Jump-to-char 模式 | `input.rs` | 两键跳转（Alt+]/[），行内快速导航，竞品均无 |
| 编辑器 | 代码折叠 | `input.rs` | 基于缩进的折叠/展开 (Alt+Shift+[/])，TS 和 Codex 均无 |
| 编辑器 | EditorComponent trait | `editor_component.rs` | 清晰的 trait 抽象支持未来 vim/emacs 切换 |
| 编辑器 | 外部编辑器集成 | `input.rs` Ctrl+G | 正确挂起/恢复 TUI 并启动 $EDITOR |
| 键绑定 | KeybindingsManager | `keymap.rs` | 后端绑定 + 用户覆盖合并，数据驱动可扩展 |
| 协议 | 完整消息类型对等 | `protocol.rs` | 14 Host + 19 Client 消息类型完全覆盖 TS 定义 |
| 协议 | 生命周期引用优化 | `protocol.rs` ClientMessage | `&'a str` 避免序列化时 clone 字符串 |
| 协议 | dialog selected 字段 | `protocol.rs` | 正确建模了 TS 类型定义中遗漏但实际传输的 selected 字段 |
| 会话 | 三模式搜索 | `session_search.rs` | token/regex/phrase 三模式，带位置评分和名称加权 |
| 会话 | 树形可视化 | `session_tree.rs` | 基于 parent_session_path 的层级树，DFS 展平 + Unicode 树线绘制 |
| 会话 | 多排序/筛选 | `session_selector.rs` | scope/sort/name-filter/path-toggle 多维组合 |
| 渲染 | 消息级 hash 缓存 | `ui.rs` MSG_CACHE | 非流式消息按内容 hash 缓存格式化结果，零重复格式化开销 |
| 渲染 | ANSI 解析器 | `ansi.rs` | 完整 ANSI->ratatui Style 转换，支持 256 色和 RGB 真彩 |
| 渲染 | 语法高亮 | `highlight.rs` | syntect 内置 + 安全限制 (512KB/10000 行) 防 OOM |
| 渲染 | 超链接 | `hyperlink.rs` | OSC 8 + 能力检测 + URL 验证 + fallback 渲染 |
| 渲染 | 图片渲染基础 | `terminal_image.rs` | Kitty/iTerm2 双协议 + MIME 检测，基础设施完备（待接入 UI） |
| 终端 | 能力检测 | `terminal_caps.rs` | 集中式 LazyLock 检测 images/trueColor/hyperlinks，覆盖主流终端 |
| 架构 | Overlay 类型系统 | `overlay.rs` | 9 点锚定 + Fixed/Percent 尺寸 + 最小终端检查 + 焦点栈 |
| 架构 | Graph 可视化 | `graph.rs` | 内置 session graph 浏览器（列表+详情+过滤+markdown预览） |
| 架构 | 轻量代码量 | 34 源文件 | 相比 Codex 200+ 文件更易理解和上手 |
| 权限 | 超时自动拒绝 | `permission.rs` | 300s 倒计时 + 双档颜色警告，防止挂起阻塞 agent |
| 权限 | 风险级别显示 | `permission.rs` | header 中显示结构化 risk level，Codex 无此信息 |
| Markdown | 内联图片渲染 | `markdown.rs` | `![alt](path)` 直接内联渲染图片，TS 需外部组合 |
| 渲染 | Ratatui cell 级 wide char | `ui.rs` + ratatui | 宽字符被部分覆盖时自动清理相邻 cell，行级方案无此保护 |

---

## 实施建议

### 第一阶段（v1.1 目标）— 核心可用性

聚焦 P0 项目：
1. **Grapheme 修复** — 引入 `unicode-segmentation`，改造所有 char 操作
2. **Word wrap 修复** — 改为按词断行
3. **事件驱动渲染** — 替换 50ms tick，实现 frame requester + rate limiter
4. **Job control** — Ctrl+Z 正确挂起/恢复
5. **键盘兼容** — WSL/tmux/VSCode 探测 + crash 硬重置
6. **Event stream pause** — 支持外部编辑器 stdin 交接
7. **清理死代码** — 删除 command/ 模块 + autocomplete_provider placeholder

### 第二阶段（v1.2 目标）— 体验提升

聚焦 P1 项目：
1. **Settings 面板** + thinking level + permission mode 运行时切换
2. **流式渲染 pipeline** — stable/tail + newline gating + table holdback
3. **Typed Event Bus** + App 子模块分解
4. **Markdown 完善** — heading/table/blockquote/inline nesting
5. **桌面通知** + Ctrl+R 历史搜索
6. **多类型审批** + auto-review denial 追踪
7. **Session 全文预览** + fuzzy search

### 第三阶段（v1.3+ 目标）— 差异化

P2 项目按需安排：
- Vim mode 完整实现
- Diff 渲染器
- HistoryCell 类型化对话
- Skills 管理 UI
- VT100 测试后端
- Theme 运行时切换
- Adaptive streaming chunking

---

*文档生成日期：2026-05-30*
*基于：TS 对等分析 (5 领域 / 34 条 feature) + Codex 竞品分析 (6 领域 / 50 条 feature)*
