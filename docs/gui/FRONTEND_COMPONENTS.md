# Rózsa 共享前端组件

旧 Tauri 组件由 `crates/rozsa-gui/frontend/ui/` 自行维护。新 Electron 与原型的共享真源位于 `desktop/renderer/`，两套工程各自拥有模板、CSS、组件、测试和入口。层级、状态归属与新增方法见 [前端界面规范](./FRONTEND_INTERFACE_STANDARD.md)。

下表路径均相对于 `frontend/ui/`。稳定结构由 template 或 renderer 唯一拥有，数据数量决定的重复内容由 feature renderer 生成。

| 组件 | 稳定结构 | CSS | 行为与组合 |
| --- | --- | --- | --- |
| 基础输入/动作 | `templates/primitives/{input,action}.html` | `styles/primitives/actions.css`，输入视觉由 Composer owner 提供 | 组合控件保留各自键盘语义 |
| Sidebar | `templates/features/sidebar.html` | `styles/features/sidebar.css`、`sidebar-window.css`、`layouts/sidebar-shell.css` | `scripts/features/sidebar.js`；session、quota、Settings navigation 与 Dev Flow 摘要 |
| Conversation | `templates/features/conversation.html`、`scripts/features/conversation.js` | `styles/features/conversation.css`、`thinking.css` | message frame、empty state、thinking；产品 streaming/error 缺口仍受阻 |
| Composer | `templates/composites/composer.html`、`templates/primitives/input.html` | `styles/composites/composer.css` | `scripts/components/composer.js`、`scripts/flows/conversation.js`；输入、toolbar、发送意图 |
| Queue / Steering | Composer 的 `queuedMessages`、`steeringConversation` slot | `styles/features/queue.css`、`steering.css` | 复用 Composer；原型演示保留，未覆盖的产品主动操作暂停 |
| Popover | `templates/composites/popover.html` | `styles/composites/popover.css` | `scripts/components/popover.js`；宿主提供选项与保存操作 |
| Permission / Question | `templates/composites/interruption-panel.html` | `styles/composites/permission-question.css` | `scripts/features/permissions.js`、`components/interruption-panel.js`、`flows/composer-interruption.js` |
| ToolCall / evidence | `templates/composites/tool-call.html`、`scripts/features/tools.js` | `styles/features/tools.css` | `scripts/components/tool-call.js`；公共展开外壳组合 Read、Edit、Bash、Question、diff 等内容 |
| Settings | `templates/features/settings.html`、`composites/settings-field.html` | `styles/features/settings.css`、`appearance.css` | `scripts/features/settings.js`；Appearance、Permissions 与 Dev Flow 已覆盖内容 |
| Notification | `templates/composites/notification.html`、`scripts/features/notifications.js` | `styles/composites/feedback.css` | `scripts/components/notification.js`；右上角 toast、unresolved error tray 与 resolve |
| Dev Flow | `templates/features/dev-flow.html`、`scripts/features/dev-flow.js` | `styles/features/dev-flow*.css` | Settings、sidebar 摘要与只读 detail；done_when/专用空列表状态留空 |
| Application shell | `scripts/core/shell.js` 装配上述模板 | `styles/layouts/app-shell.css` | `core/renderer.js`、`ports.js` 与 `viewport.js`；宿主启动和清理 |

## 数据与状态

共享组件不读取 mock 或 Tauri API。新原型 `prototype/scripts/adapters/mock-adapter.js` 读取演示数据并引用 desktop 组件；旧 Tauri `runtime/adapters/` 转换真实 payload、调用现有 handler/IPC。adapter 不生成稳定 DOM，也不吞错。

Component 管理局部 focus、展开与选择；flow 协调等待、成功、失败、取消与恢复。产品数据仍归 app；session draft、caret、scroll 与 request identity 由产品宿主管理。新组件先定义输入、意图、template/renderer 和 owner，再登记所有使用方并验证。

## 特殊化与缺口

Permission 与 Question 共用替换式面板，但各自保留响应含义、Hint/Other、校验和 request 生命周期。Permission 有 Allow、Trust、Hint、Deny 四个选项；Trust 操作因设计缺口禁用。固定 interruption demo 完成后保持 Composer 隐藏；运行时流程结束恢复 Composer 与焦点。

Gallery 的 topbar、说明与展示布局属于 `prototype/gallery/`，不加载到产品。Prototype 转发文件只引用 desktop owner；scene 配置不复制 shell。原型已修整的 11 个 scene 和 gallery 继续保留。

未覆盖主动入口是空容器与暂停操作；无法安全暂停的被动状态和在途恢复保留原运行实现，不计作迁移完成。完整逐项边界见 [UI_COVERAGE](../../prototype/docs/UI_COVERAGE.md) 和 [DESIGN_GAPS](../../prototype/docs/DESIGN_GAPS.md)，后续迭代补齐设计后再恢复。

## 相关文档

- [GUI 架构](./ARCHITECTURE.md)
- [原型组件与入口](../../prototype/docs/COMPONENTS.md)
- [原型流程](../../prototype/docs/FLOWS.md)
- [共享样式](../../crates/rozsa-gui/frontend/ui/styles/README.md)
