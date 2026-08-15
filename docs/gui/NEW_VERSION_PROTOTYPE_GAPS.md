# 新版 GUI 原型缺口

本文档列出当前 `docs/gui/new-version/` 无法忠实支撑的 runtime 可见表面。这里不提出或实现视觉方案；需要由用户补充对应原型场景或组件，覆盖清单变为 `covered` 后才恢复迁移。

新版原型的 CSS 实现真源是 `docs/gui/new-version/styles/` 组件库。新增缺失场景时应直接复用 `styles/main.css` 中的公共组件；确有场景专属规则时才增加隔离的 `styles/scenes/<scene>.css` 并在对应 HTML 中直接加载。`styles/source-order.json` 仅是原样拆分的来源与顺序证据，已删除的 `rozsa-gui.css` 不是兼容入口。以下缺口是场景、状态和交互证据缺失，不是要求再次拆分或重做 CSS。

当前没有已批准的 partial-component 例外。隐藏 DOM、单独 CSS rule 或 no-op prototype handler 均不算完成状态。

## GAP-001：Quota tooltip 与限额变体

- **状态：** `partial`；阻断 `SURF-005` 和 `TASK-T003` 中的 quota tooltip。
- **Runtime 入口：** `showQuotaTooltip`、`hideQuotaTooltip`、`updateQuotaWindow`、`renderSidebarQuotaWindow`、`#quotaTooltip`。
- **触发与数据：** hover/focus 小时或周限额条；window 数值、reset time、可见性和 display mode。
- **已知状态：** normal、near limit、exhausted、unavailable/error、tooltip open、长 reset 文本、窄宽度。
- **复现：** 提供 rate-limit 数据并显示 sidebar，依次 hover/focus 每个 quota window。
- **需要补充的原型：** 实际打开 tooltip，并展示 normal、near-limit/exhausted、unavailable、长内容和窄屏。
- **可继续范围：** `SURF-004` 的基本 quota bar；tooltip 与语义化限额变体保持阻断。
- **契约判断：** 现有 rate-limit snapshot 足够，当前无 Rust 扩展证据。

## GAP-002：消息与 rich Markdown 状态矩阵

- **状态：** `partial`；阻断 `SURF-007` 未覆盖部分和 `SURF-012`。
- **Runtime 入口：** `renderMessages`、`renderMessage`、`renderMarkdown`、`renderTable`、streaming cursor 和 message error 分支。
- **触发与数据：** user/assistant/system message、streaming flag、Markdown source、message-level error。
- **已知状态：** streaming partial text、continuation、error、heading、nested list、blockquote、link、inline code、table、长无断点文本、长 CJK、blocked image、code overflow。
- **复现：** 在 desktop/narrow 宽度渲染每种 Markdown 构造以及 streaming/error message。
- **需要补充的原型：** 使用最终 CSS/JS 的 message-content 场景或状态 gallery，逐项实际呈现上述状态。
- **可继续范围：** `complete-session` 已具体呈现的消息；共用 rich Markdown family 保持阻断。
- **契约判断：** 现有 message payload 足够。

## GAP-003：完整 tool evidence 变体矩阵

- **状态：** `partial`；阻断 `SURF-009`、`SURF-010` 未覆盖变体以及共用 tool component。
- **Runtime 入口：** Bash/Read/Write/Edit/Subagent/Question/Generic renderer、`renderToolEvidence`、`handleToolEvent`。
- **触发与数据：** tool call/result pair 与实时 `tool-event`。
- **已知状态：** pending/running/success/error/cancelled、missing result、collapsed/expanded、empty/long output、长 command/path、连续调用、窄屏。
- **复现：** 每类 tool 分别执行成功和失败，在 desktop/narrow 宽度展开与收起。
- **需要补充的原型：** 每个 tool family 与 lifecycle state 的具体场景，包括 long/empty/error output。
- **可继续范围：** 只有用户按 SPEC 批准且能独立验收的隔离变体可以继续。
- **契约判断：** 现有 tool call/result/event payload 足够。

## GAP-004：Slash-command autocomplete

- **状态：** `missing`；阻断 `SURF-014`。
- **Runtime 入口：** `updateAutocomplete`、`navigateAutocomplete`、`confirmAutocomplete`、`selectAutocomplete`、`hideAutocomplete`。
- **触发与数据：** composer text/cursor、items、prefix、validity、highlight ranges。
- **已知状态：** populated、no selection、keyboard-selected、hover、overflow/scroll、empty、stale、invoke error、IME、窄屏。
- **复现：** 输入 `/`，使用 Up/Down/Enter/Escape；再用 IME 和长列表重复。
- **需要补充的原型：** 带完整 row、scroll、focus、empty/error 状态的 autocomplete 场景。
- **可继续范围：** composer shell 可以继续，autocomplete 保持阻断。
- **源码证据：** 原型仅绑定 `insertSlashCommandPrefix`，没有 suggestion renderer；`AutocompleteResponse` 已足够。

