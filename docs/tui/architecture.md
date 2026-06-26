# TUI Architecture — rozsa-tui 目录规划

Related Code: [crates/rozsa-tui/src/](../../crates/rozsa-tui/src/)

## 目标结构

```
crates/rozsa-tui/src/
├── app.rs                 应用事件循环 + AppState
├── main.rs                入口
├── lib.rs                 库入口
├── protocol.rs            线格式 DTO（NativeUiState 等）
│
├── backend/               "数据从哪来" — trait + 实现
│   ├── mod.rs             AgentBackend trait + BackendEvent
│   ├── native.rs          同进程 NativeBackend
│   ├── socket.rs          Unix socket（过渡期）
│   └── mock.rs            测试用
│
├── input/                 "事件从哪来" — 按键/鼠标 → 动作
│   ├── mod.rs             InputState + CommandSink
│   ├── keys.rs            键盘事件处理
│   ├── mouse.rs           鼠标事件处理
│   ├── keymap.rs          快捷键管理器
│   ├── kill_ring.rs       剪切环
│   ├── undo.rs            撤销栈
│   └── editor.rs          编辑器模式（vim/normal）
│
├── render/                "画面怎么画" — 调度 + 主区域渲染
│   ├── mod.rs             缓存 + render() 顶层入口 + overlay 调度
│   ├── overlay.rs         焦点栈管理（OverlayStack）
│   ├── layout.rs          布局高度计算
│   ├── messages.rs        消息区渲染（直接消费 AgentMessage）
│   ├── input_box.rs       输入框渲染
│   ├── status.rs          状态行 + 通知
│   └── dialog.rs          简单对话框渲染
│
├── panels/                "独立交互面板" — 有 State + handle_key + render
│   ├── mod.rs
│   ├── graph.rs           会话历史图
│   ├── model_selector.rs  模型选择器
│   ├── session_selector.rs 会话选择器
│   ├── permission.rs      权限审批
│   ├── autocomplete.rs    自动补全
│   └── sidebar.rs         侧边栏
│
├── widgets/               "可复用 UI 原子" — 无自有 state，接参数渲染
│   ├── mod.rs
│   ├── tab_bar.rs         可滚动 tab 栏
│   ├── hints_bar.rs       底部快捷键提示
│   ├── filterable_list.rs 可过滤列表
│   └── search_input.rs    搜索输入框
│
├── util/                  "纯函数工具" — 不依赖 TUI 框架
│   ├── mod.rs
│   ├── ansi.rs            ANSI 转 ratatui Style
│   ├── markdown.rs        Markdown → Lines
│   ├── highlight.rs       语法高亮
│   ├── hyperlink.rs       OSC 8 超链接
│   ├── fuzzy.rs           模糊匹配算法
│   └── terminal.rs        终端能力检测 + 图片协议
│
├── data/                  "给 panels 提供数据" — 搜索/过滤/树构建
│   ├── mod.rs
│   ├── autocomplete_provider.rs
│   ├── session_search.rs
│   └── session_tree.rs
│
├── theme/                 颜色/样式
│   ├── mod.rs
│   └── palette.rs
│
└── command/               命令注册 + 帮助文本
    ├── mod.rs
    └── builtin.rs
```

## 分类判定规则

| 问题 | 答案 |
|------|------|
| 它有自己的交互状态 + 可独占焦点？ | → `panels/` |
| 它是纯渲染函数，接参数画东西，无自有 state？ | → `widgets/` |
| 它是主区域固定渲染的一部分？ | → `render/` |
| 它处理文本/数据但不画东西？ | → `util/` |
| 它是给某个 panel 准备数据的？ | → `data/` |
| 它处理用户输入事件？ | → `input/` |

## 关键重构

### render 去 JSON 化

现状：`render.rs` 消费 `serde_json::Value`，通过 `.get("role")` 等 JSON 操作。
`view_model.rs` 是 `AgentMessage → Value` 的适配层。

目标：`render/messages.rs` 直接 match `AgentMessage` / `Message` 枚举。
删除 `view_model.rs`。`NativeUiState.messages` 类型从 `Vec<Value>` 改为 `Vec<AgentMessage>`。

收益：类型安全、无序列化开销、编译器保护。

### overlay 统一调度

现状：`ui/mod.rs` 手动 `if state.graph.is_some() { render_graph }` 逐个判断。

目标：`render/mod.rs` 遍历 OverlayStack，每个 panel 实现统一的 render 签名。
新增 panel 时不需要改调度层。

注意：state 仍然保持为 AppState 上的 `Option<XxxState>` 字段（不用 trait object），
只统一渲染调度，不擦除类型。

## 迁移策略

分批执行，每批独立可编译：

1. **目录搬迁**（纯 mv + mod 路径更新，零逻辑变更）
2. **render.rs 拆文件**（messages/input_box/status/dialog 各自独立）
3. **widget 抽取**（tab_bar, hints_bar 从 model_selector/graph 中提取）
4. **render 去 JSON 化**（最后做，工作量最大）
