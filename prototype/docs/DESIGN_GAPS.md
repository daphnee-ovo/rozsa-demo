# GUI 设计缺口表

本文只收录 [`UI_COVERAGE.json`](./UI_COVERAGE.json) 中状态为 `partial` 或 `missing` 的项目。缺口表示新版原型尚未定义 runtime 已有的可见内容，不授权实施者自行补画。`partial` 的已覆盖状态接入共享 UI；未覆盖状态与 `missing` 项保留空容器并暂停相关操作。

## 本次迁移边界

以下处置是对应开发任务的实施要求，不表示当前 GUI 已经停用。逐项入口、容器位置、用户影响和代码依据记录在 JSON 的 `shared_migration`；这里沿用原有 gap ID 说明设计缺口与迁移门槛，不建立第二份缺口清单。

主动入口须在实际操作前暂停，并核对鼠标、键盘、快捷键、slash action、main/sidebar 路由和迟到回调。后台事件或在途请求不能靠隐藏 DOM 暂停：若没有安全回应、退出或透明错误路径，阻断对应区域迁移并报告，不自动允许、拒绝、取消或伪造完成。阻断期间不得宣称该区域已迁移或在 T036 清除其旧实现；旧实现只作为尚未切换的现状，不作为最终共享 UI 的备用入口。

缺失原型和功能恢复在后续迭代逐项处理。`required_prototype` 与 `blocked_scope` 保留用于后续设计；当前任务不以补齐这些缺口为验收条件。原型已有场景与交互继续保持基线。

## Application shell 与 sidebar

### GAP-001：Quota tooltip 与限额变体

- `coverage_id`：`UI-SIDEBAR-003`
- `runtime_trigger`：提供 rate-limit snapshot，显示 sidebar，依次 hover/focus 小时或周限额条。
- `current_runtime`：支持 normal、near-limit、exhausted、unavailable/error、hidden、tooltip open、长 reset 文本和窄宽度。
- `prototype_missing`：`complete-session` 只有 tooltip 容器，未实际覆盖全部限额语义、错误、长内容和窄屏状态。
- `required_prototype`：补充实际打开的 tooltip 及 normal、near-limit/exhausted、unavailable、长内容和窄屏 variant。
- `blocked_scope`：quota tooltip 与语义化限额变体；基本 quota bar 不受阻断。
- `shared_migration`：TASK-T022；空容器：main 的 #quotaTooltip；main/sidebar quota 区域中的未覆盖限额变体 slot。暂停：限额条的 tooltip hover/focus 展示；未覆盖限额变体的可见操作。
- `user_impact`：已覆盖基本 quota bar 保留；未覆盖 tooltip 和限额变体不展示。
- `safety_gate`：限额数据接收可保留；切换前验证被动刷新不会重新挂载旧变体或影响已覆盖栏位。

## Conversation、Composer、Queue 与 Steering

### GAP-002：消息状态矩阵

- `coverage_id`：`UI-CONV-002`
- `runtime_trigger`：渲染 user/assistant/system、streaming partial、continuation 与 message-level error。
- `current_runtime`：依据消息类型、streaming flag 和 error 分支显示不同内容、cursor 与错误块。
- `prototype_missing`：`complete-session` 没有覆盖完整 streaming、continuation、error 和极端长内容组合。
- `required_prototype`：补充使用最终公共 message 实现的状态 gallery 或 scene variant。
- `blocked_scope`：未在原型实际呈现的 message 状态；现有 complete-session fixture 可继续整理。
- `shared_migration`：TASK-T017 / TASK-T038；空容器：#chatMessages 中未覆盖 message 状态的 slot。暂停：未覆盖 message 状态的交互入口。
- `user_impact`：已覆盖消息使用共享 frame；未覆盖 streaming/continuation/error 和极端内容状态的迁移受阻，保留现有真实状态与错误处理，不得在 T036 清除。
- `implementation_status`：blocked-passive-states。`#chatMessages[data-ui-migration-blocked="UI-CONV-002"]` 标明边界；不隐藏在途输出或真实错误，不声明完整消息状态矩阵已迁移。
- `safety_gate`：ui-state 与 streamUpdate 可被动到达；迁移前逐状态验证留空仍保留真实产品状态和必要操作出口。无法安全处理在途输出或真实错误则阻断 conversation 区域迁移。