## GAP-005：附件、文件引用与拖放反馈

- **状态：** `missing`；阻断 `SURF-015`。
- **Runtime 入口：** `attachFileReference`、`attachDirectoryReference`、`insertFileReferences`、`formatFileReference`、`handleNativeFileDrag`。
- **触发与数据：** picker result 或 drag event 中的文件/目录路径与 composer reference range。
- **已知状态：** 单个/多个引用、空格/quoted/长/无效路径、drag enter/leave/drop、disabled directory、highlight overlap、窄屏。
- **复现：** 附加文件和目录，再将支持与不支持的路径拖入窗口。
- **需要补充的原型：** attachment/reference token 与 drag feedback 的具体场景。
- **可继续范围：** native picker chrome 仍由操作系统拥有；应用内视觉保持阻断。
- **源码证据：** 原型的 file/directory handler 是显式 `noOpToast`；现有 picker/drag payload 足够。

## GAP-006：Model picker

- **状态：** `missing`；阻断 `SURF-016`。
- **Runtime 入口：** `showModelPicker`、`selectModel`、`renderModelSelector`、`models-updated`。
- **触发与数据：** model id/name/provider/reasoning、当前选择、空列表和错误。
- **已知状态：** populated、selected、hover/focus、scroll、empty、refresh、unsupported thinking、provider/switch error、窄屏。
- **复现：** 在 populated、empty、失败 registry 下点击 model badge 并切换不同 reasoning 能力的 model。
- **需要补充的原型：** 覆盖上述状态的 model-picker 场景。
- **可继续范围：** composer 中静态 model badge 可以继续；picker 保持阻断。
- **源码证据：** 原型 `showModelPicker` 是显式 `noOpToast`；现有 model command/entry 足够。

## GAP-007：Running、fork、queue、steering 与 subagent 面板

- **状态：** `partial`；阻断 `SURF-018`。
- **Runtime 入口：** `renderRunningMessages`、`showForkPicker`、`forkAtMessage`、`showSubagentPanel`、running send/abort。
- **触发与数据：** queued/steering message、fork point、subagent list、running state、send mode、invoke failure。
- **已知状态：** populated/empty、queue/steer、remove/action、active/finished/error、fork success/failure、aborting、长文本、窄屏。
- **复现：** turn 运行时发送消息，打开 fork/subagent，注入 empty/error payload 后 abort。
- **需要补充的原型：** 四类面板的可操作、空白、错误状态及 running send/abort control。
- **可继续范围：** `complete-session.html` 的静态 populated fixture 不足以放行共用组件。
- **契约判断：** 现有 queue/fork/subagent command 足够。

## GAP-008：Permission trust 与 approval lifecycle

- **状态：** `partial`/`missing`；阻断 `SURF-019`、`SURF-020` 与默认整个 permission component family。
- **Runtime 入口：** `showPermission`、`renderPermissionCommand`、`renderPermissionTrustPage`、selection/state capture、queued approval。
- **触发与数据：** request/session/tool/description/command、trust level、queue order、response error、abort/session switch。
- **已知状态：** main、hint、trust level、selected、长命令展开/收起、queued、responding、failure、cancelled/stale、窄屏。
- **复现：** 进入 hint/trust，排队第二个 request，切换 session，并强制 response failure。
- **需要补充的原型：** trust-level、queued/responding/error/cancelled、长命令和窄屏场景。
- **可继续范围：** 只有用户明确批准的独立 main/hint 子项可以继续。
- **源码证据：** 当前 trust handler 仅重新打开主 permission demo；现有 permission payload 足够。

## GAP-009：填充后的 General settings

- **状态：** `missing`；阻断 `SURF-023`。
- **Runtime 入口：** `renderGeneralSettings`、`renderSettingsPane`、model/runtime setting update handler。
- **触发与数据：** settings snapshot、model/provider/context、transport/retry、save result/error。
- **已知状态：** populated、loading、empty model、invalid number、disabled/dependent、save error、长文本、窄屏。
- **复现：** 用正常、缺少 model、update failure backend 打开 General。
- **需要补充的原型：** 激活并填充 General 的场景，包含 dependent/error/narrow 状态。
- **可继续范围：** 静态隐藏 markup 不算覆盖，General family 保持阻断。
- **契约判断：** 现有 settings/model snapshot 足够。

