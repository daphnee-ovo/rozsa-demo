# SPEC: macOS 原生 Sidebar 与 Main Panel 容器

## Goal

在 macOS GUI 中用 `NSSplitViewController` 管理持久的 sidebar 与 main panel 两个原生 pane。Main view 与 Settings view 复用这两个 pane，只切换各 pane 内的 scene 内容，不再各自维护独立的 sidebar 布局、折叠状态和窗口背景边界。

## Scope

- macOS 使用 `NSSplitViewController`、sidebar `NSSplitViewItem` 和 main `NSSplitViewItem`。
- sidebar pane 与 main pane 各承载一个持久 Tauri WebView；scene 切换不销毁或重建 WebView。
- sidebar WebView 在 `MainSidebar` 与 `SettingsSidebar` 间切换内容；main WebView 在 `MainContent` 与 `SettingsContent` 间切换内容。
- sidebar 折叠、展开、拖动宽度、窄窗口自动折叠、全屏 overlay 和 divider persistence 由 AppKit 管理。
- 保留现有前端控件、视觉语言、session、permission、runtime 语义和 Tauri command 接口。
- 非 macOS 平台继续使用 WebView 内的 CSS 布局，不因本次 macOS 改造失去功能。

这里的 command 接口保持不变，特指现有 session、permission、settings 和 agent 业务 commands。窗口 scene 协调允许新增独立 IPC，不改变已有 command 名称或 payload。

不在本次范围：把 session list、status、settings navigation 或 main content 重写成 AppKit 控件；重新设计 Settings 信息架构；改变现有 session、permission 或 agent loop 接口。

## Requirements Trace

| Requirement | Design |
| --- | --- |
| macOS 使用原生 sidebar + main panel | `NativeSplitHost` 安装 `NSSplitViewController` 与两个 `NSSplitViewItem` |
| Main/Settings 复用同一 sidebar 容器 | sidebar WebView 与原生 sidebar item 保持不变，只更新 `SidebarScene` |
| Main/Settings 复用同一 main panel 容器 | main WebView 与原生 content item 保持不变，只更新 `ContentScene` |
| 保持现有功能和状态 | Rust runtime 继续作为状态源；两 WebView 接收各自的 targeted snapshot/event |
| 获得原生窗口行为 | toggle、divider、collapse、fullscreen overlay、autosave 不再由 CSS 模拟 |

## Design

### 1. NativeSplitHost

在 `crates/rozsa-gui/src/` 新增单一职责的 macOS bridge，例如 `native_split_view.rs`。它负责：

1. 在主线程取得 Tauri `NSWindow` 与两个 `WKWebView`。
2. 创建一个持久 `NSSplitViewController`。
3. 用 `sidebarWithViewController:` 创建 sidebar item，用普通 `splitViewItemWithViewController:` 创建 main item。
4. 将两个 WebView 分别挂载到 sidebar/main child view controller，并用 Auto Layout 贴合容器。
5. 把 split controller 挂入现有 Tauri window 的 controller/view hierarchy，不替换 Tauri 的 `NSWindow` ownership。
6. 将现有 titlebar sidebar button 的 action 改为调用 split controller 的 `toggleSidebar:`。

AppKit 对象由 window/controller hierarchy 持有，不放入要求 `Send + Sync` 的 Tauri managed state。Rust bridge 只暴露小接口：安装、切换 sidebar、读取稳定布局状态。

`objc2-app-kit` 启用现有版本中的 `NSViewController`、`NSSplitView`、`NSSplitViewController`、`NSSplitViewItem` 及必要约束 features，不引入新的 UI framework。

AppKit 是两个 pane frame 的唯一 layout owner。安装顺序固定为：创建 child WebView、停止使用 Tauri bounds API 调整其 frame、挂入两个 child view controller、安装 Auto Layout constraints、再显示 native split。安装后禁止业务代码调用两个 WebView 的 `set_bounds`、`set_size` 或 `set_position`。技术验证必须记录 resize 前后的 parent、frame 与 Tauri bounds，证明不存在双重布局写入。

安装失败时按相反顺序清理已创建的 child WebView、constraints、split items 和 child controllers，并恢复原 single-WebView hierarchy；清理失败必须报告错误并停止启动，不能留下半安装界面。

### 2. 两个持久 WebView

