# SPEC：Electron 单 WebView + Go 完整迁移路线

状态：用户于 2026-10-13 确认“边对照，边实现”，并于 2026-10-17 批准 React+TypeScript UI 路线。任务状态以 dow 为准。创建：2026-10-12；更新：2026-10-17，Asia/Singapore。
分支：`refactor/electron-go-migration`。工作流：`quick / DEV / step`。

## Goal

将 Rózsa 从 Rust/Tauri 迁移为 Electron 单 WebView + Go，完整保留原有功能、业务规则和关键处理逻辑，减少长期重复维护。实施阶段只是内部验证顺序，不是按版本削减功能的依据。

已确定：Electron 单 WebView、Go 产品后端、全部原有功能、处理逻辑等价。用户于 2026-10-13 确认 D1：前端同时转 TypeScript；D2：Electron 与 Go 采用 stdio JSON-RPC。用户于 2026-10-17 更新 D1：产品 UI 采用 React+TypeScript/TSX，按既有功能切片逐步迁移；React 组件是最终 UI 真源，prototype 复用相同组件。保留既有 HTML/CSS、布局与交互语义，不额外重设计产品行为。

执行方式已确定：每次选一个实际模块或纵向功能，读取相关旧源码和已有测试，随即实现对应 Go/TypeScript 逻辑，运行本项针对性验证，完成后再接下一项。当前源码与已有 Git 历史就是参照；取消全库冻结、源码复制、hash 清单、旧产物归档及全功能事前盘点。协议、类型和测试辅助按切片增量建立；不要求统一差异 runner、测量框架或完整全方法 schema 后才开工。

## Scope

- 覆盖五个 active crate 的实际产品能力、共享 UI、CLI、系统集成、原型接线、开发与打包流程。
- 包括当前因设计缺口暂停的功能、未覆盖的错误/取消/空状态、多问题与多选、后台会话和 subagent。暂停不能作为删除依据。
- 保留 JSONL 格式与处理规则，沿用现有配置和凭据解析语义；不引入数据库替代持久化。不读取真实用户凭据做基线。
- 不增加云服务、远程多客户端或新业务功能；不趁迁移重设计产品行为，不复活 retired terminal/TypeScript/process-bridge 实现。
- 缺失设计按既有业务契约补齐，并由用户批准原型变更；设计补齐属于完成原功能的必需工作，不能移到迁移交付之后。
- 现存行为与需求冲突时单列差异，不擅自修复或复制疑似缺陷。影响逻辑的选择在对应模块实施前确认。

## Requirements Trace

| ID | 用户要求或证据 | 路线约束 | 验收 |
| --- | --- | --- | --- |
| R1 | “降低开发维护成本，统一使用 Electron + Go” | 单一 Go 产品运行时；新前端与原型 UI 只有一份；Electron main 不承载第二套业务 | AC-003、013 |
| R2 | “不区分第几版，只要是原本该有的功能，迁移后页必须完整实现” | 功能对照表逐项收口，暂停入口必须恢复；中间 smoke 不算交付 | AC-001、008、012 |
| R3 | “是逻辑，不是具体实现”及 JSONL 示例 | 比较决策、顺序、状态、副作用、错误和持久化；不逐行翻译 Rust | AC-002、004、005、006 |
| R4 | 用户确认实验支持单 WebView | 单 renderer；接管 split、材质、浮层、焦点与全屏职责 | AC-007、009 |
| R5 | 项目跨平台与测试约定 | macOS/Linux/Windows 分别验收；Go、JS 测试归模块；跨语言由 shell 编排 | AC-010、011 |
| R6 | “调整为边对照，边实现的模式。”；无需另做冻结 | 读取当前切片来源后直接实现；覆盖/差异随实现维护；最终仍核对全部原有功能 | AC-001、002、012 |

### 参照与证据边界

- 实施时使用当前模块源码、已有测试与必要 Git 历史作参照；不建立全库快照或源码副本。
- 当前 Tauri UI 只作为旧行为参照；迁移后的前端唯一真源为 `desktop/renderer/`，Electron 构建/运行不依赖 `crates/rozsa-gui`。GUI 现状与边界见 [GUI 架构](../../../docs/gui/ARCHITECTURE.md)。
- [单 WebView 实验报告](../../../docs/migration/evidence/electron-single-webview.md)记录了合成 shell 和用户人工观察。其 separator 可访问性 FAIL、未覆盖的最小化/极限尺寸、Linux/Windows 与完整产品等价仍保持原状态；实验不是生产或跨平台验收。

### 已确认的持久化与退出边界

旧实现核对表明：窗口关闭先执行原生 teardown，再拒绝 pending approvals、取消 pending questions；stop 操作清空当前 queued messages 后依次拒绝 approvals、取消 questions、通知 agent abort，最后结束 Dev Flow session。正常 prompt 结束的错误消息先追加，再清理 pending 交互并发布最终状态。迁移需在对应功能切片验证这些顺序；这段源码记录不等于应用崩溃或退出路径已验收。

