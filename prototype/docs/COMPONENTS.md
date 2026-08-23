# 原型组件目录

原型按“基础标准 → 基础控件 → 复合组件 → 功能模块 → 页面布局 → scene/gallery 组合”组织。稳定 DOM、CSS 和 behavior 各有唯一责任位置。

## 公共结构

| 组件/功能 | template | CSS owner | behavior |
| --- | --- | --- | --- |
| Sidebar | `templates/features/sidebar.html` | `styles/features/sidebar.css`、`styles/layouts/sidebar-shell.css` | `scripts/features/sidebar.js` |
| Conversation | `templates/features/conversation.html` | `styles/features/conversation.css` | `scripts/features/conversation.js` |
| Composer | `templates/composites/composer.html`、`templates/primitives/input.html` | `styles/composites/overlays.css`、`styles/primitives/forms.css` | `scripts/components/composer.js` |
| Interruption panel | `templates/composites/interruption-panel.html` | `styles/features/permissions.css`、`styles/composites/overlays.css` | `scripts/components/interruption-panel.js`、`scripts/flows/composer-interruption.js` |
| Notification | `templates/composites/notification.html` | `styles/composites/feedback.css` | `scripts/components/notification.js` |
| Popover | `templates/composites/popover.html` | `styles/composites/overlays.css` | `scripts/components/popover.js` |
| Tool call | `templates/composites/tool-call.html` | `styles/features/tools.css` | `scripts/components/tool-call.js` |
| Settings | `templates/features/settings.html`、`templates/composites/settings-field.html` | `styles/features/settings.css`、`styles/features/appearance.css`、`styles/features/permissions.css` | `scripts/features/settings.js` |
| Dev Flow | `templates/features/dev-flow.html` | `styles/features/dev-flow.css`、`styles/features/dev-flow-detail.css` | `scripts/features/dev-flow.js` |

基础 actions/forms 和全局 tokens 由 `styles/primitives/` 与 `styles/foundations/` 提供。组件脚本只管理 DOM 状态、focus、键盘和明确意图，不读取产品后端。

## 装配与数据

- `scripts/core/template-loader.js` 负责读取模板，`renderer.js` 负责挂载稳定 fragment。
- `scripts/core/scene-loader.js` 根据 registry 取 scene 配置。
- `scripts/adapters/mock-adapter.js` 将 `mock/` 数据转换为组件输入。
- `scripts/flows/scenario-flow.js` 协调 scene-ready 与 application shell 时序。
- `scripts/flows/composer-interruption.js` 协调替换式面板、提交/取消和 focus 恢复。

组件不创建整页 HTML；scene 不复制 shell；adapter 不决定视觉；flow 不吞掉错误。

## Gallery 专属内容

`gallery/visual-direction.html` 拥有 Independent demo topbar、Empty/Active/Appearance 三个视图和视觉研究说明。其稳定视觉规则由 `styles/features/visual-demo.css` 唯一维护，入口只通过 `gallery/visual-direction.css` 导入。tab 切换和 theme toggle 由 `gallery/visual-direction.js` 管理。gallery 专属布局不冒充 application component，但其中与公共实现相同的 token、控件和交互不能再复制一份。
