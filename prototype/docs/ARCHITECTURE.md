# 原型架构

根目录 `prototype/` 保存唯一可编辑的原型场景、gallery 和 mock。已覆盖组件的模板、CSS 与交互唯一位于 `crates/rozsa-gui/frontend/ui/`，原型与真实 GUI 引用同一份实现。设计概念见 [DESIGN.md](../../DESIGN.md)，生产职责见 [GUI 架构](../../docs/gui/ARCHITECTURE.md)。

## 入口与加载

统一入口 `prototype/index.html`：

- `?scene=<id>`：registry 校验 ID，shell 装配共享模板，mock adapter 提供初始数据。
- `?gallery=visual-direction`：加载独立 gallery 的 HTML、CSS 和 module script。
- 无参数：`empty-session`。
- 未知 ID：装配前明确失败，不 fallback。

| 位置 | 职责 |
| --- | --- |
| `scenarios/registry.js`、`scenes/` | 11 个 application scene 的 ID、初始状态、mock 与操作配置 |
| `scripts/core/` | 原型启动、scene 选择与共享 shell/template loader 接入 |
| `scripts/components/`、`scripts/features/` | 接入共享组件与原型演示能力，不保存第二套组件 |
| `scripts/flows/` | 场景时序和 mock 操作协调 |
| `scripts/adapters/mock-adapter.js`、`mock/` | 演示数据与可重复结果 |
| `templates/` | `data-ui-template-src` 转发到共享 template |
| `styles/` | 有序 import 与共享 CSS 转发；gallery 专属规则保留在原型 |
| `gallery/` | 视觉参考的 topbar、说明、布局与视图切换 |
| `docs/` | 场景、组件、流程、数据、覆盖、缺口与验证说明 |

Application scene 不复制 shell，不动态加载 scene stylesheet。input template 只挂 children，保持 Queue、Steering、输入与 toolbar 的既有 DOM 关系。CSS source order 由 [样式入口](../styles/README.md) 说明。

## 数据与新增方法

Mock 只由原型 adapter 读取；生产 runtime、Rust、Tauri 配置与 bundle 对它零依赖。新增 scene 先登记 registry、mock 与初始状态，复用共享 UI，然后更新 [场景](./SCENARIOS.md)、[覆盖](./UI_COVERAGE.md) 和验证材料。新增组件在共享 owner 实现，通过 mock/Tauri adapter 接入；不在 scene 建组件副本。

原型没有覆盖的产品状态按 [设计缺口](./DESIGN_GAPS.md) 留待后续迭代，当前不推导新视觉。