### GAP-004：Slash-command autocomplete

- `coverage_id`：`UI-COMPOSER-002`
- `runtime_trigger`：在 Composer 输入 `/`，使用 Up/Down/Enter/Escape，再用 IME、长列表、空列表和错误重复。
- `current_runtime`：支持 suggestion row、highlight、keyboard/hover selection、overflow、empty/stale/error 与 IME。
- `prototype_missing`：原型只有插入 slash prefix 的动作，没有 suggestion renderer。
- `required_prototype`：补充完整 row、scroll、focus、empty/error 和窄屏状态。
- `blocked_scope`：autocomplete 组件与交互；Composer shell 不受阻断。
- `shared_migration`：TASK-T038；空容器：Composer 的 #autocomplete。暂停：autocomplete 的 input 更新与异步 suggestion 回填；suggestion 的点击/hover 和 Up/Down/Enter/Escape 路径。
- `user_impact`：输入文本及原型已有 slash prefix 动作保留；不提供 suggestion 列表与选择。
- `implementation_status`：suspended。`#autocomplete` 保持为空；input、选择、键盘与迟到结果均由 runtime 守卫暂停，普通输入和 IME 保留。
- `safety_gate`：停用 suggestion 入口并验证旧异步结果不会回填；不得误停 Composer 普通输入与 IME。slash action 对其他缺失界面的转发按对应 gap 一并守卫。

### GAP-005：附件、文件引用与拖放反馈

- `coverage_id`：`UI-COMPOSER-003`
- `runtime_trigger`：附加文件/目录并把支持或不支持的路径拖入窗口。
- `current_runtime`：显示单个/多个引用、长或无效路径、drag enter/leave/drop、disabled directory 与 range highlight。
- `prototype_missing`：file/directory handler 是显式 no-op，没有应用内 reference token 或 drag feedback。
- `required_prototype`：补充 attachment/reference token 和 drag feedback 场景。
- `blocked_scope`：应用内附件、引用与拖放视觉；操作系统 picker chrome 不在本缺口范围。
- `shared_migration`：TASK-T038；空容器：Composer 的 #attachmentReferences 与 #attachmentDragFeedback。暂停：attachFileReference 与 attachDirectoryReference；native 文件拖放及 reference 插入路径。
- `implementation_status`：suspended。附件按钮禁用；picker 调用前、返回后与引用插入前均守卫，native/HTML drop 不插入内容。
- `user_impact`：暂停通过 GUI 添加附件/目录和拖放引用；不启动 picker 后伪装完成，也不改变已有 session 产品数据。
- `safety_gate`：在打开 picker 和插入引用前阻止入口；对已经打开的 picker 与 native drag 回调验证取消/迟到结果不会修改 draft 或恢复旧 UI。

### GAP-006：Model picker

- `coverage_id`：`UI-COMPOSER-004`
- `runtime_trigger`：在 populated、empty、刷新失败 registry 下打开 picker 并切换 model。
- `current_runtime`：支持 selected、hover/focus、scroll、empty、refresh、unsupported thinking 与 provider/switch error。
- `prototype_missing`：只有静态 model badge，`showModelPicker` 是 no-op。
- `required_prototype`：补充 model picker 的正常、空、错误、滚动和窄屏状态。
- `blocked_scope`：model picker；Composer 静态 badge 不受阻断。
- `shared_migration`：TASK-T038；空容器：#modelPickerSlot。暂停：showModelPicker 的按钮/快捷键入口；/model 命令、slash action modelPicker 与 picker selection。
- `implementation_status`：suspended。真实 model badge 与 registry 刷新保留，picker 按钮禁用，迟到刷新不打开 picker。
- `user_impact`：静态 model badge 与真实当前 model 保留；暂停通过未覆盖 picker 切换 model。
- `safety_gate`：models-updated 可继续更新真实 registry/badge；验证迟到刷新不打开 picker且停用入口不改变已选 model。General 的 model 修改入口另由 GAP-009 暂停。