Session manager 在内存/index 更新前追加序列化 entry；append 没有显式 fsync，不能推断断电耐久。现有 manager 的互斥仅在实例内；未发现 OS 文件锁或跨进程 owner 协议。多个实例并发 append、header 重写与崩溃耐久的结果未动态验证，不应假定有跨实例协调或安全保证。

### 必须覆盖的功能族

这是功能 owner 索引，不冒充完成验收。每项实施时增量记录实际入口、关键处理规则、差异与证据，未做功能保持可见。最终收口再核对五 crate 的 active 导出能力、GUI IPC/事件、CLI 参数、设置/持久化及设计缺口；未覆盖能力阻止最终交付，不阻止独立模块开工。

| ID | 功能与关键规则 | 当前主要来源 | 目标 owner / 对照证据 |
| --- | --- | --- | --- |
| F01 | 模型配置、provider 选择、项目覆盖、自定义 URL/headers、thinking/context/限额 | `rozsa-model/src/types.rs`、`rozsa-app/src/model_registry/` | Go model/app；registry、thinking、context、rate-limit tests |
| F02 | Anthropic、OpenAI Completions、Responses、Bedrock 的 payload、stream、tool/usage/error | `rozsa-model/src/providers/` | Go model；逐 provider 的请求/分片响应 fixtures |
| F03 | OAuth、PKCE、callback/device flow、刷新/注销、私有环境文件及凭据优先级 | `rozsa-model/src/oauth/`、`credentials.rs`、GUI auth commands | Go model/app；现有 OAuth/credentials tests；真实登录另设人工证据 |
| F04 | Agent loop、串行/并行工具、hooks、终止条件、follow-up、steering、取消 | `rozsa-core/src/{agent_loop,config,events,tool}.rs` | Go core；有因果关系的事件序列、取消及副作用对照 |
| F05 | JSONL 会话、lazy 创建、分支、恢复、重命名、删除、工作目录 | `rozsa-app/src/session/manager.rs` | Go app/session；读取旧 fixture、写入顺序、重新打开和恢复 |
| F06 | 上下文压缩、模型/思考配置 overlay、会话隔离 | `agent_session.rs`、`compaction/`、GUI state/commands | Go app；restore、context、session-config tests |
| F07 | Read/Write/Edit/Bash、模糊编辑、文件锁、diff/evidence、进程树退出 | `rozsa-app/src/tools/`、GUI `git_diff.rs`/`turn_diff.rs` | Go app/tools + platform；真实临时文件、进程取消和错误路径 |
| F08 | 权限模式、deny/ask/allow、glob/regex、shell 分段、信任与持久化、Hint | `rozsa-app/src/permissions/`、GUI permission commands | Go app；现有规则/审批/信任测试，不能换语言后放宽规则 |
| F09 | AskUserQuestion 多题/多选/Other、排队、切 session、取消和迟到响应 | `tools/ask_user_question.rs`、GUI state/commands | Go app + UI；按 session/request identity 回放 |
| F10 | subagent 的范围、上下文、运行/恢复/取消、queue/steering/fork | `rozsa-app/src/subagent/`、GUI commands | Go app；保留生命周期及访问范围，不能只保留静态卡片 |
| F11 | 设置分层、继承/reset、主题、快捷键、capability、skills/resources、扩展 hooks | `settings/`、`themes.rs`、`skills/`、`resources/`、`extensions/`、GUI key_bindings | Go app 与 renderer/native 各归其职；设置和 hooks 错误策略逐项对照 |
| F12 | Dev Flow 发现、配置、dashboard 启停、REST/SSE、重连、回收、状态与详情 | `rozsa-app/src/dev_flow/`、GUI `dev_flow.rs` | Go app；真实 dow contract、watcher/重连/清理验收 |
| F13 | 消息/Markdown/code/diff、工具生命周期、通知与 unresolved error tray | 旧 GUI `app.js`/共享 UI 脚本（行为参照）、`notifications.rs` | `desktop/renderer/` + Go view model；正常/空/长/错误/取消完整状态矩阵 |
| F14 | autocomplete、文件/目录引用与拖放、模型选择、运行中输入、IME | GUI `file_refs.rs`、`app.js`、`runtime/` | UI + Electron system adapter + Go app；输入与异步返回对照 |
| F15 | 原生窗口、主题材质、sidebar、全屏、ready、scene、焦点与草稿/selection/scroll | GUI native host、scene_router、gui_shared | Electron main + 单 renderer；实验基线与生产成对验收 |
| F16 | CLI 目录参数、`-p`、text/json、continue/resume、provider/model/system-prompt/thinking | `rozsa-cli/src/{args,run}.rs` | Go `cmd/rozsa`；参数、stdout/stderr、退出和 GUI 启动语义 |
| F17 | 启动版本检查、PATH/config roots、发布包、开发启动、版本同步、文档 | CLI run、config_paths、`run.sh`、`devtools/` | Go/Electron/build；本机安装启动与三系统产物 |

