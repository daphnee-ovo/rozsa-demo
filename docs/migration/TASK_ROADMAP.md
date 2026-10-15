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

T010 为当前 RPC 方法定义 typed errors 和接收取消契约；T011 增补当前 session 切片的并发与事件同步基础，产品工作 abort 随业务切片接入。批准的库协议行为见 [M010](evidence/M010.md) 和 SPEC，操作入口见 CONTRIBUTING。

### TASK-T011：当前 session 并发与 snapshot 同步

Go app 的 `session.Service` 持有 Loaded session 状态、进程实例 epoch、单调水位及
32 项有界订阅缓冲；transport 只调度和同步，不从事件重建产品状态。读取与快照可并发，
已接受的 switch 按 wire 顺序进入 app；最多 8 个 app worker，取消接收后仍运行的工作
继续占用容量，超额请求透明返回 `busy`。这不是对库内部响应 writer/goroutine 数的容量承诺。

| 当前方法 | 行为 |
| --- | --- |
| `subscribe_sessions` | 替换本连接的旧订阅；先注册 app 订阅，再读取 `{epoch,watermark,active}` snapshot，返回新的 `subscriptionId`。`active` 是现有 Loaded history 投影或 null。 |
| `next_session_event` | 以 subscriptionId 等待一个事件；同一订阅只允许一个在途读取。缓冲重放跳过 snapshot 已覆盖的序号、旧 epoch 与重复事件，只交付连续的新事件。 |
| `unsubscribe_sessions` | 关闭订阅、唤醒在途读取；旧 ID 不能访问替换后的订阅。 |

本切片采用请求驱动的事件读取，先交付 snapshot 再读取后续事件；没有独立 notification
推送 writer。序号缺口、source 关闭或缓冲溢出会返回 `resync_required`，下一次读取先重新
订阅再返回新 snapshot；不会把丢失事件伪装成成功，也不持久化或重发事件日志。
当前事件只有 session active 状态，不预建 agent/tool/approval 全域协议。后续业务接入必须
让 snapshot 覆盖待审批与终态等权威状态，不能选择性静默丢弃；本项不宣称已验证这些业务。

`$/cancelRequest` notification 按数字/字符串 transport ID 取消请求接收；默认请求超时
30 秒。取消、超时、断管、关闭分别返回 typed error，不重发已接受的工作。关闭 stdin
结束在途接收，正常客户端先等待响应再关闭；shutdown 先拒绝新工作并取消在途接收。
app worker 使用独立拥有的 context，不保留库的 pooled request；接收取消不等于实现了
工具、进程树或审批的产品 abort，现有 renderer AbortSignal 仍只在 UI 侧取消接收。
Go 继续使用库原生 Conn/Stream；I/O 生命周期只观察读写错误，parser 终止通过公开
`Conn.Err` 观察并取消等待，不增加协议解析器或自定义 Stream。

针对性证据（2026-10-16，Asia/Singapore）：

- `go -C backend test -race ./internal/app/session/... ./internal/transport/... ./cmd/rozsa-backend/...`：app 水位/溢出/并发、fake app 可观察顺序、取消注册竞态与容量、序号缺口/重同步、半包连包及 UTF-8、实际 server frame 上限拒绝、慢 stdout、断管/非法 payload 和真实 main stdout/stderr/退出状态。
- `TestReplacedSubscriptionCannotDeliverLateSnapshot` 修复前失败（迟到 snapshot 返回成功），修复后通过；替换订阅不能交付旧 snapshot。
- `npm --prefix desktop run typecheck`、`npm --prefix desktop/js-tests run test:session-contract`：当前 Go/TS 错误契约与 main/preload/renderer 错误保留，原有 UI 接收取消和不重发语义。

Electron 的订阅/reload/退出接线见下节 T012；runtime、真实审批/工具取消和长输出容量随各自
切片接入。未运行 native GUI、Linux/Windows 实机或全项目 TEST。正式状态以
`dow task show TASK-T011` 为准。

### TASK-T012：Electron session bridge 与生命周期