### GAP-007：Queue、Steering、Fork 与 Subagent

- `coverage_id`：`UI-FLOW-001`
- `runtime_trigger`：turn 运行时发送 queue/steering，打开 fork/subagent，注入 empty/error 后 abort。
- `current_runtime`：支持 populated/empty、queue/steer、remove/action、active/finished/error、fork success/failure、aborting 和长文本。
- `prototype_missing`：`complete-session` 的静态 populated fixture 未覆盖四类面板的空白、错误和完整操作。
- `required_prototype`：补充四类面板的可操作、空白、错误状态及 running send/abort control。
- `blocked_scope`：Queue、Steering、Fork、Subagent 与 running composer 跨组件流程。
- `shared_migration`：TASK-T038；空容器：Composer 的 #queuedMessages 与 #steeringConversation 未覆盖状态 slot；#forkPicker 与 #subagentPanel 未覆盖状态 slot。暂停：running send 的未覆盖 queue/steer 操作及对应快捷键；未覆盖 queue action/remove 与 fork/subagent 操作；slash action forkPicker/subagentPanel。
- `user_impact`：已覆盖 populated 呈现与已覆盖 Stop/abort 出口保留；缺失的操作和变体暂停，不清空后端已有 queue 或 subagent 状态。
- `implementation_status`：operations-suspended-passive-state-retained。fork/subagent 容器为空；运行中 queue/steer 提交及其快捷键暂停且保留 draft，按钮始终为 Stop；已有 queue/steering 继续真实呈现和消费。不自动 abort，不停止后台 subagent，不清除承载被动状态的处理代码。
- `safety_gate`：ui-state 含 queuedMessages/steeringConversation，sendMessage 在 streaming 时调用 send_running_message；须验证已有队列和在途工作仍有安全处理与退出路径。无法安全暂停则阻断 running flow 迁移，不能自动 abort 或伪造完成。

## Tool call、evidence、diff 与 Markdown

### GAP-003A：Read、Edit 与 Bash tool lifecycle

- `coverage_id`：`UI-TOOL-001`
- `implementation_boundary`：Read/Bash 非空成功输出及 Edit 单文件完整 patch 使用共享实现；无结果、空输出、截断与未覆盖输出变体留空，暂停展开。真实错误仍由现有 renderer 呈现，迁移受阻；后台工具执行和原始结果不改写。
- `evidence_boundary`：`UI-EVIDENCE-001` 的成功且元数据完整 file changes/verification 已共享；失败 verification、不完整 capture 和未覆盖文件元数据保留真实处理并标记 migration-blocked，T036 不得删除。
- `runtime_trigger`：分别执行 Read/Edit/Bash 的成功、失败、取消和 missing-result，在 desktop/narrow 展开收起。
- `current_runtime`：支持 pending/running/success/error/cancelled、collapsed/expanded、empty/long output 和连续调用。
- `prototype_missing`：`tool-calls` 未实际呈现每个 tool family 的完整生命周期和极端内容。
- `required_prototype`：补充每类 tool 的 lifecycle、long/empty/error output 和窄屏 variant。
- `blocked_scope`：未被实际呈现的 Read/Edit/Bash tool 变体与共用 lifecycle。
- `shared_migration`：TASK-T018；空容器：conversation tool card 内未覆盖 Read/Edit/Bash lifecycle 与 output slot。暂停：未覆盖 tool variant 的展开/操作入口。
- `user_impact`：保留原型已覆盖 tool/evidence；未覆盖生命周期和输出变体留空，工具执行结果不作假。
- `safety_gate`：tool-event 与 ui-state/消息渲染可被动送达；隐藏可见入口不能停掉工具。须验证保留真实执行状态及已覆盖出口；不能安全处理在途结果或错误则阻断该区域迁移。

