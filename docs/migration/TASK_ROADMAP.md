# Electron + Go 按切片迁移任务索引

进入日期：2026-10-13，Asia/Singapore。

用户已决定“边对照，边实现”。正式任务状态、范围与依赖以 dow 为准；本页只维护任务顺序、功能 owner 和覆盖责任。

## 执行方式与边界

本路线保留迁移决策、任务依赖顺序与功能覆盖 owner。实现操作、构建/测试命令、工具版本和输出位置见 [CONTRIBUTING](../../CONTRIBUTING.md)；迁移决策及验收原则见 [SPEC](../../.dev-doc/refactor/electron-go-migration/SPEC.md)。

逐项覆盖根据实际实现更新；未完成能力保持可见，完整功能、逻辑等价、设计批准及跨平台验收均是最终交付要求。真实平台、原生或授权尚未验证时保持未验证；Rust/Tauri 生产链只有完整验收后才可退役。

## 功能与验收约束

保留 Electron 单 WebView + TypeScript + Go、stdio JSON-RPC、全部原有功能与处理逻辑等价、原 JSONL 写入/恢复规则。最终完整核对全部旧能力，真实平台/原生/授权未验证时不得宣称通过。

## 已完成的会话读取切片

**TASK-T001：打通 Electron→Go 会话列表读取纵向切片**，无前置依赖。已实现
Go catalog、stdio framing/dispatch、受控 Electron/preload/renderer 与候选构建；
验收结果见 [M001](evidence/M001.md)，正式任务状态以 `dow task show TASK-T001` 为准。

覆盖责任：T001 会话列表；T002 JSONL 只读树与有效上下文；T003 Electron 切换与只读历史。完整写入由 T017、完整消息状态由 T029/T045、运行会话由 T037/T049 负责。各切片的真实结果与未验证项见对应保留的 M001–M003。

**TASK-T002：JSONL 只读打开与有效上下文恢复** 已实现 Go `Open`、
`Entries`、`ContextMessages`，复用 catalog 的 header/entry 校验。按最后有效
entry 选择 leaf，通过 `parentId` 回溯分支，保留历史与有效上下文的区别；
最新 compaction 按 `firstKeptEntryId` 裁剪上下文，不生成额外 summary message。
同一组小 fixture 在 Go 和当前 Rust `SessionManager` 下对照，详见
[M002](evidence/M002.md)；正式完成状态以 `dow task show TASK-T002` 为准。
T003 负责 Electron 切换/历史链路，T017 负责完整写入 manager，T028 负责实际
压缩与配置 overlay；本项完成后停止，等待用户安排。

**TASK-T003：会话切换与只读历史恢复 TS 链路** 已接通受控 renderer → preload
→ main → stdio RPC → Go LoadedSessions/Reader。Go 按 path 复用 Loaded tab，
展示全部 active-branch 消息，模型有效上下文保留在 Go；renderer 保存本切片的
滚动/Thinking 展开，拒绝迟到响应，读取失败保留当前历史并支持手动重试。
TS/Go 聚焦测试和 macOS 隔离 native 验收见 [M003](evidence/M003.md)。
T029/T045 的完整消息、Markdown/image/tool 呈现和 T037/T049 的运行会话仍待实现；
本项不是完整迁移交付。正式状态以 `dow task show TASK-T003` 为准。

## 任务与实际依赖

T010 为当前 RPC 方法定义 typed errors 和接收取消契约；并发、事件与业务取消由所属后续任务负责。批准的协议行为见 [M010](evidence/M010.md) 和 SPEC，操作入口见 CONTRIBUTING。

T009 建立候选资源构建与 watch 基础；T013/T060 负责共享 TS 真源接入与最终搬迁。

T064 将本地 TypeScript compiler 设为默认工具链；T065 迁移为 Forge/Vite 开发与可运行 package；T014 负责后续 installer/发行资源验收。

T065：Forge/Vite 开发与可运行 package（依赖 T064）。
工具版本、构建分工、运行方式与产物路径见 CONTRIBUTING；真实验收与未验证项见 [M065](evidence/M065.md)。T013 继续共享 TS/templates 装载，T014 负责 installer。

