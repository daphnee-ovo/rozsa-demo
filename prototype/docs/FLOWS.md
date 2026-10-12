# 跨组件流程

## Scene 启动

1. `index.html` 解析 query 并由 registry 校验 ID。
2. `shell.js` 按固定列表加载 templates。
3. `renderer.js` 将 fragment 挂载到 application shell slots。
4. `scene-loader.js` 读取 scene 配置，adapter 加载对应 mock。
5. feature/component 依据初始 state 渲染，`scenario-flow.js` 发布 scene-ready。

未知 ID 在第 1 步失败，不加载 shell，也不返回默认页面。

## Composer interruption

`interruption-panel.js` 接收 permission/question 的选择、键盘和提交意图；`composer-interruption.js` 负责暂时替换 Composer、禁用冲突操作、等待 mock 响应，并在成功、失败、取消和 restore 路径中恢复对应宿主的 focus 与可见状态。固定 interruption demo 保持 Composer 隐藏；runtime 完成后恢复 Composer。组件不直接决定 mock 响应，flow 不创建稳定 HTML。

## PermissionRequest

进入 permission → 显示 Allow、Trust、Hint、Deny 四个选项 → 选择或进入 Hint → 提交 → mock 返回 success/error/cancel → 显示结果或保留错误 → 恢复 Composer/焦点。长命令展开、键盘选择和 hint 输入属于组件 UI state。

## AskUserQuestion

进入 question → 渲染选项 → 键盘或点击选择 → 编辑 other（如需要）→ submit/cancel → mock 返回结果 → 成功关闭或错误保留面板 → 恢复 focus。选择值通过明确意图交给 flow，不在 template 中硬编码业务处理。

## Settings、Notification 与 Dev Flow

- Settings feature 根据初始 pane 装配 Appearance、Permissions、Dev Flow 等模块；tab、theme 和控件状态由 settings script 管理。
- Notification component 管理 dismiss、error tray 和可见状态；跨组件取消路径仍由 flow 触发。
- Dev Flow feature 将 runtime mock 转成状态卡、detail 和 dashboard affordance；detail 的打开/关闭不改变 application shell 的公共结构。

产品 Trust 未覆盖操作禁用，其他缺失入口按 [DESIGN_GAPS](./DESIGN_GAPS.md) 暂停；原型 mock 演示不表示产品操作可用。

所有流程都要求覆盖正常、成功、失败、取消和恢复路径。新增流程时同步 UI_COVERAGE 的 behavior/used_by/verification 字段。