### GAP-003B：Subagent、Question 与 generic tool lifecycle

- `coverage_id`：`UI-TOOL-002`
- `implementation_boundary`：未覆盖 specialized/generic 成功 evidence 留空并暂停展开；真实错误与待回答 AskUserQuestion 保留现有处理和回答出口，迁移受阻。
- `runtime_trigger`：执行 Subagent、AskUserQuestion 与 generic tool 的成功、失败、取消和 missing-result。
- `current_runtime`：每类有专用或 generic evidence renderer，并支持生命周期与展开状态。
- `prototype_missing`：只有少量静态实例，没有完整 lifecycle、empty/long/error output。
- `required_prototype`：补充每个特殊 tool family 的完整状态矩阵。
- `blocked_scope`：未覆盖的 specialized/generic tool evidence 及其共用 lifecycle。
- `shared_migration`：TASK-T018；空容器：conversation tool card 内未覆盖 specialized/generic lifecycle 与 evidence slot。暂停：未覆盖 specialized/generic evidence 的展开/操作入口。
- `user_impact`：tool-calls 和 question scene 已覆盖状态保留；缺失 evidence 不展示。AskUserQuestion 已覆盖回答流程不因 tool evidence 缺失而停用。
- `safety_gate`：tool-event 与消息可被动送达；先验证缺失 evidence 留空仍保留真实状态和 question 回答出口。无法安全处理在途请求则阻断该区域迁移，不能伪造 tool/question 结果。

### GAP-002B：Rich Markdown 状态矩阵

- `coverage_id`：`UI-MARKDOWN-001`
- `implementation_boundary`：普通完成消息的纯文本/fenced code 使用共享 renderer；未覆盖 rich Markdown block 留空并暂停链接等操作，原始消息不改写。被动 streaming、continuation 和错误消息保留现有 renderer，迁移受阻。
- `runtime_trigger`：在 desktop/narrow 渲染 heading、nested list、blockquote、link、inline code、table、blocked image、长 CJK 和 code overflow。
- `current_runtime`：`renderMarkdown`/`renderTable` 对各 Markdown 构造及 overflow 产生可见结构。
- `prototype_missing`：现有消息未覆盖完整 Markdown 构造与极端长度。
- `required_prototype`：补充使用最终 message-content 实现的 Markdown 状态 gallery。
- `blocked_scope`：未被原型实际展示的 rich Markdown family。
- `shared_migration`：TASK-T018；空容器：message-content 中未覆盖 Markdown family 的 slot。暂停：未覆盖 Markdown family 的链接/展开/overflow 操作。
- `user_impact`：原型已展示的文本和 code/diff 等内容保留；缺失 Markdown 构造暂不呈现，不改写原始消息数据。
- `safety_gate`：renderMessages 可接收任意已有消息；须按 family 验证留空和已覆盖内容边界。若留空会掩盖必须回应的请求或真实错误且无已覆盖出口则阻断该区域迁移。

## PermissionRequest 与 AskUserQuestion

### GAP-008A：Permission main/hint 未覆盖状态

- `coverage_id`：`UI-PERMISSION-001`
- `runtime_trigger`：收到 permission-request，进入 hint，操作长命令、键盘选择、提交并触发失败。
- `current_runtime`：支持 main、hint、selected、长命令展开/收起、responding、failure 与 session-specific UI state。
- `prototype_missing`：主页面和 hint 只覆盖部分正常路径，缺少响应失败、长命令、键盘和窄屏组合。
- `required_prototype`：补充上述 main/hint variant 与可重复操作步骤。
- `blocked_scope`：未覆盖的 permission main/hint 状态；不得由其他面板推导视觉。
- `shared_migration`：TASK-T019；空容器：#permPanel 中未覆盖 main/hint 状态 slot。暂停：仅未覆盖 main/hint variant 的交互。
- `user_impact`：已覆盖 main/hint 决策路径保留；未覆盖长命令/失败/键盘与窄屏组合不补画。
- `safety_gate`：当前 permission-request 入 pendingPermissions 并等待真实 respond_permission；仅隐藏面板不能安全暂停。未覆盖请求/响应失败无已覆盖回应和恢复出口时阻断 permission 区域迁移；不得自动允许或拒绝。

