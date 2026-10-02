# 共享 UI 样式

原型宿主和 Tauri 宿主共用本目录中的生产规则。`foundations/`、`primitives/` 与 `layouts/` 沿用已验证原型的 selector、declaration、media query 和规则顺序，不读取 mock 或宿主 API。

- `entries/main.css` 装配共同基础前缀：tokens、reset、base、app-shell、sidebar-shell。
- `entries/sidebar.css` 引用同一基础前缀；各宿主在自己的组件装配位置引用 actions 与末尾 motion，保持原型的有效 cascade。
- `primitives/forms.css` 是已有空边界，不加入加载图；Composer 样式仍由对应复合组件负责。
- `source-order.json` 记录本目录当前 owner、内容摘要、共同入口顺序与后置规则。

当前 component/feature CSS 由对应区域任务接入。原型的 scene/gallery 专属布局留在原型宿主；生产入口不 import 原型 CSS。Tauri 入口中的旧组件规则在区域验证通过后清理，不能把它们作为第二套组件继续维护。

新增规则放入实际 owner。公共规则只有一份实现；入口只装配，不写规则、不循环引用、不引入跨组件兜底文件。

## Related Docs

- [原型样式与完整 source order](../../../../../prototype/styles/README.md)
- [GUI 架构](../../../../../docs/gui/ARCHITECTURE.md)
- [本迭代 SPEC](../../../../../.dev-doc/refactor/ui-architecture/SPEC.md)
