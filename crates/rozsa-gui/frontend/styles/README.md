# Runtime 样式入口

本目录的 `main.css`、`sidebar.css` 装配 [共享 UI 样式](../ui/styles/README.md)。已覆盖组件规则只有一份生产 owner，原型也引用它；生产入口不依赖原型 CSS 或 mock。

`native-host.css` 最后加载，只处理独立 WebView 的 viewport 与背景边界。main 不预留内嵌 sidebar 背景，sidebar 背景由原生容器提供；单 WebView 平台保持共享窗口约束。

本目录尚存的 `components/`、`features/` 和 `utilities.css` 只服务于设计缺口中无法安全暂停的被动状态、在途恢复及错误出口，或处于 inert template 中的受阻内容，不构成第二套已覆盖组件。未覆盖主动操作已暂停。逐项边界见 [DESIGN_GAPS](../../../../prototype/docs/DESIGN_GAPS.md)，不宣称这些区域已经迁移。

新增组件规则进入 `ui/styles/` 最窄 owner；宿主差异只进入平台边界。保持入口 cascade，无循环 import，不向遗留目录新增共享实现。

## 相关文档

- [GUI 架构](../../../../docs/gui/ARCHITECTURE.md)
- [界面规范](../../../../docs/gui/FRONTEND_INTERFACE_STANDARD.md)
- [主题配置](../../../../docs/gui/themes.md)
