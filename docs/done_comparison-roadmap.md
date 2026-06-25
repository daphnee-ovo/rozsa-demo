# Rust TUI 改进路线图

> 生成时间：2026-05-30
> 基于 TS TUI 功能对照 + Codex 竞品分析综合得出
> 
> **最后更新：2026-05-31** — 完成全部 TS 对标 P0/P1/P2 优化

---

## 1. 功能补全优先级（TS 对齐必修）

这些是 Rust TUI 相对 TS 原版缺失或有问题的功能，直接影响用户体验和功能完整性。

### P0 — 阻塞发布

| 功能 | 源码参考 | 工作量 | 状态 | 描述 |
|------|---------|--------|------|------|
| ~~激活或删除 command/ 死代码~~ | tui-rs/src/command/mod.rs | S | ✅ 已完成 | 清理死代码，保留命令列表用于补全 |
| ~~图片渲染接入~~ | tui-rs/src/ui.rs:1196, terminal_image.rs | M | ✅ 已完成 | 接入 base64 图片到 terminal_image 渲染 |
| ~~Grapheme cluster 编辑~~ | tui-rs/src/input.rs | M | ✅ 已完成 | 引入 unicode-segmentation，全面改造 |
| ~~OverlayStack 激活或清理~~ | tui-rs/src/overlay.rs | S | ✅ 已完成 | 标记为预留，加 allow(dead_code) |
| ~~SocketBackend async connect~~ | tui-rs/src/backend/socket.rs | S | ✅ 已完成 | 改用 tokio::task::spawn_blocking |

### P1 — 重要体验

| 功能 | 源码参考 | 工作量 | 状态 | 描述 |
|------|---------|--------|------|------|
| ~~h4-h6 标题渲染~~ | tui-rs/src/markdown.rs (parse_heading) | S | ✅ 已完成 | 扩展到支持 h1-h6 |
| ~~MSG_CACHE LRU 策略~~ | tui-rs/src/ui.rs:74-76 | S | ✅ 已完成 | 改用 lru crate，500 条 LRU 淘汰 |
| ~~事件驱动渲染替换 50ms tick~~ | tui-rs/src/app.rs:336 | M | ✅ 已完成 | 按需渲染 + 120FPS 帧率限制 |
| ~~Word 移动标点感知~~ | tui-rs/src/input.rs | S | ✅ 已完成 | 区分 word char / punctuation |
| ~~Up/Down 视觉行移动~~ | tui-rs/src/input.rs | M | ✅ 已完成 | grapheme-aware 光标位置计算 |
| ~~IME/组合输入支持~~ | tui-rs/src/input.rs | M | ✅ 已完成 | grapheme-aware + 正确光标定位 |
| ~~文本选区~~ | tui-rs/src/input.rs | L | ✅ 已完成 | Shift+方向键 + 选区删除/替换 |
| ~~Cell size 动态查询~~ | tui-rs/src/terminal_image.rs | S | ✅ 已完成 | TIOCGWINSZ ioctl 查询 |
| ~~Settings 协议扩展~~ | tui-rs/src/protocol.rs | M | ✅ 已完成 | UpdateSetting 消息 + AgentBackend trait 方法 |
| ~~Session selector 竞态修复~~ | tui-rs/src/session_selector.rs:308 | S | ✅ 已完成 | loading_scope 追踪 |
| ~~Reader 线程错误日志~~ | tui-rs/src/backend/socket.rs | S | ✅ 已完成 | tracing::warn 日志 |
| ~~表格内行内格式~~ | tui-rs/src/markdown.rs | S | ✅ 已完成 | 对 cell 内容调用 parse_inline |
| ~~嵌套 markdown 格式~~ | tui-rs/src/markdown.rs (parse_inline) | M | ✅ 已完成 | 递归解析支持任意嵌套 |
| ~~多段引用块~~ | tui-rs/src/markdown.rs | S | ✅ 已完成 | 支持嵌套引用 `>> ` |

### P2 — 改善体验