- 现有 `main` WebView 变为 main panel WebView，继续承载 chat、composer、permission panel 和 settings content。
- 新建 `sidebar` child WebView，承载 session/status sidebar 与 settings navigation。
- sidebar 与 main panel 的根节点不再通过同一 CSS Grid 划分宽度。
- 两个 WebView 由 Tauri manager 持有；scene 切换只更新 DOM 状态，不执行 reload、close 或重新创建。

Tauri 2.11.5 的 Rust `Window::add_child` 受 `unstable` feature 约束，且把其 `WKWebView` 重新挂入 AppKit controller hierarchy 不是当前项目已验证路径。因此实现必须先完成 NativeSplitHost 技术验证；验证失败时停止迁移，不自动退回自绘 split，也不直接扩大为“原生 AppKit sidebar 内容重写”。

workspace 的 Tauri dependency 必须固定为已验证的 exact `2.11.5`，明确启用 `unstable` feature，并验证 `Cargo.lock` 中 Tauri/Wry 版本与记录一致。后续升级 Tauri minor version 时必须重新执行 NativeSplitHost 生命周期验证。

### 3. Scene 与状态边界

```text
GuiScene::Main
  sidebar WebView → MainSidebar
  main WebView    → MainContent

GuiScene::Settings { selected_pane }
  sidebar WebView → SettingsSidebar { selected_pane }
  main WebView    → SettingsContent { selected_pane }
```

- `GuiScene` 与选中的 settings pane 是窗口级 UI 状态，由 Rust scene router 统一切换。
- session list、session status、settings navigation 只渲染在 sidebar WebView。
- chat、composer、permission UI 与 settings form 只渲染在 main WebView。
- Main 的 chat、composer、permission 等 stateful roots 只初始化一次。进入 Settings 时只切换 hidden/inert 状态，不替换、清空或重新挂载这些 roots。
- session draft、selection/caret、scroll、展开状态和 permission UI progress 继续按现有 session-scoped UI memory 保存。
- scene 切换发生在 IME composition 中时，先保留 Main scene 到 `compositionend`，再执行待处理切换；不强制提交或丢弃 preedit text。
- 返回 Main view 后恢复切换前的 focus owner 与 selection；目标节点不存在时明确聚焦 composer host，不静默聚焦 body。
- 返回 Main view 时恢复原来的 active session 与 UI 状态。

### 4. IPC 与事件路由

新增窗口级 scene IPC，不修改现有业务 commands：

| Interface | Direction | Payload | Purpose |
| --- | --- | --- | --- |
| `set_gui_scene` | WebView 到 Rust | `{ scene, selected_pane, expected_revision }` | 请求切换 Main/Settings 或 settings pane |
| `gui_webview_ready` | WebView 到 Rust | `{ webview, last_revision }` | 声明 WebView ready 并请求最新 snapshot |
| `gui-scene-snapshot` | Rust 到两个 WebView | `{ revision, scene, selected_pane }` | 发布窗口 scene 的最终状态 |

Rust scene router 串行处理 intent，每次成功切换递增 revision。两个 WebView 只应用高于本地 revision 的 snapshot，丢弃重复或过期 revision。两个 WebView 独立渲染，因此协议保证最终一致和旧状态不可覆盖新状态，不承诺跨 WebView 的原子帧或零瞬时混合帧。

任一 WebView 未 ready 时 scene router 保留最新完整 snapshot；ready 后直接发送该 snapshot，不重放缺失的 scene 增量。`expected_revision` 过期时 Rust 返回最新 snapshot，由请求端重新判断用户 intent。

事件边界如下：

| Event or snapshot | Producer | Target | Trigger |
| --- | --- | --- | --- |
| `gui-scene-snapshot` | scene router | `main`, `sidebar` | scene/pane 变化、WebView ready |
| `ui-state` | active session runtime | `main` | active session snapshot/stream update |
| `tool-event` | agent event forwarder | `main` | tool lifecycle |
| `permission-request` | permission listener | `main` | 新审批请求 |
| `sidebar-state` | GUI runtime | `sidebar` | initial ready；new/switch/rename/delete session；session status、git、quota 变化 |
| `theme-state` | settings runtime | `main`, `sidebar`, native host | initial ready、theme 或 translucent sidebar 变化 |
| `error` | command/runtime owner | 发起操作的 WebView；无法确定时发 `main` | 操作失败 |
| `notification` | command/runtime owner | 发起操作的 WebView；全局通知显式发两者 | 可见通知 |