分组仅便于检索，不是阶段 gate；任务顺序服从依赖关系。

### 会话读取纵向切片

| 任务 | 具体产出 | 前置任务 |
| --- | --- | --- |
| TASK-T001 | 打通 Electron→Go 会话列表读取纵向切片 | 无 |
| TASK-T002 | 移植 JSONL 只读打开与有效上下文恢复 | TASK-T001 |
| TASK-T003 | 接通会话切换与只读历史恢复 TS 链路 | TASK-T002 |

### 当前候选的构建、通信与生命周期增量

| 任务 | 具体产出 | 前置任务 |
| --- | --- | --- |
| TASK-T009 | 完善候选开发构建与资源 watch 链路 | TASK-T001 |
| TASK-T010 | 实现当前 RPC 方法的类型化错误与取消路由 | TASK-T001 |
| TASK-T011 | 扩展 stdio 并发调度与 snapshot 事件同步 | TASK-T010 |
| TASK-T012 | 完善 Electron bridge 与 Go 异常退出及重载生命周期 | TASK-T011 |
| TASK-T013 | 在 Forge/Vite 资源图中转换共享 core/port 与原型 TS 装载 | TASK-T009、TASK-T065 |
| TASK-T014 | 完善 Forge installer 配置及发行资源验收 | TASK-T009、TASK-T012、TASK-T013、TASK-T065 |
| TASK-T064 | 默认 TypeScript 工具链切换 TS7 | TASK-T009 |
| TASK-T065 | Forge/Vite 开发与可运行 package | TASK-T064 |

### Go 产品模块（按实际依赖选择一项）

| 任务 | 具体产出 | 前置任务 |
| --- | --- | --- |
| TASK-T015 | 移植分层 settings、配置路径与持久化错误语义 | TASK-T001 |
| TASK-T016 | 移植模型 registry、thinking/context/quota 与配置覆盖 | TASK-T015 |
| TASK-T017 | 实现完整 JSONL manager、lazy/branch/reopen 与写入规则 | TASK-T002、TASK-T015 |
| TASK-T018 | 移植 Anthropic provider 的完整请求与流式契约 | TASK-T016 |
| TASK-T019 | 移植 agent loop、并行工具/事件/hook/终止与取消 | TASK-T016 |
| TASK-T020 | 建立 Go app owner 与 CLI prompt→stream→JSONL→重启路径 | TASK-T017、TASK-T018、TASK-T019、TASK-T011、TASK-T024 |
| TASK-T021 | 移植 OpenAI Completions 请求/SSE/tool/usage/error | TASK-T016 |
| TASK-T022 | 移植 OpenAI Responses 转换、stream/continuation 与错误 | TASK-T016 |
| TASK-T023 | 移植 Bedrock Converse/event-stream/签名与错误 | TASK-T016 |
| TASK-T024 | 实现 credentials 优先级与 OAuth 公共 PKCE/callback/device 基础 | TASK-T015 |
| TASK-T025 | 移植 Read/Write/Edit、模糊匹配/锁与 diff evidence | TASK-T019 |
| TASK-T026 | 移植 Bash/命令分段与三平台 process tree 取消 | TASK-T019 |
| TASK-T027 | 移植 AskUserQuestion 多题/多选/Other、队列与迟到处理 | TASK-T019 |
| TASK-T028 | 移植 compaction、有效上下文恢复与 model/thinking overlay | TASK-T017、TASK-T019 |
| TASK-T030 | 移植主题定义、解析、存储与设置生效的 Go 契约 | TASK-T015 |
| TASK-T031 | 移植快捷键定义、验证、冲突和持久化的 Go 契约 | TASK-T015 |
| TASK-T032 | 接通 Anthropic OAuth login/refresh/logout app 生命周期 | TASK-T018、TASK-T024 |
| TASK-T033 | 接通 OpenAI Codex OAuth、模型刷新与兼容版本同步 | TASK-T022、TASK-T024 |
| TASK-T034 | 接通 GitHub Copilot OAuth/device 与可用模型流程 | TASK-T021、TASK-T024 |
| TASK-T035 | 移植 permission rule/mode/approval/trust/Hint 的安全决策 | TASK-T015、TASK-T026 |
| TASK-T037 | 移植后台会话、queue/steering/fork 与生命周期 owner | TASK-T020、TASK-T035、TASK-T027、TASK-T028 |
| TASK-T038 | 移植 skills/resources/extensions、capabilities 与 hook 执行语义 | TASK-T015、TASK-T019、TASK-T035 |
| TASK-T040 | 移植 subagent scope/context/运行恢复取消及事件路由 | TASK-T025、TASK-T026、TASK-T037 |
| TASK-T041 | 移植 Dev Flow discovery/dashboard/REST/SSE 重连回收 | TASK-T015、TASK-T026、TASK-T037 |
| TASK-T044 | 完成 CLI 全参数、GUI 启动与所有 slash/session 操作 | TASK-T009、TASK-T017、TASK-T020、TASK-T021、TASK-T022、TASK-T023、TASK-T032、TASK-T033、TASK-T034、TASK-T025、TASK-T026、TASK-T035、TASK-T027、TASK-T028、TASK-T037、TASK-T040、TASK-T038、TASK-T041、TASK-T030、TASK-T031 |

