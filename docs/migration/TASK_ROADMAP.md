# Electron + Go 按切片迁移任务索引

进入日期：2026-10-13，Asia/Singapore。

用户已决定“边对照，边实现”。正式任务状态、范围与依赖以 dow 为准；本页只维护任务顺序、功能 owner 和覆盖责任。

## 执行方式与边界

本路线保留迁移决策、任务依赖顺序与功能覆盖 owner。实现操作、构建/测试命令、工具版本和输出位置见 [CONTRIBUTING](../../CONTRIBUTING.md)；迁移决策及验收原则见 [SPEC](../../.dev-doc/refactor/electron-go-migration/SPEC.md)。

逐项覆盖根据实际实现更新；未完成能力保持可见，完整功能、逻辑等价、设计批准及跨平台验收均是最终交付要求。真实平台、原生或授权尚未验证时保持未验证；Rust/Tauri 生产链只有完整验收后才可退役。

## 功能与验收约束

保留 Electron 单 WebView + TypeScript + Go、stdio JSON-RPC、全部原有功能与处理逻辑等价、原 JSONL 写入/恢复规则。最终完整核对全部旧能力，真实平台/原生/授权未验证时不得宣称通过。

## 已完成的会话模块切片

**TASK-T001：打通 Electron→Go 会话列表读取纵向切片**，无前置依赖。已实现
Go catalog、stdio framing/dispatch、受控 Electron/preload/renderer 与候选构建；
验收结果见 [M001](evidence/M001.md)，正式任务状态以 `dow task show TASK-T001` 为准。

覆盖责任：T001 会话列表；T002 JSONL 只读树与有效上下文；T003 Electron 切换与只读历史；T029 React 消息状态及 Electron→Go 发送/续写/流事件/取消/错误接线。T045 继续负责其余消息 UI 状态，T037/T049 负责完整运行会话。各切片的真实结果与未验证项见对应保留的 M001–M003及本路线图 F13 记录。

**TASK-T002：JSONL 只读打开与有效上下文恢复** 已实现 Go `Open`、
`Entries`、`ContextMessages`，复用 catalog 的 header/entry 校验。按最后有效
entry 选择 leaf，通过 `parentId` 回溯分支，保留历史与有效上下文的区别；
最新 compaction 按 `firstKeptEntryId` 裁剪上下文，不生成额外 summary message。
同一组小 fixture 在 Go 和当前 Rust `SessionManager` 下对照，详见
[M002](evidence/M002.md)；正式完成状态以 `dow task show TASK-T002` 为准。
T003 负责 Electron 切换/历史链路，T017 负责完整写入 manager，T028 负责实际
压缩与配置 overlay；本项完成后停止，等待用户安排。

### TASK-T017 JSONL manager