`sidebar-state` 是独立完整 payload，至少包含 session list、active session id、session activity/status、git summary、quota summary 和 sidebar actions 状态；不引用 main WebView 的可变 JavaScript 对象。sidebar WebView 不订阅 message streaming、tool output 或 permission request，main WebView 不维护 session list 或 settings navigation 的第二份 DOM/state。

### 5. 原生窗口行为

- sidebar item 使用系统 sidebar behavior，并允许 collapse。
- 初始宽度保持当前视觉范围，目标约 240–320 px；具体 min/max 通过 `NSSplitViewItem` 约束，不再使用 CSS `clamp()` 控制 pane 宽度。
- split view 使用稳定 `autosaveName` 保存 divider position；恢复失败时使用明确初始宽度。
- 窄窗口折叠由 AppKit constraints 和 sidebar behavior 决定，移除固定 `window.innerWidth <= 1100` 判断。
- fullscreen、sidebar overlay 和 divider 由 split controller 管理；frontend 不再同步 `native-titlebar-offset`、`sidebar-edge-visible` 或 chrome background boundary。
- 保留原生 traffic lights、窗口拖动、双击 zoom 和有意义的窗口 title。

split controller 由现有 NSWindow content controller hierarchy 强引用；sidebar/main child controllers 由 split items 强引用；Tauri manager 继续持有 WebView handles。NativeSplitHost 通过已安装 controller 的稳定引用执行 toggle，不通过全局裸指针查找。安装、toggle、状态读取和 teardown 全部限制在 main thread。

titlebar 安装发生在 native split 安装之后。现有 sidebar button 改以 split controller 为 action target；drag view 挂到稳定的 window content/titlebar hierarchy，不再依赖旧 WebView parent。移除旧的整窗 sidebar material；fullscreen 只调整 titlebar drag/toggle 可见性，不接管 pane frame。窗口关闭时先解除 notification observers 和 action target，再拆除 child controllers/WebViews。

sidebar surface contract：sidebar WebView 背景保持透明。`translucentSidebar=true` 时 NativeSplitHost 使用系统 sidebar material；关闭时使用当前主题的 opaque sidebar background。`theme-state` 同时更新 native backing 与两个 WebView，native backing 应先更新，再发布同 revision 的 theme snapshot，避免闪烁。

### 6. Frontend 迁移边界

- 从现有 `index.html`/`app.js` 中提取 sidebar scene 与 main scene 的入口；共享 theme token、IPC wrapper 和纯渲染 helper，避免复制逻辑。
- Main sidebar 与 Settings sidebar 是同一 sidebar WebView 中的两个 scene，不再是两个并列 sidebar DOM 容器。
- Main content 与 Settings content 是同一 main WebView 中的两个 scene。
- 移除仅用于模拟 split 的 CSS/JS：grid column、sidebar overlay transform、edge reveal、双份 collapsed class 和 chrome boundary geometry sync。
- scene renderer 只能切换预先创建的 root 的 visibility/inert 状态，不能用 `innerHTML` 或节点替换重建 Main stateful roots。
- 不改变现有组件视觉风格；若实现发现必须修改 `docs/gui/prototype/`，先取得用户明确同意。

### 7. 文档与代码追踪

实现时同步更新：

- `docs/gui/ARCHITECTURE.md`：一个 NSWindow、一个 native split host、两个 WebView 的运行结构。
- `docs/gui/TERMINOLOGY.md`：native pane、sidebar scene、main content scene、targeted IPC 的边界。
- `docs/gui/FRONTEND_TERMINOLOGY.md`：删除“两个独立 sidebar 容器”的当前实现描述，更新 DOM/source anchors。
- `crates/rozsa-gui/src/native_titlebar.rs` 与新增 native split bridge 的文件头：互相链接相关文档并描述 ownership/lifecycle。

## Acceptance