### 设计缺口必须归零

沿用 [DESIGN_GAPS](../../../prototype/docs/DESIGN_GAPS.md) 的稳定 ID：GAP-001、014（quota/Dev Flow）；002、002B（消息/Markdown）；004、005、006、007（补全/附件/模型/运行中操作）；003A、003B（工具生命周期）；008A、008B、008C（权限与问题）；009、010、011、012（各设置页）；013（异常状态）。

每项记录：原业务来源、当前暂停/受阻状态、原型需补内容、用户设计确认、恢复后的所有入口、后端契约及自动/人工证据。只有恢复和验证后才能移除暂停守卫。保留单一共享组件，不通过复制旧 UI 形成第二套永久实现。

## Design

### 1. 职责与目录（提案）

```text
desktop/
  main/                     窗口、系统能力、Go lifecycle、受控 IPC
  preload/                  限定的 host/product bridge
  renderer/                 新前端与其原型的唯一真源：HTML/CSS、templates、UI/runtime TypeScript、scene/session state 与 adapter
  tests/                    全部 Electron JS/TS 测试：main、preload、bridge、renderer 与 native host
backend/                    一个 Go module
  cmd/rozsa/                CLI；GUI 启动或直接调用 app
  cmd/rozsa-backend/        stdio 产品服务入口
  internal/model/           provider、OAuth、credentials、stream 类型
  internal/core/            agent loop、tool/event/hook 契约
  internal/app/             sessions、permissions、tools、settings、subagent、Dev Flow
  internal/transport/       协议、dispatch、事件输出；不实现业务
  internal/platform/        OS 路径、进程控制等适配
contracts/                  按当前切片需要增补的 IPC/TS 类型与小契约用例
tests/fixtures/             实际跨语言用例需要时才加入的小输入与来源
devtools/                  Go/JS/跨进程构建和验收编排
prototype/                 保留 scene/gallery/mock，复用 desktop/renderer/ 的同一份前端资源
```

Go 依赖方向：cmd/transport → app → core → model。platform 由需要系统能力的模块通过明确接口使用。不要为了模拟 crate 组织创建多个 Go 服务或多个 module。

业务 owner 在 Go：sessions、pending approvals/questions、权限规则、队列、模型、真实事件和持久化。Electron main 只拥有窗口/OS/服务生命周期。renderer 拥有 scene、draft、caret、scroll、展开与 focus；从 Go 消费产品快照，不独立决定权限、会话恢复或工具执行。

`GuiState`（`crates/rozsa-gui/src/state.rs:287`）、GUI commands/events 和 CLI 中的 session/hook factory 必须拆分归属。不能把整个 GUI crate 翻译进 Electron main。

### 2. D1：前端采用 React+TypeScript（用户已确认）

React+TypeScript/TSX 是共享产品 UI 的实现方式。按已有功能切片逐步把 DOM/template 结构和交互迁移为 React 组件，保留既有 CSS、布局、文案、复制/选择、焦点和滚动行为；不借框架迁移重设计产品行为。迁移完成后 React 组件是 UI 真源，不保留并行的手写 DOM 消息渲染实现。Prototype 通过 mock adapter 提供场景数据并复用 `desktop/renderer/` 的同一 React 组件；演示数据不进入生产包。React/Vite 继续使用 desktop 现有 Vite pipeline，不另建第二套 emit/watch 链。通用能力优先复用使用广泛、经过验证且维护积极的现有库；新增依赖应符合当前切片需要，控制体量与后续升级负担。库的默认行为须适配已批准的产品契约。既有 retired TypeScript 实现不作为复用来源。

UI 功能按 TASK 所属切片迁移，不要求一次性重写整个 renderer。每个 UI task 在自己的 files scope 内转换既有 DOM/CSS/交互；具体路径在执行该切片前确定。旧 Tauri 工程继续独立持有其资源，不加载 Electron/prototype 的 React 组件。

流式与历史消息更新按 session/message 粒度提交。已完成消息保持稳定身份，避免无关流式更新重复渲染或解析 Markdown；批量合并 UI 更新时保留业务事件顺序及 error/completed/canceled 终态。候选具备真实 Electron 链路后，按流式输出、长历史滚动和连续输入进行针对性性能观测，再依据观测选择优化。

共享领域输入、command/result、事件变体用明确 interface/discriminated union；不把原有动态对象一律标成 any。TypeScript 类型不替代跨进程 payload 的运行时校验。只为当前功能建立 IPC 类型、Go struct/tag 和实际用例；以同一小输入验证字段与错误语义。后续切片增补自己的契约，不先建立全方法 schema 或通用生成器。