### 对应界面的设计审阅（仅阻断受影响 UI）

| 任务 | 具体产出 | 前置任务 |
| --- | --- | --- |
| TASK-T004 | 消息、Markdown 与工具缺口设计审阅 | TASK-T002 |
| TASK-T005 | Composer、附件、模型及运行中流程设计审阅 | TASK-T016、TASK-T038 |
| TASK-T006 | 权限 trust 与多问题/多选设计审阅 | TASK-T027、TASK-T035 |
| TASK-T007 | General、capability、快捷键及规则编辑设计审阅 | TASK-T015、TASK-T030、TASK-T031、TASK-T035、TASK-T038 |
| TASK-T008 | quota、Dev Flow 和逐组件异常设计审阅 | TASK-T016、TASK-T041 |

### TS UI 与原生交互切片

| 任务 | 具体产出 | 前置任务 |
| --- | --- | --- |
| TASK-T029 | 转换消息/Markdown TS 真源并恢复全部内容状态 | TASK-T004、TASK-T013、TASK-T003、TASK-T020 |
| TASK-T036 | 转换 settings/appearance TS 与恢复 General | TASK-T007、TASK-T013、TASK-T015、TASK-T016、TASK-T024、TASK-T030 |
| TASK-T039 | 恢复权限/Hint/trust 与多问题多选 TS 交互 | TASK-T006、TASK-T013、TASK-T012、TASK-T035、TASK-T027 |
| TASK-T042 | 转换 Composer/autocomplete/model picker TS 与输入行为 | TASK-T005、TASK-T013、TASK-T012、TASK-T016、TASK-T038 |
| TASK-T043 | 恢复 Skills/Tools capability scope 与 inherited/reset TS UI | TASK-T007、TASK-T038、TASK-T036 |
| TASK-T045 | 转换工具/evidence/diff TS 并恢复完整生命周期 | TASK-T004、TASK-T013、TASK-T012、TASK-T025、TASK-T026、TASK-T027、TASK-T040 |
| TASK-T046 | 恢复附件/文件引用与三平台 native picker/drop | TASK-T005、TASK-T012、TASK-T042 |
| TASK-T047 | 恢复快捷键列表/搜索/capture/冲突/reset TS 流程 | TASK-T007、TASK-T012、TASK-T043、TASK-T031 |
| TASK-T048 | 转换 sidebar/Dev Flow TS、恢复 quota 与详情/空列表 | TASK-T008、TASK-T013、TASK-T012、TASK-T016、TASK-T041、TASK-T036 |
| TASK-T049 | 恢复运行中 queue/steering/fork/subagent TS 跨组件流程 | TASK-T005、TASK-T013、TASK-T037、TASK-T040、TASK-T042、TASK-T046 |
| TASK-T050 | 恢复权限 settings/rule editor 全状态与 TS 真源 | TASK-T007、TASK-T035、TASK-T047 |
| TASK-T051 | 转换 notification/popover TS 并逐组件恢复异常出口 | TASK-T008、TASK-T012、TASK-T029、TASK-T045、TASK-T042、TASK-T046、TASK-T049、TASK-T039、TASK-T036、TASK-T043、TASK-T047、TASK-T050、TASK-T048 |
| TASK-T052 | 完成单 WebView native shell/scene/state 与原生窗口接管 | TASK-T012、TASK-T046、TASK-T049、TASK-T047、TASK-T048、TASK-T051 |