- SPEC-AC-001: macOS 运行时 view hierarchy 中存在一个 `NSSplitViewController`，包含一个 sidebar item 和一个 main item；frontend 不再用 CSS Grid 决定两 pane 的宽度。
- SPEC-AC-002: 打开/关闭 Settings 时两个原生 split item 与两个 WebView 实例均保持同一 identity，仅 scene 内容变化。
- SPEC-AC-003: 同一个原生 sidebar toggle 可在 Main 与 Settings 中折叠/展开 sidebar；divider 可拖动，宽度可持久恢复，窄窗口与 fullscreen 使用 AppKit sidebar 行为。
- SPEC-AC-004: Main → Settings → Main 后，active session、draft、caret/selection、scroll、展开状态和未决 permission UI 状态不丢失。
- SPEC-AC-005: Settings pane 切换后两个 WebView 最终应用同一最新 revision；重复或旧 revision 不可覆盖新状态。WebView 未 ready 后恢复时直接得到最新完整 snapshot。
- SPEC-AC-006: message/tool/permission 流只进入 main WebView；session/status 与 settings navigation snapshot 只进入 sidebar WebView；没有由双 WebView 监听造成的重复处理。
- SPEC-AC-007: traffic lights、窗口拖动、双击 zoom、resize、进入/退出 fullscreen 和关闭时拒绝 pending approvals 的行为无回归。
- SPEC-AC-008: 非 macOS GUI 保持现有 sidebar、main、Settings 和折叠功能。
- SPEC-AC-009: 相关架构、术语和源文件头完成双向更新，不再描述单 WebView 双 CSS sidebar 为当前 macOS 实现。
- SPEC-AC-010: AppKit 是唯一 pane layout owner；连续 resize 后两个 WebView parent 与 frame 正确，且无 Tauri bounds 写入覆盖 Auto Layout。
- SPEC-AC-011: `translucentSidebar` 开关和 theme 实时切换同时更新 native backing 与两个 WebView，透明和 opaque 两种 sidebar surface 均符合当前主题。
- SPEC-AC-012: workspace Tauri dependency 固定为 exact `2.11.5` 并启用 `unstable`；lockfile 与验证记录一致。

## Risks

- Tauri `add_child` 是 unstable API，Tauri minor update 可能改变接口或内部 WebView ownership。项目应固定并记录已验证的 Tauri minor version。
- 重新挂载 Wry 管理的 `WKWebView` 可能影响 resize、focus、IME、drag/drop、devtools 或销毁顺序。技术验证必须先覆盖这些生命周期行为。
- app-global event 在两个 WebView 中会重复消费。迁移前必须先建立 targeted event/snapshot 边界。
- Settings 切换若通过 reload 实现会破坏 main session UI memory；scene 必须在持久 DOM 上切换。
- AppKit 对象不能进入跨线程 Tauri state；ownership 必须留在主线程 view/controller hierarchy。
- 两个 WebView 无法实现真正原子渲染。revision 协议只保证最终一致；短暂跨 scene 混合属于可接受的显示过渡，不得导致可交互的错误操作目标。

## Test Plan

- 技术验证：最小 NativeSplitHost 启动两个本地 WebView，验证 parent/frame ownership、连续 resize、拖动 divider、collapse、fullscreen、focus、中文 IME、文件拖放、devtools、安装失败清理和正常关闭。
- Rust：为 scene intent、expected revision、过期 revision 丢弃、target routing、sidebar snapshot 触发和 WebView ready 恢复增加纯逻辑测试。
- Frontend：为 Main/Settings scene 切换、stateful root identity、focus/selection 恢复、IME 延迟切换、theme surface 和单一 event consumer 增加定向测试。
- IPC contract：逐项验证事件矩阵的 producer、target、payload 与触发时机；确认无 main-only event 被 sidebar 重复消费。
- Dependency：验证 exact Tauri/Wry lockfile 版本、`unstable` feature 和记录的已验证版本一致。
- 项目检查：运行 `cargo check -p rozsa-gui`，再运行受影响的 `rozsa-gui` targeted tests；最终运行 `cargo test -p rozsa-gui`。
- 真实应用：在 macOS 前台 `.app` 中验证普通窗口、窄窗口、最大化与 fullscreen；分别覆盖 sidebar 展开和折叠状态。
- 非 macOS：至少完成条件编译检查；能使用对应环境时运行 GUI fallback smoke test。

## Self Check

- [x] Goal is clear
- [x] Acceptance criteria are testable
- [x] Matches current quick mode
