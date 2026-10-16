# 共享 UI 样式

原型与 Tauri 宿主共用本目录的生产规则。组件设计以新版原型为依据；原型路径只转发，不复制规则。

| 层级 | 内容 |
| --- | --- |
| `foundations/` | tokens、reset、base、motion |
| `primitives/` | actions；forms 是空边界，不加入加载图 |
| `composites/` | Composer、popover、permission/question、feedback |
| `features/` | Sidebar、Conversation、thinking、tools、Queue、Steering、Settings、Appearance、Dev Flow |
| `layouts/` | application/sidebar shell 与响应式几何 |
| `entries/` | main/sidebar 的共同入口顺序 |

`entries/main.css` 提供 foundations、layouts、Settings、Appearance 与 feedback；`entries/sidebar.css` 复用该入口。宿主在组件装配位置加入其余 feature、actions 与末尾 motion，保留有效 cascade。完整 owner 与顺序见 `source-order.json` 和 [原型入口](../../../prototype/styles/README.md)。

响应式规则由共享 viewport 绑定提供 `--ui-vw` 与 `data-ui-max-width`；native split 使用全窗口宽度，组件几何仍在各自 owner。Gallery 专属展示布局属于原型；runtime 的原生背景/分栏适配属于 `styles/legacy/native-host.css`。

新增规则进入实际 owner。入口只装配，不写规则、不循环 import、不建立跨组件兜底文件。设计缺口中保留的旧运行规则不计入共享完成范围，见 [DESIGN_GAPS](../../../prototype/docs/DESIGN_GAPS.md)。

## 相关文档

- [组件目录](../../../docs/gui/FRONTEND_COMPONENTS.md)
- [GUI 架构](../../../docs/gui/ARCHITECTURE.md)