Go `session.Manager` 已在 T002 reader 上补齐 `Create` / `CreateLazy` /
`OpenManager`、全部七类 entry 的 append、短 ID/index/leaf、rename 的
`session_info` 追加、cwd header 重写、有效上下文复制续接分支和 delete。
依据为当前 Rust `session/manager.rs`、`types_serde.rs`，以及既有 reader、
working-directory 和 agent-session-restore 测试；没有第二套解析器。
规则与 API 见 [app SessionManager](../app/README.md#sessionmanager)。

2026-10-17 针对性验证：`go -C backend test -race ./internal/app/session/...`
及 `go -C backend test -race ./...` 通过；Rust
`cargo test -p rozsa-app --test session_reader_reference_test --test working_directory_persistence_test --test agent_session_restore_test`
通过（5 tests）。Go 模块测试覆盖旧 fixture 副本读取→追加→重开→续接、
header/malformed/空行、所有 entry/message 字段、lazy 的 append 与不同 cwd
触发、追加成功后更新内存、rename/cwd/delete、压缩后的分支复制、部分复制失败、
真实文件写入拒绝、短写/底层取消/close 错误、截断尾行和实例访问隔离。
非法 UTF-8 写入已有先失败后通过的回归测试，避免 Go 编码器静默替换数据。

保持 SPEC 的持久化边界：无 fsync、原子 header 替换或跨实例锁；Go 显式回报
close 错误，失败不推进内存，但磁盘可能已改变，不自动重试或修复截断。
独立实例可保持旧 leaf 并写成 sibling branches，重开只取最后有效 leaf；
这是已验证的旧快照限制，不是跨实例冲突解决或取消/崩溃恢复成功。
timestamp 相同的名称/custom 选择顺序未定义。桌面 RPC 仍只读；产品取消、运行
owner 和实际压缩/配置 overlay 分别留给后续任务。只使用临时 fixture 副本，
未读写真实用户数据；未进入全项目 TEST、未做 Linux/Windows 实机验证。

**TASK-T003：会话切换与只读历史恢复 TS 链路** 已接通受控 renderer → preload
→ main → stdio RPC → Go LoadedSessions/Reader。Go 按 path 复用 Loaded tab，
展示全部 active-branch 消息，模型有效上下文保留在 Go；renderer 保存本切片的
滚动/Thinking 展开，拒绝迟到响应，读取失败保留当前历史并支持手动重试。
TS/Go 聚焦测试和 macOS 隔离 native 验收见 [M003](evidence/M003.md)。
T029 已把消息、Markdown、image/tool 历史呈现迁入共享 React 真源，并接通真实消息运行链；T045/T051 的其余 UI 状态及 T037/T049 的运行会话仍待实现。本项不是完整迁移交付。正式状态以 `dow task show TASK-T003` 为准。

## 任务与实际依赖

### TASK-T018 Anthropic provider

Go `model/anthropic` 已实现 `Stream` / `StreamSimple`、Messages endpoint 与
payload、API-key/OAuth/Copilot/Cloudflare auth、headers/compat/session affinity、
图像/非 vision 占位、signed/redacted thinking、tool schema/name/result 转换、
cache/metadata/temperature/max tokens 与 adaptive/budget thinking。model 层新增
消息、选项和 sealed event variants；不依赖 app/session。SSE 处理 UTF-8 字节
分片、CRLF、多行 data、EOF flush、sparse block index、thinking/signature/tool
delta、usage 的缺省保留与显式零覆盖、cost 和 stop reason。同步 emitter 提供
背压，由调用者保持响应；context 取消/超时关闭 HTTP 读取，不自动重试。

对照来源为当前 Rust `providers/anthropic/{mod,payload,stream}.rs`、
`providers/common.rs`、`http_client.rs`、`types.rs` 与既有
`tests/{types_serde,oauth_anthropic}.rs`。Rust 尚无 Anthropic payload/stream 专用
测试；Go `testdata/request.json` 是按源码推导的 payload 期望值，`tool.sse` 是
人工合成的协议 fixture，均非真实远端录制，也不声称跨语言双执行逐事件对比。

2026-10-17（Asia/Singapore）针对性验证：

- `go -C backend test ./internal/model/anthropic/...`、Go 全模块
  `go -C backend test -race -timeout 60s ./...` 与
  `go -C backend vet ./internal/model/...` 通过。
- `cargo test -p rozsa-model --test types_serde --test oauth_anthropic`
  通过（14 tests）。Go fake HTTP provider/单字节 body 覆盖 payload、四种 auth、
  thinking off/unset/adaptive/null/budget fallback、cache/compat、图片和连续 tool
  results、完整事件顺序与快照隔离、UTF-8、空流/畸形/截断、tool/max-token stop、
  refusal/error、401/403/429/500/503、请求前/headers 前/流中取消及超时。
- metadata/compat 非法 UTF-8 回归已实际观察到修复前失败、修复后通过；编码前
  拒绝非法 UTF-8/JSON，不静默替换。初次取消 fixture 的 handler 未读完 POST
  body，清理等待超时；修正 fake handler 后通过，不归因为产品或 sandbox 缺陷。
- 完成 hook 的既有 packaged-app 检查在 sandbox 内未取得启动诊断（I007；
  I004/I005 已有同样记录）；同一未修改检查在 macOS host 权限下通过，输出
  `Forge packaged resources PASS`。未变更该检查或降低验证要求。

显式差异：Rust 逐网络 chunk 的 lossy UTF-8 解码可能损坏分片字符；Go 保留完整
字节行再解码。Rust 允许缺少 message_start 的空流成功、EOF 仅处理 message_stop、
异常 index 可能别名到既有 block、最终畸形 tool JSON 可退成 `{}`；Go 对这些异常
报错，未知 block 隔离忽略且不污染已知 block。Rust 的 refusal/sensitive/未知 stop
会发 reason=Error 的 Done；Go 发 Error 终态。Go 必须收到 start/stop、内容块必须
关闭，取消为 Aborted、deadline 为 Error，错误不伪造 Done。这些防护遵循本项
error/cancel 不伪造 final success 的验收要求；正常流事件顺序和字段按源码对照。

凭据文件、shell profile 与完整 provider 环境解析由 T024 补齐，OAuth
login/refresh/logout 由 T032 负责；T018 仅支持显式 key 与注入的 Anthropic/
Cloudflare 环境查找。产品注册、agent owner 与 session 持久化接线留给 T020。
没有读取真实凭据、请求真实 provider 或做 Linux/Windows 实机验证；没有进入
全项目 TEST。其他三个 provider 仍待对应任务。本项完成后交还 TASK_ROADMAP
写入权给 T004；前端设计文件与本项产品文件独立。

### TASK-T019 Go core agent loop

Go core 位于 `backend/internal/core/{agent,tools,hooks}`。`agent.Run` / `Continue`
驱动 Provider stream、消息上下文、tool batch、轮间更新、steering/follow-up 与
终止条件；typed AgentEvent 保留消息和工具的 start/update/end 生命周期。parallel
工具并发上限为 10，ToolExecutionEnd 与 ToolResultMessage 按调用输入顺序整理，
不把 goroutine 调度顺序当行为契约。串行和 parallel 都使用同一工具参数规范化、
hook 和错误结果路径。null 顶层参数转 `{}`；schema coercion 覆盖 number/integer、
boolean、string、array、object 及 anyOf/oneOf。工具错误、pre-hook 错误/拒绝、
取消都生成显式 error result；post-hook 可覆盖内容、错误和 terminate。Provider 与
Tool 接收 context cancellation；取消中的 partial assistant 保留为 Aborted 终态。

行为对照来源为 Rust `crates/rozsa-core/src/{agent_loop,config,events,messages,tool,coerce}.rs`
和 `crates/rozsa-core/tests/{agent_loop_test,parallel_null_args_test}.rs`。专用 Go 测试
使用 fake provider/tool 和固定 clock 验证并行结果顺序与串行结果、steering/follow-up
上下文、stop 条件、tool 取消、局部错误、hook 拒绝/失败/覆盖、null 参数和 coercion。
针对性验证：`go -C backend test -race ./internal/core/...`。这些测试固定关键因果和
可见副作用，不比较并行工具的偶然调度完成顺序。

本切片未连接 app/session、permission owner、具体工具、CLI 或 renderer；没有对真实
provider、OS 工具或 Linux/Windows 实机做验证。hook/tool 的协作式取消依赖实现遵守
Go `context.Context`；core 返回取消终态不构成外部副作用回滚保证。剩余生命周期、
权限、AskUserQuestion、compaction 和产品接线由 T020、T025–T028、T035、T037–T038
继续负责。

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
- `npm --prefix desktop run typecheck`、`npm --prefix desktop/tests run test:session-contract`：当前 Go/TS 错误契约与 main/preload/renderer 错误保留，原有 UI 接收取消和不重发语义。

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
  `npm --prefix desktop/tests test` 和 `npm --prefix desktop/tests run test:session-contract`。
- `desktop/tests/node_modules/.bin/tsx desktop/tests/bridge.test.ts`：实际调用 sender/payload
  guard、preload 校验、导航/替换竞态、旧 epoch/重复/缺口、resync、UI 接收取消与快照/读取失败并发。
- `desktop/tests/node_modules/.bin/tsx desktop/tests/lifecycle.test.ts`：真实 Go 订阅、空闲、
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
- 已有 `npm --prefix desktop/tests run test:native-history` 在 host 通过，并通过
  `ROZSA_NATIVE_EXECUTABLE` 对最新打包应用复跑同一场景；测试实例均关闭。

仅 macOS arm64 候选验收；Linux/Windows 实机、完整产品/原生视觉与未迁移业务均未验证，未进入
全项目 TEST。未新增 M012、证据页、报告或包装脚本；正式状态以 `dow task show TASK-T012` 为准。

TASK-T075 负责恢复双向独立；T009 建立候选资源构建与 watch 基础；T060 在 UI 切片前把共享前端源码和测试迁到最终目录，T013 随后负责 core/port TS 转换与原型同源装载。

T060 的 f8aa583 将旧源码/测试搬入 desktop 并让 Tauri 使用新资源，破坏了双向独立边界；TASK-T075 纠正此范围。旧 Rust/Tauri 恢复 `crates/rozsa-gui/frontend/`、`js-tests/`、本地 Rust 测试及 `index.html`/`sidebar.html` 静态入口，`run.sh` 不构建 Vite。新工程保留 `desktop/renderer/` 和 `desktop/tests/` 成果，Vite 只构建 Electron 候选入口。两边构建、运行与测试不得读取对方源码或产物；单一真源仅限新前端及其原型。

T060 已完成记录不能直接由 dow 修改，当前边界以 T075 与修订 SPEC 为准。原型专属检查从旧 Cargo suite 转入 desktop/tests，旧原生行为断言继续读取 crate 自有资源。Rust/Go 的会话 fixture 各自保存，不跨工程读取。

T013 在 T075 完成后恢复执行：范围是 `desktop/renderer/` 与 prototype 的 TS core/port、模板装载和入口接线。HTTP 原型与 Electron 共享新前端生成资源；旧 Rust/Tauri 继续使用 `crates/rozsa-gui/frontend/` 自有源码和入口，不加载 desktop 资源。相关 host-contract、HTTP template fixture、类型与真实加载验收归 T013；本任务不新增报告、证据页或包装脚本。

此前 T060 的 macOS 验收属于当时耦合布局的历史记录，不能证明当前双向独立；T075 在缺少另一工程的隔离树验收：旧 Rust CLI/staged Tauri 构建、全部 GUI Rust 测试、58 项 JS 与会话恢复对照通过；实际窗口显示主界面与 sidebar。新树 TS7、Forge package、资源/source maps、Go 与 97 项 JS/TS 通过；真实 Forge CSS/TS/HTML 更新、Go 复用与退出清理，以及 packaged 会话历史通过。验证实例均已退出。Linux/Windows 实机与完整产品验收仍未执行。


T064 将本地 TypeScript compiler 设为默认工具链；T065 迁移为 Forge/Vite 开发与可运行 package；T014 负责后续 installer/发行资源验收。

T065：Forge/Vite 开发与可运行 package（依赖 T064）。
工具版本、构建分工、运行方式与产物路径见 CONTRIBUTING；真实验收与未验证项见 [M065](evidence/M065.md)。T060 先迁入 `desktop/renderer/` 与 `desktop/tests/`，T013 在该目录继续共享 TS/templates 装载，T014 增加 Forge makers 并验收 installer 内的 UI/Go 资源及启动失败诊断；签名和各平台安装卸载由 T056/T057/T058 验收。

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
| TASK-T060 | 建立新 UI/测试最终目录；跨工程搬迁由 T075 纠正 | TASK-T009、TASK-T065 |
| TASK-T013 | 转换新前端 core/port 与原型 TS 装载并验证生成资源 | TASK-T060、TASK-T075 |
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

#### TASK-T016 当前覆盖

Go `model/types` 提供 provider/API/model/cost/thinking effort 和 quota window 类型；
`app/modelregistry` 支持生成模型载入、用户目录后项目目录覆盖、自定义模型、URL/headers、
thinking map、context、来源标记、配置诊断与事务式 refresh。未配置 thinking map 与映射中
显式 `null` 保持不同语义。刷新失败返回错误并保留旧 registry；model selection 留在 settings，
refresh 不改选中值。这里实现 quota 数据类型，不包含网络 quota 查询。

针对性验证：`go -C backend test ./internal/model/types/... ./internal/app/modelregistry/...`。
覆盖 thinking 默认/null、项目覆盖、headers/context、自定义模型、空 registry、坏文件诊断及
refresh error。完整 provider/OAuth/stream 行为和真实授权仍由后续任务及 T053–T059 验收。
| TASK-T025 | 移植 Read/Write/Edit、模糊匹配/锁与 diff evidence | TASK-T019 |
| TASK-T026 | 移植 Bash/命令分段与三平台 process tree 取消 | TASK-T019 |
| TASK-T027 | 移植 AskUserQuestion 多题/多选/Other、队列与迟到处理 | TASK-T019 |
| TASK-T028 | 移植 compaction、有效上下文恢复与 model/thinking overlay | TASK-T017、TASK-T019 |
| TASK-T030 | 移植主题定义、解析、存储与设置生效的 Go 契约 | TASK-T015 |
| TASK-T031 | 移植快捷键定义、验证、冲突和持久化的 Go 契约 | TASK-T015 |
| TASK-T032 | 接通 Anthropic OAuth login/refresh/logout app 生命周期 | TASK-T018、TASK-T024 |
| TASK-T033 | 接通 OpenAI Codex OAuth、模型刷新与兼容版本同步 | TASK-T022、TASK-T024 |
| TASK-T034 | 接通 GitHub Copilot OAuth/device 与可用模型流程 | TASK-T021、TASK-T024 |
| TASK-T091 | 按实际能力提供 Transport 设置 | TASK-T036 前置补充 |
| TASK-T092 | 统一模型凭据、目录刷新与桌面更新 | TASK-T016、TASK-T024、TASK-T032、TASK-T033、TASK-T034 |
| TASK-T035 | 移植 permission rule/mode/approval/trust/Hint 的安全决策 | TASK-T015、TASK-T026 |
| TASK-T037 | 移植后台会话、queue/steering/fork 与生命周期 owner | TASK-T020、TASK-T035、TASK-T027、TASK-T028 |
| TASK-T038 | 移植 skills/resources/extensions、capabilities 与 hook 执行语义 | TASK-T015、TASK-T019、TASK-T035 |
| TASK-T040 | 移植 subagent scope/context/运行恢复取消及事件路由 | TASK-T025、TASK-T026、TASK-T037 |
| TASK-T041 | 移植 Dev Flow discovery/dashboard/REST/SSE 重连回收 | TASK-T015、TASK-T026、TASK-T037 |
| TASK-T044 | 完成 CLI 全参数、GUI 启动与所有 slash/session 操作 | TASK-T009、TASK-T017、TASK-T020、TASK-T021、TASK-T022、TASK-T023、TASK-T032、TASK-T033、TASK-T034、TASK-T025、TASK-T026、TASK-T035、TASK-T027、TASK-T028、TASK-T037、TASK-T040、TASK-T038、TASK-T041、TASK-T030、TASK-T031 |

#### TASK-T028 当前覆盖与边界

Go `internal/app/compaction` 对照 `rozsa-app/src/compaction/mod.rs`、
`agent_session.rs` 与 `context_window_test.rs`，提供阈值/目标比例校验与 token
limits、serialized-entry 估算、provider context usage 优先触发、tool-call/result
安全边界、图片数据摘要脱敏，以及可取消的摘要执行。摘要失败、取消或 append
失败时不写 compaction entry；成功时以 `AppendCompaction` 追加 `firstKeptEntryID`
作为持久化提交点，不改写旧历史。恢复后的 `Manager.ContextMessages()` 沿用
session reader 的最新 compaction 边界，只还原有效上下文。`internal/app/sessionconfig`
按活动分支顺序重放 model/thinking overlay；切换先写 session entry 再返回新状态，
模型变化按 Rust 合同向下规范化 thinking effort，无法支持时落到 Off。各会话状态独立。

针对性验证：`go -C backend test ./internal/app/compaction/... ./internal/app/sessionconfig/...`。
覆盖比例与非法 context、provider usage、tool 配对、历史保留/重开恢复、摘要失败与
取消不推进 leaf、overlay 顺序与重开、会话隔离及 unsupported effort 降级。Go compaction
包提供摘要回调边界；运行中任务中止、provider stream 与 renderer 状态接线留给 runtime
接入切片，未在此包内实现。

#### TASK-T030 当前覆盖与边界

Go `internal/app/themes` 对照 `rozsa-app/src/themes.rs` 与 `theme_test.rs`，以公开
`Store`/`Appearance` 接口提供内置主题、global/project 分层读取、列表去重、项目层覆盖、
global 写入、自定义 JSON 解析与 CSS 变量验证。文件缺省字段从对应内置 light/dark 定义
继承；`ThemeDefinition` 不持有 `translucentSidebar`，该偏好仍属于 settings appearance。
`Appearance` 通过窄 `SettingsOwner` 接口与 T015 settings manager 组合；主题选择先验证
定义，再持久化 scope override，成功后读取完整双主题 snapshot。mode、字号和 sidebar
透明设置也在持久化成功后返回 snapshot。调用方负责平台原生外观与 renderer 发布；本模块
不实现 Electron/native 外观，也不负责系统明暗选择。

针对性验证：`go -C backend test ./internal/app/themes/...`。覆盖内置值、主题 list/get/save、
层级覆盖与全局写入、变量与 CSS 验证、JSON/UTF-8/读写错误、设置更新顺序、失败时不写、
重开后恢复。Rust 当前将主题文件 mkdir/write 的 I/O 错误也包装为 `ThemeError::Read`；Go
保留相同错误类别和路径信息。模块没有异步或取消入口，取消行为不适用、未测试。详见
[M061](evidence/M061.md)。

#### TASK-T031 当前覆盖与边界

Go `internal/app/keybindings` 对照 `rozsa-gui/src/key_bindings.rs`、
`rozsa-gui/tests/key_bindings.rs` 与 Rust 前端固定快捷键定义，公开 7 项可编辑
动作和有序固定快捷键列表。可编辑项保留默认键名、标题、描述与 global/composer/
outsideComposer scope；global `key_bindings.json` 只存覆盖值，缺省 action 继承默认值，
update/reset 返回有效列表。校验沿用 Ctrl/Meta/Alt/Shift 拼写、特殊键名与 F 前缀规则，
冲突按 ASCII 小写规范化；坏文件、非法输入、冲突和写入失败显式返回。固定快捷键不参与
可编辑绑定冲突检测。Electron native accelerator 与 capture/settings UI 接线留给 T047。

针对性验证：`go -C backend test ./internal/app/keybindings/...`。覆盖默认与固定定义、
scope、缺省继承、不创建文件、override-only 落盘、reset 恢复、无效输入/冲突不改变状态、
文件版本/JSON/action/重复绑定错误、平台键名（含 F11）和写入失败。模块同步执行且无取消
入口，取消不适用、未测试。macOS 宿主测试通过；Linux/Windows 实机未验证。详见
[M062](evidence/M062.md)。

#### TASK-T036 当前覆盖与差异

2026-10-18 批准 T2 + M1：Transport 按实际能力提供 `auto`/`sse`，WebSocket 两项可见但禁用并标明未实现；既有配置不静默改写（TASK-T091）。模型目录继续沿用 Rust provider 凭据过滤，配置读取 `<global>/models/` 与 `<project>/models/`，凭据统一为 `<global>/models/auth.json`。列表可刷新并写回 OAuth 凭据，失败进入桌面异常通知；登录/注销与动态发现只更新对应 provider 的模型，主模型和小模型消费同一目录。公共链路由 TASK-T092 提供，OAuth 登录生命周期由 TASK-T032–T034 接入，T036 依赖这些任务。

Go `internal/app/productsettings` 组合 T015 settings manager 与 T030 themes public contracts，提供 settings snapshot/update/reset、model catalog/selection、theme list/get/save；JSON-RPC、Electron main IPC、sandbox preload 与 renderer adapter 均有 typed/runtime 校验。React+TSX 现承载 General 与 Appearance，支持 global/project 写入、reset、模型/小模型选择及元数据、主题选择/编辑、字号和 sidebar 偏好。Go runtime 在后续 turn 使用新模型、thinking、image blocking、compaction 与 retry settings；首轮命名支持直接标题和已配置 small model。macOS translucent sidebar 由 Electron main 从持久化快照恢复并即时应用/清除 vibrancy。设置读取错误、写入失败、加载、依赖禁用、runtime setter 失败回滚、空模型目录和 scoped override 有针对性测试。

对照来源为 `crates/rozsa-gui/frontend/ui/templates/features/settings.html`、`ui/scripts/features/settings.js`、`desktop/renderer/runtime/app/appearance-settings.ts`、`scenes-settings.ts`、`sessions-models.ts` 和 `crates/rozsa-gui/src/commands.rs` 的设置/模型命令，以及 `crates/rozsa-app/src/agent_session.rs` 的自动压缩与命名。Go catalog 已按 Rust provider 凭据规则过滤，统一 auth 路径与主/小模型分层目录；登录、注销、凭据续期及动态发现通过共享目录更新设置列表并显示诊断，见 [M092](evidence/M092.md)。Transport 已按 T2 只启用 `auto`/`sse`，WebSocket 选项禁用并标明未实现。Rust 与 Go 都保留 steering/follow-up/running-send 设置值，但当前 Agent 执行路径不消费这些偏好；Rust `commands.rs` 仅提供读取/写入，执行路径检索未发现调用。运行中的 queue/steering UI 与状态流由 T049 负责，不属于本切片新增的执行行为。Native vibrancy 与 runtime settings 通过 mock/本地 Go 进程测试；打包 Electron GUI 的真实 macOS 材质效果未验证。

已运行：`bash devtools/test-t036.sh`。该任务局部入口构建 Go backend，运行 Go race 聚焦包、desktop typecheck、tsx 设置/共享设置/bridge/lifecycle/native 测试（33/33），并运行 session-contract 的其他五项（5/5）。完整 `session-contract.test.ts` 曾为 5/6；唯一失败是既有 ISSUE-I020 的初始化前 `getSessions` 错误类型断言，未改测试，局部验收仅排除该不相关用例。此入口由 DOW Shell adapter 直接执行，不修改项目级 `.dev-doc/test.ci`。覆盖 T091 的 SSE/WebSocket 显示与保留、T092 的 provider 目录刷新/诊断、真实 Go 子进程经 Electron preload/main bridge 的 scoped settings/model 读写，以及 provider retry 设置。Packaged app 与宿主 macOS vibrancy 效果未验证；全量 desktop test 未运行。

#### TASK-T020 当前覆盖与差异

`backend/internal/app/runtime/` 将 session manager、模型层工厂创建的 provider、credentials resolver 与 `core/agent` 组合；`backend/cmd/rozsa/` 从 settings 与分层 model registry 选择 provider/model 并提供独立 prompt/continue CLI。`Model.API` 是协议，`Model.Provider` 是提供商身份，两者不构成一一映射。协议公共边界由 `backend/internal/model/types` 持有，最小 API→实现映射由 `backend/internal/model` 持有；runtime 不维护 API 白名单，也不直接依赖各协议实现。显式注入的 agent provider 不经 API 名单过滤；未登记 API 且未注入实现时由模型工厂明确报 unsupported。endpoint、凭据和兼容配置继续来自 registry/实际 model 配置。Prompt 先落盘 user JSONL，再从该 manager 的上下文启动 core continue，避免重复提交 user；流式 text delta 写 stdout，配置、provider stream 与 JSONL 写入错误由 CLI stderr/非零退出暴露。Runtime 同时持有 session app service；transport 的 `ServeApplication` 接收 `session.ApplicationService`，server 只负责 RPC delivery/cancellation。完整 CLI 参数、resume/fork、slash 操作、多会话 queue/steering 和真实授权仍分别留在 T044、T037 与 T054。

#### TASK-T037 后台会话与生命周期

Go `app/sessions.Owner` 是公开抽象，执行 generation、可变会话和队列留在包内；生产 `app/runtime` 实际使用它管理按路径隔离的运行状态、事件与精确取消。切换只改变选中会话，退订/renderer reload 不取消执行；FIFO 在上一 prompt 返回、写入和 pending 交互清理后启动下一条。Steering 保留在 live context，按 Rust 跳过 loop user 的 JSONL 写入；fork 可复制已交付 steering 的用户消息前缀，生成新 entry ID 并记录 source path，重开仍从持久化上下文恢复。

Stop 顺序为清 queue、拒绝该会话审批、取消问题、清 steering 并取消执行、结束活动回调；窗口关闭先拒绝所有审批，再取消所有问题，然后取消各执行并等待 provider 清理与终态 JSONL 写入。JSON-RPC shutdown、断连和初始化晚到均接入同一 owner；两秒关闭期限内未 drain 会明确报错。后台 refresh 保留活动选择与同一 writable manager；写入或响应失败不会被 cancellation 终态隐藏。未新增自动保存或恢复。

对照 Rust `crates/rozsa-gui/src/{commands,state}.rs` 的 send/queue/advance/abort/fork 与 pending 清理，以及 `crates/rozsa-app/src/agent_session.rs` 的 queues、abort、persist_new_messages。已通过 `go -C backend test -race ./internal/core/agent ./internal/app/runtime ./internal/app/sessions ./internal/app/session ./cmd/rozsa-backend ./internal/transport/server`：真实 Go runtime/JSONL 与受控 provider 双会话并发、增量/终态/取消/选择隔离、真实 AskUserQuestion 工具、审批 owner 清理隔离、queue/remove/steer、live fork/重开、写入/响应失败，以及实际 JSON-RPC 双会话关闭落盘后确认。后台 refresh 与 provider drain 均保留修复前失败的回归证据。

本项验证 Go 后端生命周期。审批 policy/工具/UI 整链留在对应任务；subagent 生命周期、真实 Dev Flow dashboard、完整 queue/steering/fork UI 分别仍由 T040、T041、T049 验收，未由 hook 顺序测试替代。I017 的基础消息归属修复独立交付；本项没有运行原生 Electron 端到端验收或 Linux/Windows 实机验证。

#### TASK-T040 subagent 后端运行与重接

Go `app/subagent.Manager` 复用 `sessions.Owner/Execution`，生产 runtime 已注册真实 `subagent` 工具，支持 spawn/send/wait/interrupt/list、独立 system prompt/transcript、工具禁用/递归过滤、scope 先于权限 hook、各自 question identity、按 owner/subagent/session/tool-call 关联的事件及独立 JSONL。Interrupt/shutdown 等待本项启动的工具/hook 清理；后台任务不随发起工具返回而取消。路径规范化及 symlink 检查遵循不越域要求；provider/写入错误透明呈现，未沿用 Rust `had_error = false` 的状态缺口。重建 manager 分配新编号，不覆盖已有 transcript。

已通过 `go -C backend test -race ./internal/app/subagent/... ./internal/app/tools/subagent/... ./internal/app/runtime/...`：真实 production tool→独立 loop→AskUserQuestion→主工具结果/双方 JSONL，独立上下文/重开、cap/运行中拒绝 send、取消/迟到问题响应、scope/禁用/递归/权限 hook 顺序及事件身份；注入现有 Bash 执行器的真实进程取消。旧文件覆盖与 interrupt 早于进程清理返回均保留本次先失败后通过的回归证据。Read/Write/Bash 的生产注册仍由 T089 负责；生产权限 policy/settings 快照链尚未接入，本项仅验证 hook 边界与审批 owner 清理，未声称权限整链通过。

恢复分三种场景：①存活执行：生产消息消费者退订后重取同一 manager，再订阅原 operation、回答独立问题并完成；后端已验证，完整 renderer RPC/UI 表现仍由 T049 验收。Rust `commands.rs:1057` 经 `active_agent`（2934）查询活跃 agent；旧 `frontend/app.js:33` 仍将 subagents/fork/runningSubmit 列为 `UI-FLOW-001` 暂停。②应用重启：manager 元数据不恢复；Rust `state.rs:524` 的 restore→create_agent→`AgentSession::new`（`agent_session.rs:332`）建立空 manager。③旧 JSONL：Go runtime 重建后子 transcript 字节不变且可重开读取，child header 保留 source path；消息文件没有原 scope/system prompt，不将其冒充为原 manager 恢复。持久决策见 [应用层恢复边界](../app/README.md#恢复边界决策2026-10-18)。

2026-10-18 用户决定保持旧行为：不新增跨重启 manager 恢复或 subagent 专属 queue/steering/fork；主会话已有语义继续沿用 T037。Rust `commands.rs:864` 的 queue/steer 与 1111 的 fork 操作选中 tab 的主 agent/消息；subagent loop 配置（527–528）无 steering/follow-up 回调。跨 provider/API 的 manager model override 尚未接 credential/protocol factory，明确拒绝而不误用既有 provider；模型工具 schema 与 Rust 一样不提供此参数。本项验收覆盖 Go 后端；完整运行 UI、原生 Electron 与真实模型/授权均未由本项验证。

#### TASK-T083 模型协议公共边界

`model/types.Protocol` 仅要求 `API()` 与 `StreamSimple(...)`；公共 `types.Stream(...)` 将共享 `StreamOptions` 包装为 `SimpleStreamOptions` 并委托。Anthropic、Completions、Responses 共用该选项适配边界，不各自维护重复 `Stream` 包装。API 到具体实现的最小构造映射只在 `backend/internal/model`；工厂不依赖 core，agent 保持只消费 `StreamSimple(...)` 的窄接口。Runtime 保留显式注入优先，缺省时调用模型工厂并在未知 API 时透明失败。Provider 身份、endpoint 与认证仍由实际配置提供。

验收聚焦共享选项转交、任意 API 配合显式注入实现可走公共 runtime、未知 API 无注入时明确失败；不为每个协议新增持久化或 continue 测试矩阵。

#### TASK-T021 OpenAI Completions provider

`backend/internal/model/completions/` 对照 Rust `providers/openai_completions.rs`、`payload.rs`、`sse.rs` 实现 `/chat/completions` 请求、compat 驱动 payload、merge headers、自定义 endpoint、session affinity、Cloudflare Gateway 与 Copilot headers、SSE/UTF-8 分片、text/reasoning/tool-call delta、finish reason、usage/cache/cost 与 typed stream error。Provider 身份由 model 配置提供；它不要求品牌为 OpenAI。Retry 沿用旧适配器对 408/409/429/5xx 与连接/超时的最多两次重试及 Retry-After 数值延迟。

Runtime 按 `Model.API` 选择 Anthropic、Completions 或 Responses adapter，API key wrapper 面向 provider interface 组合，不再嵌入特定 provider 实现。`bash devtools/test-prompt-path.sh` 以本地 fake HTTP 服务实际运行 CLI：自定义 `openai-completions` provider 请求成功、assistant text 与 usage 写入 JSONL，continue 不重复发送请求，provider HTTP 错误以非零退出和错误消息入 JSONL；同一脚本覆盖 Anthropic prompt/JSONL/continue 及 503 错误。Go provider tests 覆盖 request/header/compat、tool arguments 拼接、response metadata、usage/cost、HTTP/SSE/finish error 和取消。验证仅为本机 fake service；未请求真实 provider，也未验证平台特有服务兼容矩阵。

针对性验证：`bash devtools/test-prompt-path.sh`。该脚本以真实 `go run ./cmd/rozsa` 分别启动 prompt 与 `--continue` 进程，对本地 fake Anthropic SSE provider 发出一次请求；核对 stdout、同一 session JSONL 中 user/assistant、重启后继续读取且不重复写入；同时验证 `$NAME` 凭据缺失和 provider HTTP 503 都导致非零退出及 stderr 诊断。证据目录由脚本输出，位于 `tmp/test-prompt-path-<pid>/`。未读取真实凭据、未请求真实 provider、未完成真实 OAuth 授权，未验证 Linux/Windows 实机。

本切片 continue 遵循 Go core 行为：若恢复后的最后一条上下文消息已经是 assistant，continue 成功无操作；若上次留下 user/tool result，则由 agent core 发起后续 stream。会话选择仅取显式 `--session-dir` 中最近修改的 JSONL 文件；多层 session 目录、resume/fork 和完整参数仍未覆盖。

#### TASK-T022 OpenAI Responses provider

`backend/internal/model/responses/` 对照 Rust `providers/openai_responses/{provider,types,convert,sse}.rs` 实现独立的 Responses 请求与事件协议。运行时由 `Model.API == openai-responses` 选择 adapter；`Model.Provider` 继续提供服务商身份，endpoint、凭据和兼容配置仍来自 registry，故自定义 provider 可使用 Responses，而不会被错误地路由到 Completions。请求转换保留 item-oriented history、tools、reasoning、session affinity 和 account ID header。SSE normalizer 覆盖 UTF-8/任意分片、thinking/text/tool events、usage/cost，以及 completed、failed、incomplete、取消、malformed/unsupported、缺失终态和迟到 event；无效工具参数和 tool result 序列化错误会显式返回。`response.incomplete` 以 `Length` 完成，`response.failed` 以错误终态返回，二者与 Completions 保持独立协议处理。

验证通过：`go -C backend test ./internal/model/responses/... ./internal/app/runtime/...`、`go -C backend test -race ./internal/model/responses/...`、`bash devtools/test-prompt-path.sh`。模块测试覆盖 typed request/history、thinking/tool delta 与终态、cached usage/cost、Unicode 分段、malformed/unsupported/迟到 event、缺失终态及 cancellation。fake HTTP CLI 验收确认自定义 Responses provider 的 `/v1/responses` 请求、model/key/stream/input、流式文本、JSONL assistant/provider/API/response ID/usage 持久化，以及新进程 `--continue` 不重复请求且不改写已完成会话；同一入口中的 Anthropic 与 Completions 成功、错误和续接场景也通过。证据目录：`tmp/test-prompt-path-93763/`。验收仅使用本机 fake service，不代表真实服务商兼容、真实授权或 Linux/Windows 实机验证。

#### TASK-T024 当前覆盖与差异

Go `model/credentials` 按已配置模型 key → provider 环境变量 → OAuth `auth.json`
解析来源；`$NAME` 内部按进程环境 → 私有 `.env` → shell profile 的静态 literal
赋值查找，不执行 shell。auth 写入原子替换并设私有权限，保留同文件的其他
provider 数据；过期 OAuth refresh 可注入、按 auth 文件串行并尊重 context
取消，登出只清理指定 provider。`model/oauth/common` 提供随机 PKCE/state、严格
callback state 校验、仅 loopback callback listener 与 RFC 8628 pending、slow_down
(+5s)、expiry、cancel polling。专用回归测试：
`go -C backend test ./internal/model/credentials/... ./internal/model/oauth/common/...`。

差异/边界：shell profiles 只读取安全的静态赋值；provider OAuth HTTP 登录、token
交换及 refresh 已由 T032–T034 实现，见 [M032](evidence/M032.md)、[M033](evidence/M033.md)、[M034](evidence/M034.md)；真实账号授权仍由
T054 验收。旧 Rust 手动粘贴的裸 authorization code 不含可验证 state，Go helper
仅为兼容解析接口将调用方传入的 state 附于裸 code；HTTP callback/query/hash
输入必须提供匹配 state。T024 的自动测试不代表真实授权或远端 provider 已验收。

TASK-T015 切片覆盖 `backend/internal/app/settings/` 与 `backend/internal/platform/configpaths/`。行为对照 Rust `settings/{schema,merge,storage}.rs`、`config_paths.rs` 及 `settings_test.rs`、`config_paths_test.rs`：层级顺序为 defaults → global → project → local；嵌套字段继承、tools/skills 名称覆盖、移除局部字段恢复继承；`devFlow` 只读取 global；appearance/compaction 校验、缺失配置、读/解析/写错误均由专用 Go 测试覆盖。路径 fixture 验证 macOS/Linux/Windows 风格根、项目 key、两个环境变量和 `.agents/skills` 用户路径，不访问凭据或用户配置。

落盘保持 Rust 的 pretty JSON、写入换行差异（字段更新有换行，`save_global` 无额外换行）及进程 umask 控制的目录/文件权限。Go 用 `map[string]any` 暴露可复用的设置视图；Rust 原实现的公开配置类型仍是强类型结构。平台 fixture 是宿主内的路径规则验证，不代表 Linux/Windows 实机验证。针对性命令：`go -C backend test ./internal/app/settings/... ./internal/platform/configpaths/...`。

### 对应界面的设计审阅（仅覆盖新增或改变设计的部分）

已存在的 DOM、CSS、组件和 flow 直接接入 Electron，不依赖 T004–T008 整套审批；
原型覆盖不足本身不代表该功能不存在。只有新场景或需改变现有视觉/交互的部分，才由
对应任务提出方案并等待批准。

| 任务 | 具体产出 | 前置任务 |
| --- | --- | --- |
| TASK-T004 | 消息、Markdown 与工具缺口设计审阅 | TASK-T002 |
| TASK-T005 | Composer、附件、模型及运行中流程设计审阅 | TASK-T016、TASK-T038 |
| TASK-T006 | 权限 trust 与多问题/多选设计审阅 | TASK-T027、TASK-T035 |
| TASK-T007 | General、capability、快捷键及规则编辑设计审阅 | TASK-T015、TASK-T030、TASK-T031、TASK-T035、TASK-T038 |
| TASK-T008 | quota、Dev Flow 和逐组件异常设计审阅 | TASK-T016、TASK-T041 |

### TS UI 与原生交互切片

T013 和 T079 已完成。TASK-T080 将原双 WebView UI 的 DOM 整合进 Electron `index.html`，
使 sidebar 与 main 同处一个 WebView；沿用既有组件和 flow，renderer 源码按职责拆在
`desktop/renderer/runtime/app/*.ts`，构建时按稳定顺序合并为 classic `runtime/app.js`，以 Electron adapter
对接已实现的真实会话 bridge；未实现的 Go 方法透明报错。T029、T052、T053 按各自职责
在 T080 后继续。T080 的 macOS 单窗口用 Electron 原生 Sidebar vibrancy 作为窗口材质，
只让 sidebar 与标题栏对应区域透出系统材质，main panel 保持不透明；不以 CSS blur 冒充。
T052 继续负责其余 native shell/scene/state 与真实平台完整验收，T080 不代表全部业务
或完整 UI 状态已经验收。

| 任务 | 具体产出 | 前置任务 |
| --- | --- | --- |
| TASK-T029 | React/TSX 消息真源、完整历史状态与 Electron→Go 消息运行链 | TASK-T013、TASK-T003、TASK-T020、TASK-T079、TASK-T080 |
| TASK-T079 | 转换现有会话消息组件与 flow 为 TypeScript | TASK-T013 |
| TASK-T080 | 接入既有完整 UI 到 Electron 单 WebView 产品入口 | TASK-T003、TASK-T009、TASK-T013、TASK-T079 |
| TASK-T036 | 转换 settings/appearance TS 与恢复 General | TASK-T013、TASK-T015、TASK-T016、TASK-T024、TASK-T030、TASK-T091、TASK-T092 |
| TASK-T039 | 恢复权限/Hint/trust 与多问题多选 TS 交互 | TASK-T013、TASK-T012、TASK-T035、TASK-T027 |
| TASK-T042 | 转换 Composer/autocomplete/model picker TS 与输入行为 | TASK-T013、TASK-T012、TASK-T016、TASK-T038 |
| TASK-T043 | 恢复 Skills/Tools capability scope 与 inherited/reset TS UI | TASK-T038、TASK-T036 |
| TASK-T045 | 转换工具/evidence/diff TS 并恢复完整生命周期 | TASK-T013、TASK-T012、TASK-T025、TASK-T026、TASK-T027、TASK-T040 |
| TASK-T046 | 恢复附件/文件引用与三平台 native picker/drop | TASK-T012、TASK-T042 |
| TASK-T047 | 恢复快捷键列表/搜索/capture/冲突/reset TS 流程 | TASK-T012、TASK-T043、TASK-T031 |
| TASK-T048 | 转换 sidebar/Dev Flow TS、恢复 quota 与详情/空列表 | TASK-T013、TASK-T012、TASK-T016、TASK-T041、TASK-T036 |
| TASK-T049 | 恢复运行中 queue/steering/fork/subagent TS 跨组件流程 | TASK-T013、TASK-T037、TASK-T040、TASK-T042、TASK-T046 |
| TASK-T050 | 恢复权限 settings/rule editor 全状态与 TS 真源 | TASK-T035、TASK-T047 |
| TASK-T051 | 转换 notification/popover TS 并逐组件恢复异常出口 | TASK-T012、TASK-T029、TASK-T045、TASK-T042、TASK-T046、TASK-T049、TASK-T039、TASK-T036、TASK-T043、TASK-T047、TASK-T050、TASK-T048 |
| TASK-T052 | 完成单 WebView native shell/scene/state 与原生窗口接管 | TASK-T012、TASK-T046、TASK-T049、TASK-T047、TASK-T048、TASK-T051、TASK-T080 |

### 最终汇接、完整功能与真实验收

| 任务 | 具体产出 | 前置任务 |
| --- | --- | --- |
| TASK-T053 | 完成全部 runtime/原型 TS 与生产 adapter 汇接收口 | TASK-T003、TASK-T044、TASK-T029、TASK-T045、TASK-T042、TASK-T046、TASK-T049、TASK-T039、TASK-T036、TASK-T043、TASK-T047、TASK-T050、TASK-T048、TASK-T051、TASK-T052、TASK-T080 |
| TASK-T054 | 真实 provider/OAuth 授权与远端错误/取消 smoke | TASK-T044、TASK-T053 |
| TASK-T055 | 整链路故障/重启/进程清理与完整等价矩阵收口 | TASK-T044、TASK-T053 |
| TASK-T056 | macos 实际安装包、CLI、原生 GUI 与系统能力验收 | TASK-T014、TASK-T054、TASK-T055 |
| TASK-T057 | linux 实际安装包、CLI、原生 GUI 与系统能力验收 | TASK-T014、TASK-T054、TASK-T055 |
| TASK-T058 | windows 实际安装包、CLI、原生 GUI 与系统能力验收 | TASK-T014、TASK-T054、TASK-T055 |
| TASK-T059 | 完整功能验收与维护收益最终判定 | TASK-T054、TASK-T055、TASK-T056、TASK-T057、TASK-T058 |

### 启动与运行中会话性能基线

| 任务 | 具体产出 | 前置任务 |
| --- | --- | --- |
| TASK-T088 | 独立性能入口覆盖冷/热启动分阶段首绘、全量 1200 条历史的最新首屏、至少 6000 条扩展压力、滚动可达性、会话切换、展开、卡顿和内存；macOS arm64 多轮固定阈值，冷启动端到端不超过 4000ms、热启动不超过 3000ms，任一轮超限即失败。多会话污染及并发负载测试随 T037/T049 等多会话实现后分发。 | TASK-T029、TASK-T087 |
| TASK-T089 | 将 Read/Write/Bash 接入生产 Go runtime；本地假 provider 由显式合成 prompt 触发，以固定 seed 持续循环生成 thinking、文本和工具调用，Bash 只运行临时目录内的合成 CLI。按 1200/2400/3600/6000 条增长检查点记录工具耗时、backend/renderer 与 JS heap/RSS、DOM、帧间隔、长任务、输入和会话切换延迟，分析增长斜率、波动和峰值。普通单元/UI 套件不触发循环负载。 | TASK-T019、TASK-T025、TASK-T026、TASK-T035、TASK-T045 |

### 完整验收后的默认入口切换与退役

| 任务 | 具体产出 | 前置任务 |
| --- | --- | --- |
| TASK-T061 | 切换默认 Electron+Go 开发/发行并退役 Rust/Tauri 生产链 | TASK-T059、TASK-T060、TASK-T075 |
| TASK-T062 | 最终证据/文档/数据与全部 AC 交付闭环 | TASK-T061 |

## 功能覆盖与差异（随实现更新）

下表是 owner 声明，不是全部通过证据。原有能力包括因 GUI 设计缺口暂停的能力。实现时更新本表状态；只在有独有决策或确需长期保留的真实验收结果时更新相应文档。具体失败输入只加小 fixture/回归用例，不复制全库。

| 功能族 | 覆盖范围 | 实现 owner | 当前状态 |
| --- | --- | --- | --- |
| F01 | 模型配置、registry、thinking/context/quota | TASK-T016、TASK-T028、TASK-T033、TASK-T042、TASK-T048 | 待实现 |
| F02 | Anthropic、Completions、Responses、Bedrock | TASK-T018、TASK-T021、TASK-T022、TASK-T023 | [T018](#task-t018-anthropic-provider) Anthropic 请求/流契约已验证；[T021](#task-t021-openai-completions-provider) Completions 协议与 CLI 接线已用本地 fake provider 验证；[T022](#task-t022-openai-responses-provider) Responses 协议、runtime 接线与自定义 provider CLI 链路已用本地 fake provider 验证；T023 Go Bedrock 请求、AWS SDK SigV4/default credential chain/bearer/skip-auth、ConverseStream event 内容与异常已用本地假凭据及二进制 fixtures 验证，model.NewProtocol/runtime 抽象接线已覆盖；真实 AWS 网络授权未执行 |
| F03 | credentials、PKCE/callback/device、刷新/注销 | TASK-T024、TASK-T032、TASK-T033、TASK-T034、TASK-T054、TASK-T092 | 三个 OAuth provider 与共享目录的本地测试通过，见 [M032](evidence/M032.md)、[M033](evidence/M033.md)、[M034](evidence/M034.md)、[M092](evidence/M092.md)；真实账号授权待 T054 |
| F04 | Agent loop、hooks、并行/终止/取消 | TASK-T019、TASK-T020、TASK-T026、TASK-T037、TASK-T038 | T019 Go core loop/tool/hooks 的 fake provider/tool 与 race 用例已验证；app/session owner、权限和产品生命周期/接线待 T020、T026、T037、T038 |
| F05 | 会话列表/读取/JSONL写入、分支/恢复/重命名/删除/cwd | TASK-T001、TASK-T002、TASK-T003、TASK-T010、TASK-T017、TASK-T037、TASK-T044 | T001 列表、T002 只读树恢复、T003 Electron 切换/历史已实现，见 M001/M002/[M003](evidence/M003.md)；T010 协议见 [M010](evidence/M010.md)；[T017](#task-t017-jsonl-manager) 已补齐 Go manager 写入/重开/分支/rename/delete/cwd；运行会话与产品入口仍待 T037/T044 |
| F06 | compaction、有效上下文与会话配置 overlay | TASK-T002、TASK-T028 | T002 已恢复持久化有效上下文，见 M002；实际压缩与配置 overlay 待 T028 |
| F07 | Read/Write/Edit/Bash、锁、diff/evidence与进程树 | TASK-T025、TASK-T026、TASK-T045 | T025 Go Read/Write/Edit、进程内路径锁、file delta/turn evidence 与 tracked/untracked Git diff 已通过 backend 专用 race 测试；真实文件错误、CRLF/BOM/fuzzy、歧义匹配、symlink 与取消均有覆盖。T026 Go Bash 已覆盖 PATH/cwd/quote/UTF-8、stdout/stderr、退出码、超时、取消、输出上限和后代清理；当前宿主为 macOS，Linux/Windows 仅完成适配器构建/fixture 检查，尚无实机运行证据。权限失败、原子 rename 失败及 T025 极限截断仍待覆盖 |
| F08 | permission模式/规则/approval/trust/Hint | TASK-T035、TASK-T039、TASK-T050 | T035 Go 模块已覆盖 mode、deny/ask/allow precedence、glob/regex 与 path-aware `**`、复合 shell 与 progressive trust、workspace/symlink 边界、Rust 默认只读命令及可编辑安全边界、危险命令/嵌套替换/敏感环境变量阻断、Hint、session/project trust；审批 owner 按 session/request identity 隔离 queue/respond/failure/retry/stale/cancel/close，成功响应才移除。项目 trust adapter 经 Go settings manager 写 project layer，含 legacy readonly-rule migration；race 测试和 vet 通过。对照 `crates/rozsa-app/src/permissions/mod.rs` 与 permission controller/trust hierarchy/persistence/safe command 既有测试。尚未接 Go runtime 的设置快照、工具执行及 transport；因此本项模块契约已实现，但生产权限链路仍待 T037/T038/T039/T050 接线与验收。 |
| F09 | 多题/多选/Other、队列/取消/迟到响应 | TASK-T027、TASK-T086、TASK-T039 | T027 `backend/internal/app/questions` 已实现参数/答案校验、产品 request ID、FIFO presentation、跨 session 保留、取消和迟到响应拒绝；T086 将 Go runtime 的生产 AskUserQuestion tool 接入同一 owner，并以真实 `Tool.Execute` 入口验证回答、取消和迟到处理。Electron question UI/transport 生命周期仍待 T039；本条不代表生产端到端验收。 |
| F10 | 后台会话、queue/steering/fork/subagent | TASK-T037、TASK-T040、TASK-T049 | [T037](#task-t037-后台会话与生命周期) Go owner 已接入生产 runtime 与关闭链，双会话事件、取消、状态、问题和审批清理隔离及 JSONL/queue/steer/fork 已通过受控 provider 与 JSON-RPC/race 验收；[T040](#task-t040-subagent-后端运行与重接) 已接真实工具/独立 loop/question/JSONL、scope、进程取消、存活执行后端重接与重启历史保留，按用户决定不新增跨重启 manager 恢复或 subagent 专属 queue/steering/fork；完整运行 UI 待 T049，真实 dashboard 待 T041 |
| F11 | 分层设置、主题、快捷键、capability/skills/resources/extensions | TASK-T015、TASK-T030、TASK-T031、TASK-T036、TASK-T038、TASK-T043、TASK-T047、TASK-T050、TASK-T091、TASK-T092 | T030 Go themes 存储及设置 snapshot 契约已通过本模块聚焦检查，见 [M061](evidence/M061.md)；T031 Go 固定/可编辑快捷键定义、校验、冲突和 global override/reset 契约已通过本模块聚焦检查，见 [M062](evidence/M062.md)；T036 已接入 Go layered settings/themes/model selection 与 React General/Appearance、runtime model/thinking/block-images/auto-compaction/naming/retry，以及 macOS native vibrancy。T091/T092 补齐 Transport 能力限制与模型 auth filtering/目录更新，见 [M092](evidence/M092.md)；steering/follow-up/running-send 执行仍有差异，host GUI 材质效果未验证。T038 Go skills/resources/extensions/capabilities 四模块专用测试通过，覆盖分层覆盖与 reset、诊断与 slash 注入、资源祖先顺序/配置覆盖、扩展通知继续及转换停止/取消；对照 `crates/rozsa-app/src/{skills,resources,extensions}/`、skills/resources/settings-capabilities 现有测试。模块尚未接入生产 runtime/renderer，后续由 T043/T050 等验收；Electron native accelerator、capture 仍待对应任务验收 |
| F12 | Dev Flow discovery/dashboard/REST/SSE/重连/回收 | TASK-T041、TASK-T048 | 待实现 |
| F13 | 消息/Markdown/tools/notification与异常 | TASK-T003、TASK-T029、TASK-T045、TASK-T051、TASK-T079、TASK-T080 | T003 提供只读历史与手动重试，见 M003；T029 建立共享 React 消息/Markdown 真源，保留 user/assistant/system/toolResult、thinking、图片、工具卡片、错误、流式与终态；Electron bridge→Go runtime 真实接通 start/continue/replay/subscribe/cancel，runtime 写入后刷新 session 快照再发布 terminal event。2026-10-18 macOS arm64 原生候选、1,200 条历史/32 个 fake-provider delta 单次观测：历史 React 行首次呈现 441 ms、首个流片段 192 ms、DevTools 连续输入 124 ms，最终 1,202 行且中段滚动位置保持；全量测试并发运行观测分别升至 1,336/2,380/390 ms，作为负载敏感样本。独立性能入口 `npm --prefix desktop/tests run test:performance`，测试位于 `desktop/tests/performance/message-performance.test.js`，不纳入普通 `test`/`test:ui` 通配符；T029 的 `npm --prefix desktop/tests test` 曾通过 108/108，`npm --prefix desktop run typecheck` 与 Go session/runtime/RPC 聚焦测试通过。T045/T051 其余消息 UI 与通知异常仍待验收 |
| F14 | Composer/autocomplete/model/file refs/picker/drop/IME | TASK-T042、TASK-T046、TASK-T049 | 待实现 |
| F15 | 单窗口材质/split/scene/root与焦点草稿滚动 | TASK-T003、TASK-T012、TASK-T052、TASK-T056、TASK-T057、TASK-T058、TASK-T080 | T080 接入 Sidebar vibrancy 并装配单 WebView 的既有 sidebar/main；其余原生窗口、焦点/草稿/scene 状态由 T052 和真实平台验收任务负责 |
| F16 | 全部CLI参数/text/json/continue/resume/slash/session | TASK-T020、TASK-T044 | 待实现 |
| F17 | PATH/config/版本/开发构建/安装包/发行 | TASK-T001、TASK-T009、TASK-T014、TASK-T015、TASK-T033、TASK-T044、TASK-T056、TASK-T057、TASK-T058、TASK-T060、TASK-T061、TASK-T062 | 待实现 |

差异记录：当前切片发现的旧行为与需求冲突，由对应 owner 明确差异并确认；未覆盖能力持续可见。T055 核对五 crate active exports、GUI commands/events、CLI 参数与 slash、settings/config/credentials、JSONL 和暂停入口；T059/T062 收口全部 Must。完整覆盖是最终交付要求，不是开工前置任务。

## 设计缺口 owner

T004–T008 记录需要新增或改变设计时的提案 owner，不作为既有 UI 迁用或接线的依赖；只有修改 prototype 的新增/改变设计范围需先取得批准。独立 backend 不受其阻断；未批准的设计不得修改 prototype，未恢复功能不移除暂停守卫。

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
| SPEC-AC-015 | TASK-T001、TASK-T009、TASK-T010、TASK-T012、TASK-T013、TASK-T029、TASK-T036、TASK-T039、TASK-T042、TASK-T043、TASK-T045、TASK-T046、TASK-T047、TASK-T048、TASK-T049、TASK-T050、TASK-T051、TASK-T052、TASK-T053、TASK-T060、TASK-T061、TASK-T062、TASK-T079 |

## 审查与验证边界

取消、修改或完成任务的工作流操作见 CONTRIBUTING；正式状态由 dow 管理。平台、原生与真实授权尚未验证时保持待完成；focused checks 不等于全项目 TEST。性能/维护收益在候选就绪后按 T059 验收。

## Related Docs

- [迁移 SPEC](../../.dev-doc/refactor/electron-go-migration/SPEC.md)
- [GUI 架构](../gui/ARCHITECTURE.md)、[设计缺口](../../prototype/docs/DESIGN_GAPS.md)
- [贡献与测试约定](../../CONTRIBUTING.md)
- 正式任务：`dow task list --all` / `dow task show TASK-Txxx`（仅通过 dow 管理）