TASK-T064 将本地 TypeScript compiler 设为默认工具链；TASK-T065 将默认开发/package 流程切换为 Electron Forge + Vite。严格类型检查与 bundling 分开；Forge 管理 start/package，Vite 构建 main/preload/renderer 资源。单 sandbox preload、Node/DOM 类型边界、共享 CSS/HTML 同源与无第二套 emit/watch 链属于已批准的设计约束。TASK-T060 先将共享前端源码与前端测试迁入 `desktop/renderer/` 和 `desktop/tests/`；TASK-T013 随后在最终目录转换 core/port 与原型 TS 装载。新原型与 Electron 共用 `desktop/renderer/` 资源。旧 Rust/Tauri 保有 `crates/rozsa-gui/frontend/`、crate 内 JS/Rust 测试及入口；其构建、运行和测试不得读取 desktop、backend 或新原型资源，也不依赖 Node/Vite。Electron+Go 同样不依赖旧工程源码、资源或生成产物。T014 负责 installer，平台任务负责发行验收。具体命令、依赖版本与产物路径见 CONTRIBUTING；真实验收及边界见 [M065](../../../docs/migration/evidence/M065.md)。

分别配置共享 UI/renderer 的 DOM 类型边界与 main/preload 的 Electron/Node 类型边界，启用 strict 类型检查，禁止 renderer 获得 Node 能力。原型与产品包读取同一批共享生成资源，不直接加载 TS，也不维护手写 JS 副本。静态资源加载与模板 URL 必须通过实际页面验收。

每个模块按原有 JS 行为测试完成转换，再进行新宿主接线。Electron main/preload/bridge/renderer/native host 的所有 JS/TS 测试统一归 `desktop/tests/`。类型检查单独运行，运行时测试使用锁定的 TS runner 或受测编译产物。禁止把类型检查通过等同于产品逻辑通过。原型 mock/scene/gallery 脚本随之迁移，但演示数据与代码不得进入生产包。

TASK-T060 在 UI 切片前把当前共享 UI 源码和前端测试迁入最终目录；不要求它们暂留旧目录，也不在完整验收后安排第二次搬迁。随后各 UI 切片直接在 `desktop/renderer/` 实现并通过 `desktop/tests/` 验证。Electron 构建/运行/测试只使用新工程资源；prototype 与 Electron 共享新前端。旧 Rust/Tauri 是自有源码、测试、资源及入口的独立工程，不是新资源的宿主。T060 的 f8aa583 跨工程搬迁及 T013 草稿耦合由 T075 纠正；T013 在最终目录转换 core/port 与原型 TypeScript 装载，仅验证新 Electron 与 HTTP 原型共享资源，不要求旧 Rust/Tauri 加载这些资源。T061 完整验收与批准切换前保持双向独立，现有 Rust/Tauri 源码和 Git 历史供只读行为对照。

已有 DOM、CSS、组件、flow 与业务交互按现状直接迁入 Electron，不以 T004–T008 全套设计审阅作为入口装配或既有界面接线的前置条件；共享原型覆盖不足不代表产品功能不存在。只有新增场景或改变已有视觉/交互的部分，才需由对应任务单独提出方案并在修改原型前取得用户批准。批准只约束该变更范围；未实现的后端能力仍须保留入口并透明报错。

### 3. D2：Electron main ↔ Go 采用 stdio JSON-RPC（用户已确认）

```text
共享 UI → renderer adapter → preload 的受控接口
  → Electron main → stdio JSON-RPC → Go transport → app/core/model
  ← 定向到唯一 renderer 的事件与快照 ← Go 产品状态

Go CLI → 同一个 app/core/model（不依赖 Electron 运行 headless prompt）
原型 → mock adapter → 同一份 UI（mock 不进生产包）
```

默认每个桌面应用实例启动一个 Go backend，所有项目/session 由它统一管理；CLI headless 直接复用 Go app。跨实例访问同一数据目录的既有规则在会话写入任务中对照并测试；不允许 Rust/Go 对照程序同时写同一用户会话。

