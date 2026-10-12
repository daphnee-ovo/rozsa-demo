# 原型组件目录

原型与 Tauri 使用同一份 `crates/rozsa-gui/frontend/ui/`。组件的 template、CSS、renderer、flow 及特殊化关系详见 [共享组件目录](../../docs/gui/FRONTEND_COMPONENTS.md)，职责约束见 [前端界面规范](../../docs/gui/FRONTEND_INTERFACE_STANDARD.md)。

## 原型装配

- `templates/` 的 `data-ui-template-src` 指向共享模板，不保存稳定结构副本。
- `styles/` 对共同规则只做 import 转发；完整顺序见 [样式说明](../styles/README.md)。
- `scripts/core/shell.js` 建立 mock host，装配 Sidebar、Conversation、Settings、Popover、Notification、Dev Flow、Composer 与 interruption。
- `scripts/components/` 和 `scripts/features/` 接入共享实现及演示数据；跨组件场景时序由 `scripts/flows/` 协调。
- `scripts/adapters/mock-adapter.js` 读取 `mock/`，不决定组件视觉。

Composer 由 composite 与 input primitive 组成；input 只挂 children，Queue、Steering、`#msgInput` 与 toolbar 保持同一 wrapper。PermissionRequest 与 AskUserQuestion 共用替换式面板，保留各自选择、Hint/Other 和校验语义。ToolCall 外壳组合各类 evidence 与 diff；通知统一位于右上角。

## Gallery

`gallery/visual-direction.html`、CSS 与 module script 拥有展示 topbar、说明及 Empty/Active/Appearance 切换。共享 token 和共同控件引用共享 UI；展示专属布局不属于 runtime application shell。

## 覆盖与维护

11 个 scene 的已验证视觉与操作继续保留。产品缺失场景不因为原型中存在相邻组件就视为覆盖；以 [UI_COVERAGE](./UI_COVERAGE.md) 和 [DESIGN_GAPS](./DESIGN_GAPS.md) 为准。修改共享组件须沿全部使用方验证，不只检查当前 scene。
