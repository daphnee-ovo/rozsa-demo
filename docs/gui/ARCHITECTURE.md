# Rózsa GUI 架构

新版原型与 Tauri 产品对已覆盖组件引用同一份生产 UI。原型使用 mock adapter，产品使用 Tauri adapter；设计概念见 [DESIGN.md](../../DESIGN.md)，场景和设计参考见根目录 [prototype/](../../prototype/)。

## 职责与依赖

`rozsa-cli / rozsa-gui → rozsa-app → rozsa-core → rozsa-model`。app 拥有 session、权限、设置与 Dev Flow 产品运行时；core 负责 agent loop 与工具；model 负责 provider、流式响应与凭据。GUI 不复制产品状态。

| 位置 | 职责 |
| --- | --- |
| `frontend/ui/styles/` | 唯一共享 CSS，按 foundations、primitives、composites、features、layouts、entries 分层 |
| `frontend/ui/templates/` | 稳定 DOM 骨架，不读取产品或 mock 数据 |
| `frontend/ui/scripts/` | 共享 renderer、组件交互、feature 与跨组件 flow |
| `frontend/runtime/adapters/` | Tauri request、订阅、数据转换与宿主能力 |
| `frontend/runtime/entries/` | main/sidebar 装配入口 |
| `frontend/app.js`、`sidebar.js`、`gui_shared.js` | 现有产品接线、session UI state 与 WebView revision 契约 |
| `prototype/` | 场景、gallery、mock adapter、演示数据与共享实现的路径转发 |

组件管理 focus、展开与选择；flow 协调提交、等待、错误和恢复；adapter 连接数据与操作，不选择 DOM/CSS。draft、caret、scroll 和随 session 恢复的展开状态由产品宿主管理。详见 [前端界面规范](./FRONTEND_INTERFACE_STANDARD.md) 与 [组件目录](./FRONTEND_COMPONENTS.md)。

## 双宿主启动

原型 `prototype/index.html` 校验 scene/gallery ID。Application scene 的 shell 通过转发模板装配共享 UI，由 mock adapter 提供数据与演示结果。Gallery 有独立展示布局，不代表产品页面。

Tauri 的 `index.html` 和 `sidebar.html` 加载 `runtime/entries/main.js`、`sidebar.js`。main 在 `RozsaUIReady` 完成共享模板/组件装配后接入产品事件；sidebar 装配同一共享 Sidebar/Dev Flow renderer。两种宿主都使用共享模板加载机制。生产包只包含 `frontend/`，不依赖原型 scene 或 mock。

`ui/scripts/core/ports.js` 统一 request、订阅清理与单调 revision 提交。异常保持拒绝，dispose 撤销订阅。AbortSignal 取消结果接收，不代表 Tauri 后台操作停止；产品取消仍须调用现有 abort 契约。

## 原生窗口与事件

macOS 使用一个 NSWindow 和持久 NSSplitViewController，sidebar/main 各有一个真实 WKWebView。原生 titlebar、divider、sidebar material 与收起状态由 AppKit host 管理；Main/Settings 切换复用 WebView。两端 readiness handshake 完成后统一显示 split root。

```text
NativeTitlebarHost + NativeSplitHost
  ├─ sidebar WebView：MainSidebar / SettingsSidebar
  └─ main WebView：MainContent / SettingsContent
        ↕ targeted events / invoke
  scene_router + commands + events → app runtime
```

非 macOS 使用单 WebView 的共享 CSS Grid 布局。共享响应式规则使用全窗口宽度；macOS adapter 用 outer width / scale factor 提供 `--ui-vw` 与 `data-ui-max-width`，避免把分栏宽度当作整个窗口。

| 事件 | 消费位置 |
| --- | --- |
| `gui-scene-snapshot` | ready 的 main/sidebar，完整 scene/revision；旧 revision 丢弃 |
| `ui-state`、`tool-event` | main 的真实消息与工具状态 |
| `permission-request`、`question-request` | main，按 session/request identity 隔离 |
| `sidebar-state` | sidebar 完整状态 snapshot |
| `theme-state` | 原生 backing 先更新，再发布同 revision 完整主题 snapshot |

权限与 question 响应沿现有 handler/IPC 返回 app。PermissionRequest 显示 Allow、Trust、Hint、Deny 四个选项；Trust 的未覆盖操作暂停。Question 保留多题、单/多选、Other 与校验。pending request 生命周期见 [运行时术语](./TERMINOLOGY.md)。

## 覆盖与缺口

[UI_COVERAGE.json](../../prototype/docs/UI_COVERAGE.json) 记录设计覆盖、共享 owner、真实触发方式和使用方；[DESIGN_GAPS.md](../../prototype/docs/DESIGN_GAPS.md) 记录未覆盖状态及恢复条件。

已覆盖的 Sidebar、Conversation frame、Composer、工具内容、替换式面板、Settings、通知与 Dev Flow 使用共享实现。未覆盖主动入口保留空容器并暂停操作，后续迭代逐项补齐设计。无法安全暂停的被动消息状态、在途恢复和错误出口保留原运行实现，标为受阻；不计入已迁移区域，不作为共享 UI 的备用实现。

所有通知位于右上角。错误 toast 到期后进入 unresolved error tray；恢复成功明确 resolve。普通 ready/success 不主动打扰。Dev Flow 由项目级 app runtime 提供状态，GUI 不读取 `.dev-doc` 或解析原始 REST/SSE。

## 开发与验证

仓库根目录 `./run.sh` 启动 staged debug app，`./run.sh --prepare-only` 验证 bundle。原型 HTTP root 必须是仓库根目录，才能加载共享 UI。

小功能用所属语言测试；JavaScript 位于模块 `js-tests/`。[CONTRIBUTING.md](../../CONTRIBUTING.md) 说明统一编排。契约测试不等于视觉验收，prototype/app 的截图、DOM、computed style 与交互须按相同条件成对比较。[Native split 验证](./NATIVE_SPLIT_VALIDATION.md) 区分实际验证与平台限制；Linux/Windows 真实视觉保持 pending。

## 相关文档

- [界面使用指南](./UI_USAGE_GUIDELINES.md)
- [前端组件目录](./FRONTEND_COMPONENTS.md)
- [前端术语](./FRONTEND_TERMINOLOGY.md)
- [主题配置](./themes.md)
- [Dev Flow 集成](./DEV_FLOW_INTEGRATION.md)
- [共享样式](../../crates/rozsa-gui/frontend/ui/styles/README.md)
- [原型架构](../../prototype/docs/ARCHITECTURE.md)