### 最终汇接、完整功能与真实验收

| 任务 | 具体产出 | 前置任务 |
| --- | --- | --- |
| TASK-T053 | 完成全部 runtime/原型 TS 与生产 adapter 汇接收口 | TASK-T003、TASK-T044、TASK-T029、TASK-T045、TASK-T042、TASK-T046、TASK-T049、TASK-T039、TASK-T036、TASK-T043、TASK-T047、TASK-T050、TASK-T048、TASK-T051、TASK-T052 |
| TASK-T054 | 真实 provider/OAuth 授权与远端错误/取消 smoke | TASK-T044、TASK-T053 |
| TASK-T055 | 整链路故障/重启/进程清理与完整等价矩阵收口 | TASK-T044、TASK-T053 |
| TASK-T056 | macos 实际安装包、CLI、原生 GUI 与系统能力验收 | TASK-T014、TASK-T054、TASK-T055 |
| TASK-T057 | linux 实际安装包、CLI、原生 GUI 与系统能力验收 | TASK-T014、TASK-T054、TASK-T055 |
| TASK-T058 | windows 实际安装包、CLI、原生 GUI 与系统能力验收 | TASK-T014、TASK-T054、TASK-T055 |
| TASK-T059 | 完整功能验收与维护收益最终判定 | TASK-T054、TASK-T055、TASK-T056、TASK-T057、TASK-T058 |

### 完整验收后的切换与退役

| 任务 | 具体产出 | 前置任务 |
| --- | --- | --- |
| TASK-T060 | 将唯一共享 TS/HTML/CSS 真源迁到 ui 并更新全部引用 | TASK-T059 |
| TASK-T061 | 切换默认 Electron+Go 开发/发行并退役 Rust/Tauri 生产链 | TASK-T060 |
| TASK-T062 | 最终证据/文档/数据与全部 AC 交付闭环 | TASK-T061 |

## 功能覆盖与差异（随实现更新）

下表是 owner 声明，不是全部通过证据。原有能力包括因 GUI 设计缺口暂停的能力。实现时更新本表状态；只在有独有决策或确需长期保留的真实验收结果时更新相应文档。具体失败输入只加小 fixture/回归用例，不复制全库。