### GAP-008B：Permission trust 与 approval lifecycle

- `coverage_id`：`UI-PERMISSION-002`
- `runtime_trigger`：进入 trust，排队第二个 request，切换 session，并强制 response failure/cancel。
- `current_runtime`：支持 trust level、queued/responding、failure、cancelled/stale 与 focus/state 恢复。
- `prototype_missing`：trust handler 只重新打开主 demo，没有 trust renderer 或 queue/error/cancel state。
- `required_prototype`：补充 trust-level、queued/responding/error/cancelled 和窄屏场景。
- `blocked_scope`：permission trust、队列与 approval lifecycle。
- `shared_migration`：TASK-T019；空容器：#permPanelTrust 内 trust slot；permission queue/responding/error/cancelled 未覆盖状态 slot。暂停：enterPermissionTrust 与 trust 选择快捷键；未覆盖 approval lifecycle 的操作。
- `user_impact`：暂停缺失 trust 操作；不能把缺失 queue/error/cancelled 状态伪装成已审批或已恢复。
- `safety_gate`：当前队列在 showPermission 中追加，成功回复后才移除；下一请求调用 prepare_permission。隐藏 trust 入口不能阻止被动排队或 response failure。因此 approval lifecycle 在验证安全回应/恢复之前阻断迁移；不得丢弃 pending 请求或自动答复。

## Settings、Appearance 与 permission settings

### GAP-009：填充后的 General settings

- `coverage_id`：`UI-SETTINGS-002`
- `runtime_trigger`：用正常、缺少 model、loading 和 update failure 打开 General。
- `current_runtime`：显示 model/provider/context、transport/retry、dependent/disabled、validation 与 save error。
- `prototype_missing`：只有隐藏 markup，没有激活并填充 General 的 scene fixture。
- `required_prototype`：补充 populated、loading、empty、dependent/error、长文本和窄屏状态。
- `blocked_scope`：General settings family。
- `shared_migration`：TASK-T020；空容器：#pane-general 内容 slot。暂停：main/sidebar General tab 与 pane 路由；General 内 model/provider/transport/retry 等更新入口。
- `user_impact`：General 为空且无法通过该界面读写设置；既有配置及其他已覆盖 Settings pane 保留。
- `safety_gate`：在 tab/跨 WebView 路由和 invoke 更新前守卫；进入迁移前确认无未完成保存，迟到 load/update 结果不重挂旧 pane且不伪造保存成功。

### GAP-010：Skills 与 Tools capability settings

- `coverage_id`：`UI-SETTINGS-003`
- `runtime_trigger`：在 global/project scope 切换并 reset inherited item，注入 update error。
- `current_runtime`：支持 populated/empty、inherited/local override、enabled/disabled、reset、loading/error。
- `prototype_missing`：capability renderer 是显式空函数。
- `required_prototype`：补充填充后的 Skills/Tools 场景和上述状态。
- `blocked_scope`：capability row、scope 与 update/reset flow。
- `shared_migration`：TASK-T020；空容器：#pane-skills 与 #pane-tools 的 capability/scope slot。暂停：main/sidebar Skills/Tools tab 与 pane 路由；capability scope/update/reset 操作。
- `user_impact`：暂停 GUI 的 Skills/Tools scope 与启用/继承/reset 修改；不修改实际 capability 配置。
- `safety_gate`：同时守卫 main/sidebar 路由和 row handlers；确认无在途更新并检查迟到刷新不会重新插入旧 row。

### GAP-011：Keyboard shortcut settings

