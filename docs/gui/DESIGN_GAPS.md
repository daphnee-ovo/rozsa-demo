# GUI 设计缺口表

本文只收录 [`UI_COVERAGE.json`](./UI_COVERAGE.json) 中状态为 `partial` 或 `missing` 的项目。缺口表示新版原型尚未定义 runtime 已有的可见内容，不授权实施者自行补画。当前没有获批的 partial-component 例外。

## Application shell 与 sidebar

### GAP-001：Quota tooltip 与限额变体

- `coverage_id`：`UI-SIDEBAR-003`
- `runtime_trigger`：提供 rate-limit snapshot，显示 sidebar，依次 hover/focus 小时或周限额条。
- `current_runtime`：支持 normal、near-limit、exhausted、unavailable/error、hidden、tooltip open、长 reset 文本和窄宽度。
- `prototype_missing`：`complete-session` 只有 tooltip 容器，未实际覆盖全部限额语义、错误、长内容和窄屏状态。
- `required_prototype`：补充实际打开的 tooltip 及 normal、near-limit/exhausted、unavailable、长内容和窄屏 variant。
- `blocked_scope`：quota tooltip 与语义化限额变体；基本 quota bar 不受阻断。

## Conversation、Composer、Queue 与 Steering

### GAP-002：消息状态矩阵

- `coverage_id`：`UI-CONV-002`
- `runtime_trigger`：渲染 user/assistant/system、streaming partial、continuation 与 message-level error。
- `current_runtime`：依据消息类型、streaming flag 和 error 分支显示不同内容、cursor 与错误块。
- `prototype_missing`：`complete-session` 没有覆盖完整 streaming、continuation、error 和极端长内容组合。
- `required_prototype`：补充使用最终公共 message 实现的状态 gallery 或 scene variant。
- `blocked_scope`：未在原型实际呈现的 message 状态；现有 complete-session fixture 可继续整理。

### GAP-004：Slash-command autocomplete

- `coverage_id`：`UI-COMPOSER-002`
- `runtime_trigger`：在 Composer 输入 `/`，使用 Up/Down/Enter/Escape，再用 IME、长列表、空列表和错误重复。
- `current_runtime`：支持 suggestion row、highlight、keyboard/hover selection、overflow、empty/stale/error 与 IME。
- `prototype_missing`：原型只有插入 slash prefix 的动作，没有 suggestion renderer。
- `required_prototype`：补充完整 row、scroll、focus、empty/error 和窄屏状态。
- `blocked_scope`：autocomplete 组件与交互；Composer shell 不受阻断。

### GAP-005：附件、文件引用与拖放反馈

- `coverage_id`：`UI-COMPOSER-003`
- `runtime_trigger`：附加文件/目录并把支持或不支持的路径拖入窗口。
- `current_runtime`：显示单个/多个引用、长或无效路径、drag enter/leave/drop、disabled directory 与 range highlight。
- `prototype_missing`：file/directory handler 是显式 no-op，没有应用内 reference token 或 drag feedback。
- `required_prototype`：补充 attachment/reference token 和 drag feedback 场景。
- `blocked_scope`：应用内附件、引用与拖放视觉；操作系统 picker chrome 不在本缺口范围。

### GAP-006：Model picker

- `coverage_id`：`UI-COMPOSER-004`
- `runtime_trigger`：在 populated、empty、刷新失败 registry 下打开 picker 并切换 model。
- `current_runtime`：支持 selected、hover/focus、scroll、empty、refresh、unsupported thinking 与 provider/switch error。
- `prototype_missing`：只有静态 model badge，`showModelPicker` 是 no-op。
- `required_prototype`：补充 model picker 的正常、空、错误、滚动和窄屏状态。
- `blocked_scope`：model picker；Composer 静态 badge 不受阻断。

### GAP-007：Queue、Steering、Fork 与 Subagent

- `coverage_id`：`UI-FLOW-001`
- `runtime_trigger`：turn 运行时发送 queue/steering，打开 fork/subagent，注入 empty/error 后 abort。
- `current_runtime`：支持 populated/empty、queue/steer、remove/action、active/finished/error、fork success/failure、aborting 和长文本。
- `prototype_missing`：`complete-session` 的静态 populated fixture 未覆盖四类面板的空白、错误和完整操作。
- `required_prototype`：补充四类面板的可操作、空白、错误状态及 running send/abort control。
- `blocked_scope`：Queue、Steering、Fork、Subagent 与 running composer 跨组件流程。

## Tool call、evidence、diff 与 Markdown

### GAP-003A：Read、Edit 与 Bash tool lifecycle

- `coverage_id`：`UI-TOOL-001`
- `runtime_trigger`：分别执行 Read/Edit/Bash 的成功、失败、取消和 missing-result，在 desktop/narrow 展开收起。
- `current_runtime`：支持 pending/running/success/error/cancelled、collapsed/expanded、empty/long output 和连续调用。
- `prototype_missing`：`tool-calls` 未实际呈现每个 tool family 的完整生命周期和极端内容。
- `required_prototype`：补充每类 tool 的 lifecycle、long/empty/error output 和窄屏 variant。
- `blocked_scope`：未被实际呈现的 Read/Edit/Bash tool 变体与共用 lifecycle。

### GAP-003B：Subagent、Question 与 generic tool lifecycle

- `coverage_id`：`UI-TOOL-002`
- `runtime_trigger`：执行 Subagent、AskUserQuestion 与 generic tool 的成功、失败、取消和 missing-result。
- `current_runtime`：每类有专用或 generic evidence renderer，并支持生命周期与展开状态。
- `prototype_missing`：只有少量静态实例，没有完整 lifecycle、empty/long/error output。
- `required_prototype`：补充每个特殊 tool family 的完整状态矩阵。
- `blocked_scope`：未覆盖的 specialized/generic tool evidence 及其共用 lifecycle。