Go 采用 [`go.lsp.dev/jsonrpc2`](https://pkg.go.dev/go.lsp.dev/jsonrpc2) 的原生连接、`Content-Length` framing 与 JSON-RPC 2.0 消息处理；Electron main 使用 `vscode-jsonrpc`。不手写协议 envelope、framing、额外解析器或非公开库配置。stdout 仅协议，stderr 为日志；磁盘 JSONL 与传输 framing 不混用。

双端实现选型为 Go `go.lsp.dev/jsonrpc2` 与 Electron main 的
[`vscode-jsonrpc`](https://www.npmjs.com/package/vscode-jsonrpc).
Node 侧直接使用其 `StreamMessageReader`、`StreamMessageWriter` 与
`createMessageConnection`，不保留手写客户端 framing 或 transport ID 调度。
2026-10-15 用户补充批准：畸形 JSON、非法 envelope（包括 boolean ID）沿用 Go
库原生的透明报错并终止连接策略；在途请求失败且不自动重试，不额外补发错误
response。测试中的库诊断错误码不表示实际 wire response；合法请求中的业务参数
错误仍返回类型化 RPC 错误。实现与针对性验收记录见 [M010](../../../docs/migration/evidence/M010.md)。

职责边界：库负责标准 JSON-RPC envelope、Content-Length framing、request/notification 区分及标准错误消息编码；项目负责产品业务类型与 JSON tags、已实现方法的显式路由、领域错误到协议错误 data/TypeScript discriminated union 的映射、transport ID 与产品 identity 分离，以及经证据确认且只能用库公开配置表达的必要适配。v1.0.1 的公开配置包括 `WithCodec` 与 `WithPreempter`；文档中没有依据的 framing/parser 配置不视为存在，不得以自定义 Stream 补齐。若后续需求超出库能力，先回主会话裁决，不将该需求写成已实现或已验收。

按该库语义，missing 与 `null` params 都作为无参数处理；具体方法仍须校验必填业务字段。missing 与 `null` ID 都识别为 notification，不产生响应；所有需要结果的产品调用发送带 ID 的 request，notification 只路由给显式声明接受通知的方法。标准 notification 语义与业务字段/方法契约分别归库和项目维护。

每次只为已实现方法建立显式 request/result/error 类型及 method enum/map；Go 明确 struct/tag，不用宽泛 `map[string]any` 代替领域模型。后续方法逐切片映射。事件接入时建立所属 session、turn/request identity、epoch/序号；scene/theme revision 与产品 revision 分属不同 owner。

启动握手检查协议版本和能力后再 ready；事件订阅任务实现先订阅再取带水位 snapshot、重放缓冲及拒绝旧 epoch/重复事件。不承诺持久化事件日志；溢出透明报错并重新同步，审批/终态不静默丢弃。所选库的单 body 上限为 1 GiB；涉及长输出时仍须测定并测试应用层容量、背压与所需的更小业务 payload 限制，不为只读列表预先测量全产品。

库提供 request context 与 `Preempter` 钩子（可用于按序处理取消 notification）；项目负责把明确的产品 abort 路由到 Go 工作、取消对应工具/进程树并保留业务取消语义。AbortSignal 取消 UI 接收不等于停止产品操作，沿用现有 [port](../../../desktop/renderer/scripts/core/ports.ts) 语义。协议库不拥有产品状态恢复：超时、断管或 backend 异常退出只报告失败/中断，不自动重发 Write/Edit/Bash/审批响应。恢复读取已持久化状态；不能伪称在途任务已恢复。

renderer reload 只重订阅和恢复快照，不重新启动同一会话。生命周期及相关业务切片分别读取旧关闭/退出/异常中断逻辑：当前窗口关闭拒绝待审批、取消问题；工作取消、dashboard 关闭和写入/退出顺序随接入核对。Go 保留业务语义，Electron 关闭握手承接进程边界；不能增加自动保存或恢复。超时透明报告并按 OS adapter 清理。CLI stdout 不混入服务 framing。

Electron `contextIsolation: true`、renderer sandbox、`nodeIntegration: false`；preload 只暴露经过校验的能力，不裸露任意 ipc/fs/process。新 IPC sender 与 payload 校验、CSP、渲染不可信模型内容和外部链接处理，作为替代 Tauri bridge 的必要边界测试。保持现有有效内容的呈现语义，不借安全接线删减产品能力。

未采用 loopback HTTP + 事件流：本次没有独立服务/多客户端需求，不增加端口、身份校验和连接管理。Dev Flow 自身已有 REST/SSE 集成继续保留，不因桌面采用 stdio 被删除。

### 4. 逻辑等价的核对方法

每个切片记录所涉及行为的前置状态、输入、决策、事件因果、副作用、持久化、错误/取消和恢复。先读现有测试与源码，然后实现并运行所属模块针对性测试；需要比较实际执行时才用同一小输入在隔离目录运行相关 Rust/Go 路径。既有测试、代码和 Git 历史都可作为依据，不强制为每个模块先写独立 reference runner。允许内部实现不同，不允许改变可观察规则。

JSONL 特别要求：lazy materialization 的实际触发条件；header/entry 类型和字段；ID/关联/leaf；追加成功后更新内存；rename 追加 session_info；cwd 修改重写 header；compaction 留存历史且恢复有效上下文。保留当前 header 错误、空行和 malformed entry 的读取规则；当前 `open()` 跳过无法解析的 entry，是否修复另行决策，不在移植时默改。

跨语言差异必须覆盖 JSON missing/null、空数组/map、整数精度、Unicode/转义、字段别名、时间、浮点、path/case/symlink、UTF-8 分片、shell 引号/分段、regex/glob 方言与进程退出。不能用 Go 默认解析行为替代对契约的确认。随机 ID 和时间通过注入固定值或保持关联的归一化比较；只归一化真正非确定项，不能抹掉事件先后或字段差异。

`model/types` 集中定义公共 `Protocol`：只要求 `API()` 与 `StreamSimple(...)`；公共 `Stream(...)` 将 `StreamOptions` 转成 `SimpleStreamOptions` 后调用该接口。`model` 维护最小 API→实现构造映射，runtime 只在没有显式注入时请求该工厂，不维护协议白名单或直接构造协议实现。显式注入实现由 runtime 直接使用；未登记 API 且无注入实现时，在统一创建入口返回明确错误。`Model.Provider`、endpoint 与认证仍取自实际配置。core agent 可继续消费仅含 `StreamSimple(...)` 的窄接口，模型层不得依赖 core。

模型协议在对应 provider 任务中按需添加 request/stream 小输入；真实远端输出不作为完全相同基准。保留 provider 独有语义，不能以通用 SDK 覆盖所有实现。Bedrock event stream、OAuth、权限 regex、process cancellation 和打包依赖随所属实现验证，不以综合 spike 阻挡独立模块。

### 5. 单 WebView 的生产接入

采用窗口级 Sidebar vibrancy + 不透明 Main，侧栏浮层需要相应裁切/几何处理。保留透明开关、主题与窗口激活状态的效果；不以 CSS blur 代替窗口背后的系统材质。

将宽度上下限/记忆、分隔线鼠标与键盘/ARIA、折叠、边缘唤出、窄窗口、fullscreen、标题栏拖动/双击、交通灯、文件/目录 picker 与 native drop 明确分配给 Electron 或 UI。不要照搬实验的 DOM separator 可访问性缺陷。scene 切换保持 root identity 和会话 UI state；单 WebView 不再需要双 ready 协调，但最终窗口仍应完整就绪后显示。

macOS 的验收参照原生效果；Linux/Windows 按各自既有产品行为验收，不强求复制 AppKit 材质。所有平台均要求功能、错误、取消、输入和文件路径语义完整。

### 6. 按切片对照实现与最终收口

迁移按下方依赖表从会话读取纵向切片开始，再并行推进真实代码依赖满足的产品模块和对应 UI。每项先核对其旧源码与测试，再实现并做本项针对性验证；不把统一 runner、完整 schema 或事前全功能盘点设为开工门槛。

| 工作单元 | 具体产出 | 验证与推进规则 |
| --- | --- | --- |
| 会话读取切片 T001–T003 | 列表→JSONL只读恢复→会话切换历史链路 | 逐项读取相关旧源码/测试，随即实现并针对性验证；未实现消息细节仍由 T029/T045 负责 |
| 构建/通信增量 T009–T014 | 当前候选 watch、类型化错误/取消、事件同步、生命周期、T013 共享 TS 装载、安装包资源 | 已有切片能运行后按需要完善；完整 schema、通用 feature-host、综合依赖 spike 都不是开工条件 |
| 共享 UI 前置迁入 T060 | 把共享源码和前端测试放入 `desktop/renderer/` 与 `desktop/tests/`，使 prototype 与 Electron 共用前端资源 | 在 T013 和后续 UI 切片之前完成；不等待最终完整验收 |
| Electron 既有完整 UI 入口 T080 | 将既有双 WebView UI 的 DOM、`app.js`、组件、样式及 runtime flow 整合进 Electron `index.html` 的单 WebView；以受控 Electron adapter 接 Go 已实现会话链 | 不等待 T004–T008 整套设计审阅；既有界面直接迁用。真实新增/改变设计单独批准；未实现 Go 方法透明报错 |
| 产品模块与界面切片 | settings/registry/JSONL写入/providers/OAuth/tools/权限/问题/运行会话/subagent/Dev Flow/CLI，以及对应 TS UI | 只等待真实代码依赖；既有 UI 接线不依赖 T004–T008 整套批准，只有新增或改变设计时才要求对应批准。前后端仅在文件与接口无冲突时并行；共享入口、协议、CSS、原型接线与路线图指定单一 owner，并串行交接 |
| 最终完整验收 T053–T059 | 汇接复核、整链路故障/重启、真实授权、三平台安装与 GUI、完整功能核对和收益报告 | 汇总切片证据，按全部旧源码入口和设计缺口找遗漏；真实未验证项保持未完成；性能测量在候选就绪后做 |
| 完整验收后的入口切换 T061–T062 | 默认入口切换、Rust/Tauri生产依赖退役与最终文档 | T061 同时依赖 T059 完整功能验收和 T060 共享 UI 前置迁入；旧生产链保持独立可用到完整验收与批准切换 |

分组是索引，不是额外阶段 gate。无文件冲突且不争用共享接口的前后端任务可以并行；同一任务仍按其声明范围执行。共享 manifest/lock、transport/bridge/入口、CSS、原型接线与 TASK_ROADMAP 每次只由一个明确 owner 写入，完成后串行交接；涉及新增文件先用 dow 更新 scope，不把无关依赖串成全局前置链。

TASK-T013 与 TASK-T079 已完成，分别建立共享 core/port 与原型 TS 装载、以及既有消息/Markdown 组件与 conversation flow 的 TS 真源。TASK-T080 接续现有完整 UI 入口与 Electron host adapter，依赖 T003、T009、T013、T079；真实会话链可用，其他 Go 未实现方法继续透明报错。TASK-T029 依赖 T080，继续负责完整消息状态、失败/取消/恢复和产品业务接线；TASK-T052 在 T080 后接管 native shell/scene/state，T053 做全部 runtime/原型 TS 与生产 adapter 汇接。T080 完成不代表这些任务或整体功能验收完成。既有界面接线不依赖 T004–T008；这些设计任务只约束实际新增或改变的设计范围。

TASK_ROADMAP 的功能覆盖随实现维护；未做功能保持待实现。最终 T055 核对全部 feature/gap/contract 与 owner，T059 确认 Must 和真实验收完成。发现遗漏或需求冲突时明确责任任务；全覆盖是最终交付要求，不是开工门槛。

保留当前 Rust/Tauri 源码和已有 Git 历史作参照，不新增长期 Electron→Rust 桥。候选使用隔离输入，禁止双写用户数据。切片失败回到该项，默认入口保留至完整验收。切换时先验证 staged 包与配置副本；失败恢复入口与匹配副本，不能用旧快照覆盖后续用户新写入。

## Acceptance

- SPEC-AC-001: 对照清单随切片实现维护；最终覆盖所有 active 功能和原有被暂停能力，每项有来源、owner、用例、结果。最终无未分类项且所有 Must PASS；不要求开工前完成全盘点。
- SPEC-AC-002: 相同 fixture 下 Rust/Go 的业务决策、事件因果顺序、状态、副作用、失败/取消/恢复等价；所有归一化规则可审阅，不隐藏差异。
- SPEC-AC-003: Go 统一管理产品状态；renderer/main 不复制权限、会话或 Agent 业务；GUI 与 CLI 复用 app/core/model。
- SPEC-AC-004: JSONL/config/credentials 契约对照通过；既有文件 fixture 可读取、追加、重开和续接；真实数据不用于双写对照。
- SPEC-AC-005: 所有 provider/OAuth/stream/error/usage 行为及模型配置覆盖通过；真实授权验证缺失时明确未完成。
- SPEC-AC-006: 工具、并行/取消、hooks、权限、问题、subagent、queue/steer/fork/compaction 与 Dev Flow 整链路通过，含真实进程退出和 watcher。
- SPEC-AC-007: IPC schema、握手、所选库的 framing/message semantics、背压、request/notification 与身份路由、领域错误映射、订阅/快照竞态、reload、断管、关闭均有测试；副作用请求不自动重试。该验收要求不表示实现已通过。
- SPEC-AC-008: 全部设计缺口恢复且用户已批准相关原型；完整状态矩阵通过；新原型和 Electron 产品保持一份 UI（不包含旧 Rust/Tauri），生产包不含 mock/gallery。
- SPEC-AC-009: 单 WebView 原生视觉与交互成对检查通过；separator ARIA/键盘、最小化、极限宽度等实验遗留项全部验收。
- SPEC-AC-010: macOS/Linux/Windows 在各自系统验证 CLI、文件/进程/凭据/输入/窗口；交叉编译不能替代真实 GUI 验收。
- SPEC-AC-011: Go 与 JS/TS 单元/特性测试各归模块；跨语言 shell 集成；新增 bug 修复有先失败后通过的回归证据；TEST 阶段仅按用户显式请求进入。
- SPEC-AC-012: 首条 smoke、局部迁移或平台 skipped 不算完成；功能矩阵、设计缺口和必需未验证项均归零才交付。
- SPEC-AC-013: 最终安装包/默认开发入口运行 Electron+Go；不依赖 Rust/Tauri；原型、README、AGENTS、CONTRIBUTING、架构/测试/版本脚本及 backlinks 同步。
- SPEC-AC-014: 候选就绪后对开发启动、修改到验证、构建、安装包/内存作相同场景的新旧记录，注明可用旧产品或 Git 历史参照与条件差异；维护收益有 owner/重复实现减少的依据，不预先声称性能提升。无可用比较明确缺测；阈值经用户确认，未确认只报告数值。测量不阻塞具体功能开工。
- SPEC-AC-015: 共享 UI、runtime/原型脚本与 Electron main/preload 完成 TypeScript 迁移；严格类型检查和原有运行行为测试分别通过；新 Electron 产品与其原型读取同源生成资源，无手写 JS/TS 双真源，生产不携带 mock。

## Risks

| 风险 | 处理 |
| --- | --- |
| 当前可见 UI 不是完整功能集 | 实现时对照源码/测试/GAP并维护覆盖；最终完整核对，缺口必需恢复 |
| Rust/Go 序列化、regex、并发语义不同 | 当前切片就地验证契约与所需依赖；实际失败输入加小回归用例 |
| TS 转换与宿主迁移同时改变路径/加载/类型 | 分模块保留行为测试并验证资源实际装载；分开 DOM 和 Node 类型边界 |
| GUI crate 中藏有产品编排 | 对 commands/state/events/CLI factory 逐项分 owner，不机械翻译文件 |
| 进程拆分引入重复执行和退出泄漏 | 确定请求 identity、epoch、取消和退出契约，禁止自动重发副作用 |
| 单窗口实验不等于全产品验收 | 保留实验限制，补原生、可访问性、真实业务及三平台证据 |
| 真实模型/OAuth/其他平台暂不可用 | 明确 NEEDS_CONTEXT/UNVERIFIED，继续独立项；不能把缺证据写成完成 |
| 原有测试含结构/源码路径断言 | 保留业务断言，解释技术实现变化后替换结构断言；不能为迎合新实现削弱行为要求 |
| Rust 被提前删除导致失去参照 | 完整验收前保留现有源码；已有 Git 历史一直可参照，不另建冻结链 |

## Test Plan

1. 先读现有测试与 fixture，不运行全面生产测试来装饰规划结论。按模块建立差异测试；例如 `agent_session_restore_test.rs`、`permissions_test.rs`、`bash_abort_test.rs`、`settings_test.rs`、`dev_flow_real_dow_contract_test.rs`、provider 和 GUI tests。
2. Go 特性用所属 package 的 `_test.go`；Electron 的 JS/TS 测试集中于 `desktop/tests/`，其他模块测试仍归各自模块。只有跨语言/跨进程验证确有需要时才增加 shell 编排。fixture 和预期结果就近放置；跨语言确需共享时放 `tests/fixtures/`。长期保留独有决策和确需追溯的真实验收结果；不读真实凭据，不归档无关内容。
3. 核对持久化 bytes/结构、模型请求、流式分片、事件因果、文件变化、权限结果、进程退出。固定 clock/ID/provider；并行事件只比较合同要求的先后，不把 scheduler 偶然顺序写成业务规则。
4. 真实 GUI 对照在相同主题/尺寸/状态下看截图与动态操作；DOM/截图不替代 IME、drag、native blur、全屏和崩溃清理。测试实例结束立即关闭。
5. macOS watcher/FSEvents 必须 host 验证；其他系统分别执行平台 fixture/集成及 GUI。失败先归因现存问题或本次改动，不 reset/stash 共享工作区。
6. build、format、lint、race、package smoke 的操作入口集中维护在 CONTRIBUTING/脚本。进入全项目 TEST 仍需用户显式请求。

## Self Check

- [x] 目标和完整功能边界明确；没有 MVP 或缺口延期交付。
- [x] 路线退出条件、旧逻辑保护、进程边界和切换门槛明确。
- [x] 单 WebView 结论与实验限制分开记录。
- [x] SPEC 已获用户确认，转 quick/TASK 分解；未写生产代码。
- [x] D1、D2 已由用户于 2026-10-13 确认：同时转 TypeScript，采用 stdio JSON-RPC。
- [ ] 全部原有功能最终核对与验收完成（随切片增量维护，最终收口；不是开工前置条件）。
- [x] 初次独立路线审阅：无 P1，已修正长期 fixture/临时副本的 P2 文档矛盾。
- [x] TypeScript 范围更新后的既有独立复核完成；该历史审阅不代表本次路线调整或产品验证已通过。

## Related Docs

- [正式任务与切片索引](../../../docs/migration/TASK_ROADMAP.md)：任务顺序、依赖和功能 owner；正式状态以 dow 为准。
- [GUI 架构](../../../docs/gui/ARCHITECTURE.md)、[设计规范](../../../DESIGN.md)、[设计缺口](../../../prototype/docs/DESIGN_GAPS.md)
- [贡献与测试约定](../../../CONTRIBUTING.md)、[Electron 实验报告](../../../tmp/electron-native-validation/verification.md)
- 官方依据：[JSON-RPC 2.0](https://www.jsonrpc.org/specification)、[Electron IPC](https://www.electronjs.org/docs/latest/tutorial/ipc)、[Electron security](https://www.electronjs.org/docs/latest/tutorial/security)、[Go encoding/json](https://pkg.go.dev/encoding/json)。当前依赖版本以 manifest/lock 为准。库提供 framing 与 JSON-RPC envelope；产品方法、业务类型/错误、事件 epoch/watermark 和恢复仍由本项目定义与验收。