| 功能族 | 覆盖范围 | 实现 owner | 当前状态 |
| --- | --- | --- | --- |
| F01 | 模型配置、registry、thinking/context/quota | TASK-T016、TASK-T028、TASK-T033、TASK-T042、TASK-T048 | 待实现 |
| F02 | Anthropic、Completions、Responses、Bedrock | TASK-T018、TASK-T021、TASK-T022、TASK-T023 | 待实现 |
| F03 | credentials、PKCE/callback/device、刷新/注销 | TASK-T024、TASK-T032、TASK-T033、TASK-T034、TASK-T054 | 待实现 |
| F04 | Agent loop、hooks、并行/终止/取消 | TASK-T019、TASK-T020、TASK-T026、TASK-T037、TASK-T038 | 待实现 |
| F05 | 会话列表/读取/JSONL写入、分支/恢复/重命名/删除/cwd | TASK-T001、TASK-T002、TASK-T003、TASK-T010、TASK-T017、TASK-T037、TASK-T044 | T001 列表、T002 只读树恢复、T003 Electron 切换/历史已实现，见 M001/M002/[M003](evidence/M003.md)；T010 为现有方法增补类型化错误、库通信与接收取消，见 [M010](evidence/M010.md)；写入与运行会话等操作待实现 |
| F06 | compaction、有效上下文与会话配置 overlay | TASK-T002、TASK-T028 | T002 已恢复持久化有效上下文，见 M002；实际压缩与配置 overlay 待 T028 |
| F07 | Read/Write/Edit/Bash、锁、diff/evidence与进程树 | TASK-T025、TASK-T026、TASK-T045 | 待实现 |
| F08 | permission模式/规则/approval/trust/Hint | TASK-T035、TASK-T039、TASK-T050 | 待实现 |
| F09 | 多题/多选/Other、队列/取消/迟到响应 | TASK-T027、TASK-T039 | 待实现 |
| F10 | 后台会话、queue/steering/fork/subagent | TASK-T037、TASK-T040、TASK-T049 | 待实现 |
| F11 | 分层设置、主题、快捷键、capability/skills/resources/extensions | TASK-T015、TASK-T030、TASK-T031、TASK-T036、TASK-T038、TASK-T043、TASK-T047、TASK-T050 | 待实现 |
| F12 | Dev Flow discovery/dashboard/REST/SSE/重连/回收 | TASK-T041、TASK-T048 | 待实现 |
| F13 | 消息/Markdown/tools/notification与异常 | TASK-T003、TASK-T029、TASK-T045、TASK-T051 | T003 仅文本历史与读取错误/重试，见 M003；完整内容/运行状态仍待实现 |
| F14 | Composer/autocomplete/model/file refs/picker/drop/IME | TASK-T042、TASK-T046、TASK-T049 | 待实现 |
| F15 | 单窗口材质/split/scene/root与焦点草稿滚动 | TASK-T003、TASK-T012、TASK-T052、TASK-T056、TASK-T057、TASK-T058 | T003 只读历史滚动/Thinking 展开按 session 保存，见 M003；其余 shell/Composer 状态待实现 |
| F16 | 全部CLI参数/text/json/continue/resume/slash/session | TASK-T020、TASK-T044 | 待实现 |
| F17 | PATH/config/版本/开发构建/安装包/发行 | TASK-T001、TASK-T009、TASK-T014、TASK-T015、TASK-T033、TASK-T044、TASK-T056、TASK-T057、TASK-T058、TASK-T060、TASK-T061、TASK-T062 | 待实现 |

差异记录：当前切片发现的旧行为与需求冲突，由对应 owner 明确差异并确认；未覆盖能力持续可见。T055 核对五 crate active exports、GUI commands/events、CLI 参数与 slash、settings/config/credentials、JSONL 和暂停入口；T059/T062 收口全部 Must。完整覆盖是最终交付要求，不是开工前置任务。

## 设计缺口 owner

具体方案在迁移到对应界面时审阅。T004–T008只有对应 UI 的批准依赖，不阻断独立 backend。未批准不修改 prototype，未恢复不移除暂停守卫。

| 缺口 | 设计审阅 | 恢复实现 |
| --- | --- | --- |
| GAP-002、GAP-002B | TASK-T004 | TASK-T029 |
| GAP-003A、GAP-003B | TASK-T004 | TASK-T045 |
| GAP-004、GAP-006 | TASK-T005 | TASK-T042 |
| GAP-005 | TASK-T005 | TASK-T046 |
| GAP-007 | TASK-T005 | TASK-T049 |
| GAP-008A、GAP-008B、GAP-008C | TASK-T006 | TASK-T039 |
| GAP-009 | TASK-T007 | TASK-T036 |
| GAP-010 | TASK-T007 | TASK-T043 |
| GAP-011 | TASK-T007 | TASK-T047 |
| GAP-012 | TASK-T007 | TASK-T050 |
| GAP-001、GAP-014 | TASK-T008 | TASK-T048 |
| GAP-013 | TASK-T008 | TASK-T051 |

## 最终验收 owner