## GAP-010：Skills 与 Tools capability settings

- **状态：** `missing`；阻断 `SURF-024`。
- **Runtime 入口：** `renderCapabilitySettings`、`renderCapabilityPane`、scope/change/reset handler。
- **触发与数据：** project/global scope、capability item、inherited/enabled、update/reset error。
- **已知状态：** populated、empty、inherited、local override、enabled/disabled、reset、loading/error、长标签、窄屏。
- **复现：** 在 global/project scope 打开 Skills/Tools，切换并 reset inherited item，注入 update error。
- **需要补充的原型：** 填充后的 Skills/Tools 场景，覆盖上述状态。
- **可继续范围：** 整个 capability row/scope family 保持阻断。
- **源码证据：** 原型 renderer 是显式空函数；现有 capability command 足够。

## GAP-011：Keyboard shortcut settings

- **状态：** `missing`；阻断 `SURF-025`。
- **Runtime 入口：** `renderKeyBindings`、update/reset handler、search。
- **触发与数据：** customizable/fixed binding、query、update/reset result、validation error。
- **已知状态：** populated、match/no match、editing/listening、reset、conflict/error、长 command、keyboard focus、窄屏。
- **复现：** 打开 Keyboard shortcuts，搜索、修改/reset，并触发 invalid/conflict。
- **需要补充的原型：** 包含真实 row 和全部交互状态的 shortcut 场景。
- **可继续范围：** shortcut list/search/error family 保持阻断。
- **源码证据：** 原型只过滤预先存在的 row，但没有提供任何 row fixture；现有 command 足够。

## GAP-012：填充后的 permission settings 与 rule editor

- **状态：** `missing`；阻断 `SURF-026`。
- **Runtime 入口：** permission settings/list/row renderer、editor、tool option、save/reset/delete/move handler。
- **触发与数据：** mode、global/project rule、inherited flag、Read-only Bash、tool、regex/path、update error。
- **已知状态：** Deny/Ask/Allow、empty、inherited/local、collapsed/expanded、drag/delete/reset、editor、combobox、invalid/save error、窄屏。
- **复现：** 用 inherited/local rule 打开 Permissions，展开、排序、删除、恢复并创建 glob/regex/path rule。
- **需要补充的原型：** 填充且可操作的 permission-settings 场景。
- **可继续范围：** permission settings/rule family 保持阻断。
- **源码证据：** `permissions.html` 打开空 shell，renderer 是显式空函数；现有 payload/command 足够。

## GAP-013：跨组件异常状态

- **状态：** `partial`；阻断 `SURF-035` 以及任何无法证明异常状态已覆盖的迁移项。
- **Runtime 入口：** 各 renderer 的 loading/error/empty/disabled/long-content 分支与 `showError`。
- **触发与数据：** invoke rejection、malformed/empty payload、stale revision、长 CJK/path/command、窄屏、reduced motion。
- **已知状态：** loading、empty、disabled、recoverable/fatal error、retry/cancel、overflow、focus retention、session switch。
- **复现：** 对每个 surface 注入失败、空白、stale 和超长 payload，并在窄屏与 reduced motion 下操作。
- **需要补充的原型：** 缺少异常状态的组件需要各自的原型 variant；不能用通用 fallback 自行补画。
- **可继续范围：** 已有明确原型和专项证据的状态可以继续，其余逐项阻断。
- **契约判断：** 如果现有 IPC 无法触发所需原型状态，只记录最小契约建议并请求用户批准，不自行扩展 Rust。

## Rust/IPC 边界结论

当前 covered 原型没有证明必须修改 `crates/rozsa-gui/src/`。最新 `DevFlowDetailItem` 已包含 refs、dependency、done-when 与 create/modify/test file array。后续发现 IPC 不足时，只记录真实入口、payload、错误路径、跨平台影响与最小契约建议；没有用户批准和 task scope 更新，不实现 Rust 变更。

## 平台待验证项

- macOS：在对应迁移任务中完成视觉与行为验收。
- Linux/Windows：当前只完成 contract/fallback 自动化验证；真实视觉结果标为 `pending platform visual verification`，不宣称通过。

## 相关文档

- [`新版 GUI 迁移覆盖清单`](./NEW_VERSION_MIGRATION_COVERAGE.md)
- [`机器可验证覆盖清单`](./NEW_VERSION_MIGRATION_COVERAGE.json)
- [`新版 GUI 原型`](./new-version/)
- [`原型 CSS 组件库说明`](./new-version/styles/README.md)
- [`SPEC.md`](../../.dev-doc/main/SPEC.md)