- `coverage_id`：`UI-SETTINGS-004`
- `runtime_trigger`：打开 Keyboard shortcuts，搜索、修改/reset，并触发 invalid/conflict。
- `current_runtime`：支持 match/no-match、editing/listening、fixed/customizable、reset、conflict/error 与 keyboard focus。
- `prototype_missing`：只有对预存 row 的筛选逻辑，没有任何 row fixture。
- `required_prototype`：补充真实 row 和全部搜索、编辑、冲突、错误状态。
- `blocked_scope`：shortcut list、search 与 edit/error flow。
- `shared_migration`：TASK-T020；空容器：#pane-keyboard-shortcuts 的 search/list/editor slot。暂停：main/sidebar Keyboard shortcuts tab 与 pane 路由；shortcut 搜索/编辑/reset 入口。
- `user_impact`：暂停通过缺失设置界面修改快捷键；不全局停用未暂停产品流程所用快捷键。
- `safety_gate`：区分 shortcut 设置操作与既有运行快捷键；已开始编辑或保存的结果需安全收尾，不能把缺失编辑界面当成保存成功。

### GAP-012：Permission settings 与 rule editor

- `coverage_id`：`UI-SETTINGS-005`
- `runtime_trigger`：用 inherited/local rule 打开 Permissions，展开、排序、删除、恢复并创建 glob/regex/path rule。
- `current_runtime`：支持 Deny/Ask/Allow、empty、inherited/local、collapsed/expanded、drag/delete/reset、editor/combobox 与 save error。
- `prototype_missing`：`permissions` scene 只有空 shell，renderer 是显式空函数。
- `required_prototype`：补充填充且可操作的 permission-settings scene。
- `blocked_scope`：permission rule list、row、editor 与更新流程。
- `shared_migration`：TASK-T020；空容器：#pane-permissions 内 rule list 与 editor slot。暂停：main/sidebar Permissions settings tab 与 pane 路由；rule expand/reorder/delete/reset/add/edit/save 操作。
- `user_impact`：暂停 GUI permission rule 编辑；现有真实规则及 runtime approval 契约保留。
- `safety_gate`：暂停设置编辑不能自动改变 permission mode 或规则；确认无在途保存与迟到 rule editor 回填。runtime approval 的安全门槛另见 GAP-008A/008B。

## Notification 与错误反馈

### GAP-013：跨组件异常状态

- `coverage_id`：`UI-FEEDBACK-001`
- `runtime_trigger`：向各 surface 注入 invoke rejection、malformed/empty/stale/long payload，并在窄屏与 reduced motion 下操作。
- `current_runtime`：各 renderer 分别处理 loading、empty、disabled、recoverable/fatal error、retry/cancel、overflow、focus retention 与 session switch。
- `prototype_missing`：多个组件只有正常状态；不能用一个通用 fallback 证明各组件异常状态。
- `required_prototype`：为缺少异常状态的组件分别补充明确 variant。
- `blocked_scope`：任何没有具体原型和专项证据的异常状态；已有明确原型的状态不受阻断。
- `shared_migration`：TASK-T021（各区域任务共同核对）；空容器：各缺失异常状态所在 component slot（main/sidebar）。暂停：仅缺失异常 variant 的 retry/cancel/恢复操作。
- `user_impact`：已覆盖 Notification 与错误表达保留；未覆盖异常 variant 不补画，不以空对象/通用成功提示替代真实失败。
- `safety_gate`：error/notification/app-notification 与 invoke rejection 会被动到达；须逐组件核对真实错误是否仍可透明表达及安全退出。无已覆盖错误或恢复出口时阻断该组件迁移；不能静默丢弃错误或删掉必要取消操作。

## Related Docs

- [`GUI 界面对照表`](./UI_COVERAGE.md)
- [`机器可读界面对照表`](./UI_COVERAGE.json)
- [`SPEC：GUI 原型与真实前端组件化重构`](../../.dev-doc/refactor/ui-architecture/SPEC.md)