15 条 SPEC-AC 均有 owner；下表只声明责任。全部实现切片须进行相应逻辑对照和针对性测试，最终汇总不代替每项验证。

| 验收标准 | 主要 owner |
| --- | --- |
| SPEC-AC-001 | TASK-T055、TASK-T059、TASK-T062 |
| SPEC-AC-002 | 全部相关实现切片；TASK-T055、TASK-T062 |
| SPEC-AC-003 | TASK-T001、TASK-T010、TASK-T011、TASK-T012、TASK-T019、TASK-T020、TASK-T037、TASK-T044、TASK-T053、TASK-T061 |
| SPEC-AC-004 | TASK-T001、TASK-T002、TASK-T003、TASK-T015、TASK-T017、TASK-T024、TASK-T028、TASK-T030、TASK-T031、TASK-T035、TASK-T055、TASK-T062 |
| SPEC-AC-005 | TASK-T016、TASK-T018、TASK-T021、TASK-T022、TASK-T023、TASK-T024、TASK-T032、TASK-T033、TASK-T034、TASK-T054、TASK-T062 |
| SPEC-AC-006 | TASK-T019、TASK-T020、TASK-T025、TASK-T026、TASK-T027、TASK-T028、TASK-T035、TASK-T037、TASK-T038、TASK-T040、TASK-T041、TASK-T044、TASK-T055、TASK-T056、TASK-T057、TASK-T058 |
| SPEC-AC-007 | TASK-T001、TASK-T003、TASK-T010、TASK-T011、TASK-T012、TASK-T053、TASK-T055 |
| SPEC-AC-008 | TASK-T004、TASK-T005、TASK-T006、TASK-T007、TASK-T008、TASK-T029、TASK-T036、TASK-T039、TASK-T042、TASK-T043、TASK-T045、TASK-T046、TASK-T047、TASK-T048、TASK-T049、TASK-T050、TASK-T051、TASK-T053、TASK-T055、TASK-T062 |
| SPEC-AC-009 | TASK-T012、TASK-T052、TASK-T056、TASK-T057、TASK-T058 |
| SPEC-AC-010 | TASK-T014、TASK-T015、TASK-T023、TASK-T024、TASK-T025、TASK-T026、TASK-T041、TASK-T044、TASK-T052、TASK-T054、TASK-T056、TASK-T057、TASK-T058 |
| SPEC-AC-011 | 全部相关实现切片；TASK-T055、TASK-T056、TASK-T057、TASK-T058、TASK-T060、TASK-T061、TASK-T062 |
| SPEC-AC-012 | TASK-T053、TASK-T054、TASK-T055、TASK-T056、TASK-T057、TASK-T058、TASK-T059、TASK-T062 |
| SPEC-AC-013 | TASK-T009、TASK-T014、TASK-T044、TASK-T056、TASK-T057、TASK-T058、TASK-T060、TASK-T061、TASK-T062 |
| SPEC-AC-014 | TASK-T059、TASK-T062 |
| SPEC-AC-015 | TASK-T001、TASK-T009、TASK-T010、TASK-T012、TASK-T013、TASK-T029、TASK-T036、TASK-T039、TASK-T042、TASK-T043、TASK-T045、TASK-T046、TASK-T047、TASK-T048、TASK-T049、TASK-T050、TASK-T051、TASK-T052、TASK-T053、TASK-T060、TASK-T061、TASK-T062 |

## 审查与验证边界

取消、修改或完成任务的工作流操作见 CONTRIBUTING；正式状态由 dow 管理。平台、原生与真实授权尚未验证时保持待完成；focused checks 不等于全项目 TEST。性能/维护收益在候选就绪后按 T059 验收。

## Related Docs

- [迁移 SPEC](../../.dev-doc/refactor/electron-go-migration/SPEC.md)
- [GUI 架构](../gui/ARCHITECTURE.md)、[设计缺口](../../prototype/docs/DESIGN_GAPS.md)
- [贡献与测试约定](../../CONTRIBUTING.md)
- 正式任务：`dow task list --all` / `dow task show TASK-Txxx`（仅通过 dow 管理）