接通当前 `subscribe_sessions` / `next_session_event` / `unsubscribe_sessions`，
preload 只新增对应的三个限定方法。main 校验 sender、main frame、可信 URL、参数数量与
subscription identity；导航、renderer 崩溃和关闭立即使旧接收失效，异步释放旧订阅。
renderer 先接 snapshot，再消费连续事件；旧 epoch/重复事件不交付，缺口/溢出透明提示并
重新订阅读取。空闲 long-poll 的 Go deadline 不视为进程故障，订阅启动失败则直接显示。
reload 和 renderer 崩溃后的手动 reload 从同一 Go 进程恢复 active history，不发送
`switch_session` 或启动 turn。权威 snapshot 阻止迟到成功覆盖页面，同时保留用户在途
读取的实际失败；滚动/Thinking 展开仍按当前页面的 session 视图保存，不新增跨 reload 持久化。

握手要求当前五个 session capabilities；版本不符、启动失败、协议/断管/请求超时均保留
typed error，并自动结束自有进程。正常退出等待 `shutdown` acknowledgement 和进程 close；
3 秒未退出则报告超时并终止、回收自有进程。启动过程中退出不会再创建窗口，不自动重启
backend 或重发已接受操作。

已逐段读取旧 `lib.rs` 窗口关闭 teardown→拒绝审批→取消问题、`commands::abort` 的
清空 queued messages→拒绝审批→取消问题→agent abort→结束 Dev Flow session、prompt 错误
追加→交互清理→最终状态，以及 `AgentSession` hooks/消息追加、dashboard shutdown/回收。
当前候选只有只读 catalog/history，不模拟尚未迁移的审批、问题、hook、写盘、dashboard 或
产品 abort；这些顺序与行为测试由对应业务 owner 接入时继续落实。AbortSignal 仍只取消
UI 接收；session 同步停止另外释放订阅，不伪称停止产品工作，不增加自动保存/恢复。

针对性验证（2026-10-16，Asia/Singapore）：

- `npm --prefix desktop run typecheck`、`npm --prefix desktop run package`；
  `npm --prefix desktop/js-tests test` 和 `npm --prefix desktop/js-tests run test:session-contract`。
- `desktop/js-tests/node_modules/.bin/tsx desktop/js-tests/bridge.test.ts`：实际调用 sender/payload
  guard、preload 校验、导航/替换竞态、旧 epoch/重复/缺口、resync、UI 接收取消与快照/读取失败并发。
- `desktop/js-tests/node_modules/.bin/tsx desktop/js-tests/lifecycle.test.ts`：真实 Go 订阅、空闲、
  reload snapshot、closing typed error、幂等关闭；库原生 synthetic peers 覆盖失败握手、请求超时、
  断管和无响应关闭。握手不匹配未自动回收的回归修复前失败、修复后通过；快照掩盖用户读取失败的
  回归亦先失败后通过。没有以源码/配置字符串断言代替行为。
  两个模块测试直接使用 tsx，并接入现有 `devtools/test-migration-build.sh` 供 dow 的 Shell
  adapter 完成检查；当前 dow 不支持直接识别 tsx，不新增 wrapper 或全局 runner 配置。
- host 运行同一 `lifecycle.test.ts --native`：使用 Forge 实际生成的 main/preload/renderer，
  只在测试启动器中截取错误弹框以便完成自动退出；真实 Electron 验证连续 reload、renderer 崩溃后
  手动 reload、Go 崩溃/断管、版本不符/缺 binary/握手超时、关闭超时、启动中退出，所有自有进程回收。
  实际运行时验证 sandbox/contextIsolation、无 Node globals、第二 renderer 的 sender 拒绝、CSP、
  不可信 history 文本、禁止外链导航/新窗口，以及 JSONL bytes/mtime 不变。
- 已有 `npm --prefix desktop/js-tests run test:native-history` 在 host 通过，并通过
  `ROZSA_NATIVE_EXECUTABLE` 对最新打包应用复跑同一场景；测试实例均关闭。

仅 macOS arm64 候选验收；Linux/Windows 实机、完整产品/原生视觉与未迁移业务均未验证，未进入
全项目 TEST。未新增 M012、证据页、报告或包装脚本；正式状态以 `dow task show TASK-T012` 为准。

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