| 功能 | 源码参考 | 工作量 | 状态 | 描述 |
|------|---------|--------|------|------|
| ~~Undo VecDeque 优化~~ | tui-rs/src/undo.rs | S | ✅ 已完成 | VecDeque O(1) 淘汰 |
| ~~Permission 导航循环~~ | tui-rs/src/permission.rs | S | ✅ 已完成 | wrap around |
| ~~send_msg 避免 try_clone~~ | tui-rs/src/backend/socket.rs | S | ✅ 已完成 | 在 lock 内直接写入 |
| ~~Session rename 快捷键改进~~ | tui-rs/src/session_selector.rs:339 | S | ✅ 已完成 | 改为 Ctrl+R |
| ~~自适应水平线宽度~~ | tui-rs/src/markdown.rs | S | ✅ 已完成 | min(width, 80) |
| /help 动态命令列表 | tui-rs/src/command/builtin.rs | S | 部分完成 | 已扩展列表，动态化需后端支持 |
| /hotkeys 动态配置 | tui-rs/src/command/builtin.rs | S | 部分完成 | 扩展文本，动态化需 keymap 变更 |
| ~~Underscore 斜体支持~~ | tui-rs/src/markdown.rs | S | ✅ 已完成 | _italic_ 和 __bold__ |
| ~~Task list 支持~~ | tui-rs/src/markdown.rs | S | ✅ 已完成 | ☑ / ☐ 渲染 |
| ~~重复 handle_key 绑定清理~~ | tui-rs/src/input.rs | S | ✅ 已完成 | 移除冗余 match arms |

---

## 2. 竞品学习优先级（Codex 借鉴）

这些是从 Codex 学到的值得采纳的功能或模式。

### P0 — 核心稳定性（v1.0.x 目标对齐）

| 功能 | Codex 参考 | 我们的目标文件 | 工作量 | 描述 |
|------|-----------|--------------|--------|------|
| Job Control (Ctrl+Z) | tui/job_control.rs | tui-rs/src/main.rs | M | 正确挂起/恢复，保存终端状态 |
| Frame Rate Limiter | tui/frame_rate_limiter.rs | tui-rs/src/app.rs (新模块) | M | 120 FPS 上限，按需绘制 |
| Paste burst 检测 | bottom_pane/paste_burst.rs | tui-rs/src/input.rs (新模块) | S | 无 bracketed paste 时的粘贴检测 |
| Event pause/resume | tui/event_stream.rs | tui-rs/src/app.rs | M | 子进程时暂停事件 |
| 键盘模式检测 | tui/keyboard_modes.rs | tui-rs/src/terminal_caps.rs | M | WSL/tmux/VSCode 适配 |

### P1 — 渲染质量

| 功能 | Codex 参考 | 我们的目标文件 | 工作量 | 描述 |
|------|-----------|--------------|--------|------|
| 双区域流式模型 | streaming/controller.rs | tui-rs/src/ui.rs (新模块) | L | Stable + Tail 避免全量重渲染 |
| Table holdback | streaming/table_holdback.rs | tui-rs/src/markdown.rs | S | 表格完成后一次性渲染 |
| Markdown stream collector | markdown_stream.rs | tui-rs/src/markdown.rs | M | 换行门控 |
| Live wrap | live_wrap.rs | tui-rs/src/ui.rs | M | 流式逐字符换行 |

### P2 — UX 增强

| 功能 | Codex 参考 | 我们的目标文件 | 工作量 | 描述 |
|------|-----------|--------------|--------|------|
| Vim 模式 | bottom_pane/textarea/vim.rs | tui-rs/src/input.rs (新模块) | L | text object + operator |
| Ctrl+R 历史搜索 | chat_composer/history_search.rs | tui-rs/src/input.rs | M | 增量反向搜索 |
| 桌面通知 | notifications/osc9.rs, bel.rs | tui-rs/src/ (新模块) | S | 长任务完成通知 |
| Transcript 预览 | resume_picker/transcript.rs | tui-rs/src/session_selector.rs | M | Session 恢复时预览 |
| AppEvent 事件总线 | app_event.rs | tui-rs/src/app.rs (重构) | L | 解耦架构 |
| Skills toggle UI | bottom_pane/skills_toggle_view.rs | tui-rs/src/ (新模块) | M | 技能管理界面 |
| 审批选项扩展 | chatwidget/permission_popups.rs | tui-rs/src/permission.rs | M | 更细粒度的 always allow |

### P3 — 锦上添花

| 功能 | Codex 参考 | 我们的目标文件 | 工作量 | 描述 |
|------|-----------|--------------|--------|------|
| Diff 渲染增强 | diff_model.rs, diff_render.rs | tui-rs/src/ui.rs | M | unified diff + 高亮 |
| Multi-agent 导航 | multi_agents.rs | — | L | 多 agent 视图切换 |
| Onboarding flow | onboarding/ | — | M | 首次运行引导 |
| Terminal probing | terminal_probe.rs | tui-rs/src/terminal_caps.rs | S | 批量终端探测 |
| Resize reflow 防抖 | app/resize_reflow.rs | tui-rs/src/app.rs | S | 高频 resize 合并 |

---

## 3. Bug 修复清单

按严重程度排序（2026-05-31 更新）：

