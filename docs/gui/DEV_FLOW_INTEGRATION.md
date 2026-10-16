# Dev Flow GUI 集成

Dev Flow 是可选的项目级集成。app 层发现兼容 `dow`，提供 typed snapshot；共享 UI 显示 Settings、sidebar 状态与只读详情。原型通过 mock adapter 演示同一份组件，产品通过 [runtime adapter](../../desktop/renderer/runtime/adapters/sidebar-dev-flow.js) 接入。

## 发现与设置

共享发现层搜索 Homebrew、npm、Cargo 与 PATH，Settings 可指定一个绝对路径。候选经有时限的 `dow --version` 验证；显式无效路径透明报错，不静默改用其他安装。项目当前 revision 的 STATUS.yaml 可读且 `dow status` 成功后才可用；后续初始化可被检测。

Dev Flow pane 使用共享 typography、hairline、switch、input 与 button。Overview 和 Settings 是独立子模块：Overview 共用布局且各字段承担边框，Settings 保留轻圆角容器。

- 未发现 CLI：安装说明与 Check again。
- Overview：Dashboard availability、可点击 loopback 地址、版本与 Memory Use。
- Settings：可输入/粘贴的 Executable Path、picker，以及 Enable Dev Flow、Show Dev Flow status in sidebar、Show Dashboard button。

路径失焦时保存，picker 使用同一保存命令；清空后恢复自动发现。总开关始终可操作，关闭或 CLI 不可用时禁用两个依赖开关。后端串行写入，前端用单调 intent revision 拒绝迟到响应；成功后立即发布 sidebar snapshot。自动安装与 system-prompt 注入未实现，见 [TODO](../TODO.md)。

## Sidebar 与详情

Status 不增加独立 Dev Flow 标题，只统计未关闭项为 `N Tasks · N Issues`。Claimed row 单行截断，保证 Sessions 最小高度；溢出合为 more N。Hover 延时打开只读详情，离开关闭；click 固定同一 surface。Dashboard 位于 Settings 上方，在总开关或对应偏好关闭时不显示。

已覆盖详情使用共享 renderer，显示 task/issue、元信息与 Files，支持 Close/Escape。done_when disclosure 与专用空列表状态未覆盖，保留空 slot；不伪造完成，也不提供 mutation controls。恢复条件见 [DESIGN_GAPS](../../prototype/docs/DESIGN_GAPS.md)。

## 产品 ownership 与接口

同一 canonical project root/revision 的 session 共用服务与 snapshot。named branch、unborn branch、detached commit 与非 Git 项目保持不同 identity。切换项目保留旧服务供返回复用；无活跃 session 15 分钟后进入回收候选，软预算为 `max(system memory × 5%, 256 MiB)`。活跃/current 服务和可能的外部 dashboard clients 受保护。Memory Use 为服务 child RSS 加项目 registry cache 估算。

Status/sidebar/dashboard 通过 app adapter 和 GUI typed DevFlowRuntime facade。只调用 loopback REST v1 读取 status、tasks、issues；SSE events 只提供 invalidation，再重新获取并原子发布完整验证 snapshot。GUI 不读取任务文件，不解析 raw REST/SSE，不调用 dashboard mutation/document routes。

Dashboard 地址和 sidebar action 共用受限 loopback opener，启动或复用当前项目服务，并在系统浏览器打开。

## Bash 工具摘要

Bash card 是独立的消息内容转换，不消费 status snapshot、不持久化 presentation state，也不通过 app 调用命令。成功且未截断的工具结果可识别：

- Task：create、update、remove、done、reopen。
- Issue：create、update、remove、close、reopen。
- Claim：claim、claim --revoke。

摘要显示 action、resource、短 ID 与可用 title；每个 ID 至多一次有界 detail GET，不用 collection lookup。删除后无法获取 title 时保留 ID 和 Details unavailable。show/list/schema、status/init/doctor/iterate/version 等保持普通 Bash 摘要。

识别支持 -H/--human、已知参数值、input redirection 和正确的 `2>&1`；不能把错误的 `2&>1` 或多命令最终 exit code 当作单项成功证据。

## 错误与验证

所有错误经共享通知中心在右上角显示，6 秒后保留在 unresolved error tray，恢复成功明确 resolve；普通 ready/success 保持安静。

真实 dow 契约测试使用 `tmp/test_env/` 的隔离 Git 项目、明确 cwd 与 9800–9900 端口，并清理自己启动的服务。macOS watcher/FSEvents/SSE 测试需要 host permissions，沙箱内 timeout 不能直接归因于产品。DOM/契约测试不替代真实原型与 app 验证；Linux/Windows 视觉保持 pending。

## 相关文档

- [GUI 架构](./ARCHITECTURE.md)
- [组件目录](./FRONTEND_COMPONENTS.md)
- [运行时术语](./TERMINOLOGY.md)
- [前端术语](./FRONTEND_TERMINOLOGY.md)
- [界面使用指南](./UI_USAGE_GUIDELINES.md)