### GAP-002B：Rich Markdown 状态矩阵

- `coverage_id`：`UI-MARKDOWN-001`
- `runtime_trigger`：在 desktop/narrow 渲染 heading、nested list、blockquote、link、inline code、table、blocked image、长 CJK 和 code overflow。
- `current_runtime`：`renderMarkdown`/`renderTable` 对各 Markdown 构造及 overflow 产生可见结构。
- `prototype_missing`：现有消息未覆盖完整 Markdown 构造与极端长度。
- `required_prototype`：补充使用最终 message-content 实现的 Markdown 状态 gallery。
- `blocked_scope`：未被原型实际展示的 rich Markdown family。

## PermissionRequest 与 AskUserQuestion

### GAP-008A：Permission main/hint 未覆盖状态

- `coverage_id`：`UI-PERMISSION-001`
- `runtime_trigger`：收到 permission-request，进入 hint，操作长命令、键盘选择、提交并触发失败。
- `current_runtime`：支持 main、hint、selected、长命令展开/收起、responding、failure 与 session-specific UI state。
- `prototype_missing`：主页面和 hint 只覆盖部分正常路径，缺少响应失败、长命令、键盘和窄屏组合。
- `required_prototype`：补充上述 main/hint variant 与可重复操作步骤。
- `blocked_scope`：未覆盖的 permission main/hint 状态；不得由其他面板推导视觉。

### GAP-008B：Permission trust 与 approval lifecycle

- `coverage_id`：`UI-PERMISSION-002`
- `runtime_trigger`：进入 trust，排队第二个 request，切换 session，并强制 response failure/cancel。
- `current_runtime`：支持 trust level、queued/responding、failure、cancelled/stale 与 focus/state 恢复。
- `prototype_missing`：trust handler 只重新打开主 demo，没有 trust renderer 或 queue/error/cancel state。
- `required_prototype`：补充 trust-level、queued/responding/error/cancelled 和窄屏场景。
- `blocked_scope`：permission trust、队列与 approval lifecycle。

## Settings、Appearance 与 permission settings

### GAP-009：填充后的 General settings

- `coverage_id`：`UI-SETTINGS-002`
- `runtime_trigger`：用正常、缺少 model、loading 和 update failure 打开 General。
- `current_runtime`：显示 model/provider/context、transport/retry、dependent/disabled、validation 与 save error。
- `prototype_missing`：只有隐藏 markup，没有激活并填充 General 的 scene fixture。
- `required_prototype`：补充 populated、loading、empty、dependent/error、长文本和窄屏状态。
- `blocked_scope`：General settings family。

### GAP-010：Skills 与 Tools capability settings

- `coverage_id`：`UI-SETTINGS-003`
- `runtime_trigger`：在 global/project scope 切换并 reset inherited item，注入 update error。
- `current_runtime`：支持 populated/empty、inherited/local override、enabled/disabled、reset、loading/error。
- `prototype_missing`：capability renderer 是显式空函数。
- `required_prototype`：补充填充后的 Skills/Tools 场景和上述状态。
- `blocked_scope`：capability row、scope 与 update/reset flow。

### GAP-011：Keyboard shortcut settings

- `coverage_id`：`UI-SETTINGS-004`
- `runtime_trigger`：打开 Keyboard shortcuts，搜索、修改/reset，并触发 invalid/conflict。
- `current_runtime`：支持 match/no-match、editing/listening、fixed/customizable、reset、conflict/error 与 keyboard focus。
- `prototype_missing`：只有对预存 row 的筛选逻辑，没有任何 row fixture。
- `required_prototype`：补充真实 row 和全部搜索、编辑、冲突、错误状态。
- `blocked_scope`：shortcut list、search 与 edit/error flow。

### GAP-012：Permission settings 与 rule editor

- `coverage_id`：`UI-SETTINGS-005`
- `runtime_trigger`：用 inherited/local rule 打开 Permissions，展开、排序、删除、恢复并创建 glob/regex/path rule。
- `current_runtime`：支持 Deny/Ask/Allow、empty、inherited/local、collapsed/expanded、drag/delete/reset、editor/combobox 与 save error。
- `prototype_missing`：`permissions` scene 只有空 shell，renderer 是显式空函数。
- `required_prototype`：补充填充且可操作的 permission-settings scene。
- `blocked_scope`：permission rule list、row、editor 与更新流程。

## Notification 与错误反馈

### GAP-013：跨组件异常状态

- `coverage_id`：`UI-FEEDBACK-001`
- `runtime_trigger`：向各 surface 注入 invoke rejection、malformed/empty/stale/long payload，并在窄屏与 reduced motion 下操作。
- `current_runtime`：各 renderer 分别处理 loading、empty、disabled、recoverable/fatal error、retry/cancel、overflow、focus retention 与 session switch。
- `prototype_missing`：多个组件只有正常状态；不能用一个通用 fallback 证明各组件异常状态。
- `required_prototype`：为缺少异常状态的组件分别补充明确 variant。
- `blocked_scope`：任何没有具体原型和专项证据的异常状态；已有明确原型的状态不受阻断。

## Related Docs

- [`GUI 界面对照表`](./UI_COVERAGE.md)
- [`机器可读界面对照表`](./UI_COVERAGE.json)
- [`SPEC：GUI 原型与真实前端组件化重构`](../../.dev-doc/refactor/ui-architecture/SPEC.md)