| # | 严重度 | Bug | 位置 | 状态 |
|---|--------|-----|------|------|
| 1 | 🔴 | command/ 模块从未被调用（死代码） | tui-rs/src/command/mod.rs:35 | ✅ 已清理 |
| 2 | 🔴 | 图片渲染路径未接入 | tui-rs/src/ui.rs:1196 | ✅ 已接入 |
| 3 | 🔴 | Grapheme 不感知导致 emoji 编辑错误 | tui-rs/src/input.rs | ✅ 已修复 |
| 4 | 🔴 | OverlayStack 死代码 | tui-rs/src/overlay.rs | ✅ 已标记预留 |
| 5 | 🔴 | 阻塞 connect 在 async fn | tui-rs/src/backend/socket.rs | ✅ 已修复 |
| 6 | 🟡 | h4-h6 标题静默丢弃 | tui-rs/src/markdown.rs | ✅ 已修复 |
| 7 | 🟡 | MSG_CACHE 全量清除 | tui-rs/src/ui.rs:74-76 | ✅ 已修复 |
| 8 | 🟡 | 50ms 固定 tick 浪费 CPU | tui-rs/src/app.rs:336 | ✅ 已修复 |
| 9 | 🟡 | Word 移动不区分标点 | tui-rs/src/input.rs | ✅ 已修复 |
| 10 | 🟡 | Up/Down 忽略视觉换行 | tui-rs/src/input.rs | ✅ 已修复 |
| 11 | 🟡 | Session scope 切换竞态 | tui-rs/src/session_selector.rs:308 | ✅ 已修复 |
| 12 | 🟡 | Cell size 硬编码 8x16 | tui-rs/src/terminal_image.rs | ✅ 已修复 |
| 13 | 🟢 | Undo Vec::remove(0) O(n) | tui-rs/src/undo.rs | ✅ 已修复 |
| 14 | 🟢 | Permission 导航不循环 | tui-rs/src/permission.rs | ✅ 已修复 |
| 15 | 🟢 | send_msg try_clone FD 泄漏 | tui-rs/src/backend/socket.rs | ✅ 已修复 |
| 16 | 🟢 | Rename 裸 'r' 键误触 | tui-rs/src/session_selector.rs:339 | ✅ 已修复 |
| 17 | 🟢 | Reader 静默丢弃错误 | tui-rs/src/backend/socket.rs | ✅ 已修复 |
| 18 | 🟢 | handle_key 重复绑定 | tui-rs/src/input.rs | ✅ 已修复 |
| 19 | 🟢 | CycleThinking dead_code | tui-rs/src/protocol.rs | ✅ 已激活 |
| 20 | 🟢 | 水平线固定宽度 | tui-rs/src/markdown.rs | ✅ 已修复 |

---

## 4. 已有优势维护

以下是我们做得好的地方，改进过程中应注意保持：

| 优势 | 文件 | 维护要点 |
|------|------|----------|
| AgentBackend trait 抽象 | backend/mod.rs | 重构事件总线时保持 trait 边界清晰 |
| BackendEvent stream 模型 | backend/mod.rs | 不要退化为回调模式 |
| BackendError 类型化错误 | backend/mod.rs | 扩展时保持枚举完整性 |
| Mock backend 测试支持 | backend/mock.rs | 每个新功能都加 mock 测试 |
| 代码折叠功能 | input.rs | Codex 无此功能，是差异化亮点 |
| 协议层清晰分离 | protocol.rs | 新增消息类型时保持一致 |
| Session tree 可视化 | session_tree.rs, graph.rs | 继续增强 |
| Fuzzy 算法独立模块 | fuzzy.rs | 保持可复用性 |
| 简洁编译 | 全局 | 新增模块时注意文件不要爆炸增长 |
| Kill ring 完整实现 | kill_ring.rs | 保持 |
| Keymap 可配置 | keymap.rs | 扩展时保持向后兼容 |

---

## 与 v1.0.x 目标对齐

当前目标：**稳定性保障（panic recovery、内存泄漏修复）、渲染性能（按需重绘、缓存优化）、核心 UX 补全（paste marker、IME 光标、job control）**

| 目标 | 本报告对应项 | 最高优先级任务 |
|------|-------------|---------------|
| panic recovery | Bug #5 (async connect), Bug #4 (dead code cleanup) | 清理死代码消除潜在 panic 源 |
| 内存泄漏修复 | Bug #15 (try_clone FD), Bug #7 (cache) | LRU 缓存 + 共享 writer |
| 按需重绘 | Codex P0: Frame Rate Limiter + Requester | 替换 50ms tick |
| 缓存优化 | Bug #7 (MSG_CACHE LRU) | 分代/LRU 缓存 |
| paste marker | Codex P0: Paste burst 检测 | 状态机实现 |
| IME 光标 | TS P1: IME/组合输入 | stdin 缓冲 + 组合状态 |
| job control | Codex P0: Job Control | 信号处理 + 终端状态保存 |
