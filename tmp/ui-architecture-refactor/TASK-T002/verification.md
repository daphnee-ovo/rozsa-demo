# TASK-T002 重构前 baseline

本文记录 `TASK-T001` 的 [`UI_COVERAGE.json`](../../../docs/gui/UI_COVERAGE.json) 中全部稳定检查 ID 的重构前条件。后续比较必须复用同一 ID、输入、viewport、theme、操作步骤和证据范围；prototype 只能与 prototype 对比，runtime 只能与 runtime 对比。

## 固定环境

| 项目 | 值 |
| --- | --- |
| Git revision | `cbc8a8f03931ac9f151cafde2e338e949c440833` |
| 日期与时区 | 2026-08-22，Asia/Singapore |
| 平台 | macOS 26.5.2（Build 25F84） |
| Prototype server | `uv run python -m http.server 4173 --bind 127.0.0.1 --directory docs/gui/new-version` |
| Prototype browser | Codex In-app Browser；浏览器控制接口未暴露 UA/engine version，不能据此宣称具体 Chromium/WebKit 版本 |
| Desktop viewport | 1440×1000，DPR 1；每页初始 `scrollX=0`、`scrollY=0` |
| Narrow viewport | 820×900，DPR 1；覆盖 empty、complete、settings、Dev Flow runtime、gallery |
| Browser default | 1280×720，DPR 2；仅用于重置 viewport 后确认环境，不作为成对截图尺寸 |
| Motion | `prefers-reduced-motion: reduce = false`；未伪造 reduce 环境 |
| Prototype font | `-apple-system, system-ui, SF Pro Text, PingFang SC, Noto Sans CJK SC, Segoe UI, system-ui, sans-serif` |
| Runtime launch | `./run.sh`，成功构建并打开 `target/debug/Rózsa.app` |
| Runtime identity | bundle `dev.rozsa.app`；验证实例绝对路径 `/Users/xinyue/ballad/rozsa-demo/target/debug/Rózsa.app` |
| Runtime window | 标准窗口 `Rózsa`，main/sidebar 两个 `tauri://localhost` WebView，启动截图为 1242×800 PNG |
| Runtime theme/font | System theme；Appearance 显示 Font Size 16；UI/code font 值见 `runtime-settings-appearance.png` 的 AX 记录摘要 |
| Runtime data | 当前 workspace 的新建空 session；不读取已有历史 session 内容，不提交 prompt，不触发产品写操作 |
| Runtime draft | 临时输入 `Baseline draft — do not send`，截图后已清空；focus 位于 Composer entry |

Prototype 当前没有集中 mock。各 scene 使用 `rozsa-gui.js` 的内置 fixture 与 HTML 中的场景结构；这正是后续 T004 要迁移的重构前输入。每个 `*.json` 保存 URL、scene identity、viewport/DPR、theme、font、motion、scroll、focus、body 尺寸、关键 root computed style、可见 panel ID 和 stylesheet 列表。

## Prototype 实际打开记录

以下页面均通过 HTTP 返回 200、完成 `load`，并保存全页 PNG 与同名 JSON。唯一无关网络噪声是浏览器请求 `/favicon.ico` 返回 404；页面 CSS/JS 均成功加载。

| Scene / gallery | 初始证据 | 操作后证据 | 操作与预期 |
| --- | --- | --- | --- |
| `ask-user-question` | `prototype-screenshots/ask-user-question.{png,json}` | `ask-user-question-selected.{png,json}` | 点击第二个选项；选项处理后 focus 回到输入区，页面不导航。 |
| `complete-session` | `complete-session.{png,json}` | `complete-session-narrow.{png,json}` | 检查 message、thinking、tool、queue/steering 与 Composer；窄屏保持 820 宽且无页面级横向滚动。 |
| `dev-flow-active` | `dev-flow-active.{png,json}` | — | 检查 settings connected/dashboard 与 sidebar active work fixture。 |
| `dev-flow-runtime` | `dev-flow-runtime.{png,json}` | `dev-flow-runtime-expanded.{png,json}`、`dev-flow-runtime-narrow.{png,json}` | 点击 T004 row；focus 留在 row，`aria-expanded` 状态存在；检查 detail/file tree/pin 布局。 |
| `dev-flow` | `dev-flow.{png,json}` | — | 检查 unavailable/未连接设置状态。 |
| `empty-session` | `empty-session.{png,json}` | `empty-session-narrow.{png,json}` | 检查空 poster、Composer 与 application shell。 |
| `notifications` | `notifications.{png,json}` | `notifications-dismissed.{png,json}` | 点击第一条 Dismiss；可访问 `status` 数量从 4 变为 3。 |
| `permission-request` | `permission-request.{png,json}` | `permission-request-hint.{png,json}` | 点击 Deny and hints；focus 进入 `#permHintInput`。 |
| `permissions` | `permissions.{png,json}` | — | 只呈现 permission settings 空 shell，和 `UI-SETTINGS-005` 的 `missing` 分类一致。 |
| `settings-appearance` | `settings-appearance.{png,json}` | `settings-appearance-dark.{png,json}`、`settings-appearance-narrow.{png,json}` | 点击 Dark；`data-theme=dark` 且 Dark 为 pressed。 |
| `tool-calls` | `tool-calls.{png,json}` | — | 检查 Read/Edit/Bash/Subagent/Question/generic fixture 及当前展开状态。 |
| visual demo Empty | `visual-demo.{png,json}` | `visual-demo-narrow.{png,json}` | 检查 Independent demo topbar 与 Empty view；窄屏保存完整长页。 |
| visual demo Active | — | `visual-demo-active.{png,json}` | 点击 Active，保存可见 heading、focus 与完整截图。 |
| visual demo Appearance | — | `visual-demo-appearance.{png,json}` | 点击 Appearance，保存可见 heading、focus 与完整截图。 |

## Prototype 稳定检查 ID 对照

| 检查 ID | 覆盖项目 | 截图 / DOM-computed-style 证据 | 重放要点 |
| --- | --- | --- | --- |
| `BASE-P-SHELL-001` | `UI-SHELL-001` | `empty-session.*`、`complete-session.*` 及两份 narrow | 以相同 viewport/theme 比较 shell/sidebar/composer root。 |
| `BASE-P-SIDEBAR-001` | `UI-SIDEBAR-001` | `empty-session.*`、`complete-session.*` | 比较 session row、active/idle/empty/new-session 可见结构。 |
| `BASE-P-SIDEBAR-002` | `UI-SIDEBAR-002` | `complete-session.*` | 比较 Git、tool chip 与 quota bar。 |
| `BASE-P-SIDEBAR-003` | `UI-SIDEBAR-003` | `complete-session.*` | tooltip 容器存在；near-limit/error/hidden 未定义，保持 partial。 |
| `BASE-P-CONV-001` | `UI-CONV-001` | `empty-session.*` | 比较空 poster、scroll=0 与 focus。 |
| `BASE-P-CONV-002` | `UI-CONV-002` | `complete-session.*` | 只比较 fixture 已展示的 message；缺失矩阵不得补画。 |
| `BASE-P-CONV-003` | `UI-CONV-003` | `complete-session.*`、`tool-calls.*` | 比较 thinking expanded、duration 与 focus。 |
| `BASE-P-COMPOSER-001` | `UI-COMPOSER-001` | `empty-session.*`、`complete-session.*` | 比较 Composer DOM、toolbar、draft/focus 基线。 |
| `BASE-P-COMPOSER-005` | `UI-COMPOSER-005` | `complete-session.*` | 比较 thinking effort control 与初始 focus。 |
| `BASE-P-FLOW-001` | `UI-FLOW-001` | `complete-session.*` | queue/steering 只验证现有 populated fixture，异常状态保持 partial。 |
| `BASE-P-TOOL-001` | `UI-TOOL-001` | `tool-calls.*` | 比较 Read/Edit/Bash 当前 fixture；完整 lifecycle 保持 partial。 |
| `BASE-P-TOOL-002` | `UI-TOOL-002` | `tool-calls.*`、`ask-user-question.*` | 比较 specialized/generic 当前实例。 |
| `BASE-P-EVIDENCE-001` | `UI-EVIDENCE-001` | `tool-calls.*`、`complete-session.*` | 比较 code/diff/file changes/turn activity。 |
| `BASE-P-MARKDOWN-001` | `UI-MARKDOWN-001` | `complete-session.*` | 比较已展示 Markdown；完整构造矩阵保持 partial。 |
| `BASE-P-PERMISSION-001` | `UI-PERMISSION-001` | `permission-request.*`、`permission-request-hint.*` | main → hint，期望 focus=`permHintInput`。 |
| `BASE-P-QUESTION-001` | `UI-QUESTION-001` | `ask-user-question.*`、`ask-user-question-selected.*` | 选项点击、focus 与 panel 占位结果不变。 |
| `BASE-P-SETTINGS-001` | `UI-SETTINGS-001` | `settings-appearance.*`、`settings-appearance-dark.*`、narrow | Appearance light → dark，pressed/theme 一致。 |
| `BASE-P-SETTINGS-005` | `UI-SETTINGS-005` | `permissions.*` | 空 shell 是缺口证据，不视为 filled rule coverage。 |
| `BASE-P-SETTINGS-006` | `UI-SETTINGS-006` | `settings-appearance.*` | `#pane-extensions` 保留空状态结构。 |
| `BASE-P-NOTIFICATION-001` | `UI-NOTIFICATION-001` | `notifications.*`、`notifications-dismissed.*` | status 数量 4 → 3。 |
| `BASE-P-FEEDBACK-001` | `UI-FEEDBACK-001` | 全部 scene JSON/PNG | 只比较各 scene 已有 error/empty/long-content；未定义状态保持 partial。 |
| `BASE-P-DEVFLOW-001` | `UI-DEVFLOW-001` | `dev-flow.*`、`dev-flow-active.*` | unavailable/connected/dashboard fixture 分别比较。 |
| `BASE-P-DEVFLOW-002` | `UI-DEVFLOW-002` | `dev-flow-active.*`、`dev-flow-runtime.*` | 比较 claimed work/more/hover/pin 当前状态。 |
| `BASE-P-DEVFLOW-003` | `UI-DEVFLOW-003` | `dev-flow-runtime.*`、expanded、narrow | 点击 T004，比较 focus/expanded/detail/file tree。 |
| `BASE-P-DEVFLOW-004` | `UI-DEVFLOW-004` | `dev-flow-runtime.*` | 比较 Dev Flow Bash presentation。 |
| `BASE-P-GALLERY-001` | `UI-GALLERY-001` | `visual-demo.*` | 比较 Independent demo topbar；不得套用 application shell。 |
| `BASE-P-GALLERY-002` | `UI-GALLERY-002` | `visual-demo.*`、narrow | 比较 Empty view。 |
| `BASE-P-GALLERY-003` | `UI-GALLERY-003` | `visual-demo-active.*` | 点击 Active 后比较可见 heading 与布局。 |
| `BASE-P-GALLERY-004` | `UI-GALLERY-004` | `visual-demo-appearance.*` | 点击 Appearance 后比较可见 heading 与布局。 |

## Runtime 实际运行记录

`./run.sh` 在 40.13 秒内完成 debug build 并打开目标 bundle。Computer Use 使用绝对 app 路径，避免与系统中另一份同 bundle identifier 的应用混淆。验证过程没有点击 Send、Dashboard、Add/Delete/Restore、theme switch 或任何会修改产品数据的控件。

| 到达状态 | 截图 | 实际结果 |
| --- | --- | --- |
| main + native sidebar + empty session | `runtime-screenshots/runtime-main-initial.png` | main/sidebar 两个 WebView 同时存在；Composer entry 初始 focus；sidebar 显示 session、Git/Dev Flow status。 |
| Composer draft | `runtime-composer-draft.png` | entry 显示临时 draft；未发送；随后清空并确认 placeholder 恢复。 |
| Settings / Skills | `runtime-settings.png` | 通过 sidebar Settings 进入；main scene 切换、sidebar 成为 settings navigation。 |
| Appearance | `runtime-settings-appearance.png` | System theme、Font Size 16、translucent/rate-limit controls 与主题字段可达；未修改值。 |
| Permissions settings | `runtime-settings-permissions.png` | Global/Project、mode、Deny/Ask/Allow、现有 allow rules 与 Read-only Bash summary 可达；未修改规则。 |
| Dev Flow settings | `runtime-settings-dev-flow.png` | Dashboard 显示 Ready，但按钮 unavailable；版本 v0.3.9、path 与三个开关可达。 |
| Notification/error lifecycle | `runtime-main-initial.png`、`runtime-notification-dismissed.png` | 启动错误 toast 可见；Dismiss 后错误进入 expanded Unresolved errors list。 |

PermissionRequest/AskUserQuestion、tool lifecycle、queue/steering、model picker、附件、autocomplete 和 filled error matrices 没有通过真实 app 人工制造：创建这些状态需要启动 agent、注入 production event 或改变用户数据，不属于 T002 的只读 baseline 授权。对应稳定 ID 仍引用当前 runtime screenshot、源码入口和专项测试；状态继续保持 `partial`/`missing`，不得把“未触发”写成通过。

## Runtime 稳定检查 ID 对照

| 检查 ID | 项目 | 截图与实际步骤 | 基线结论 |
| --- | --- | --- | --- |
| `BASE-R-SHELL-001` | `UI-SHELL-001` | `runtime-main-initial.png`；`./run.sh` 启动 | main/sidebar/application shell 可达。 |
| `BASE-R-SCENE-002` | `UI-SHELL-002` | initial → settings → Back；`frontend_scene_continuity` | scene root 未重建，focus 回到 Composer。 |
| `BASE-R-SIDEBAR-001` | `UI-SIDEBAR-001` | `runtime-main-initial.png` | session list/active empty session 可见；未读取其他 session 内容。 |
| `BASE-R-SIDEBAR-002` | `UI-SIDEBAR-002` | `runtime-main-initial.png` | Git/Dev Flow status 可见；当前 model 不显示 rate-limit bar。 |
| `BASE-R-SIDEBAR-003` | `UI-SIDEBAR-003` | `runtime-main-initial.png` | 当前 fixture 无 OAuth quota tooltip；保持 partial。 |
| `BASE-R-CONV-001` | `UI-CONV-001` | `runtime-main-initial.png` | empty session poster 可达。 |
| `BASE-R-CONV-002` | `UI-CONV-002` | `runtime-main-initial.png` + `ui-state` 源码入口 | 当前新 session 无 message；完整矩阵未人工注入。 |
| `BASE-R-CONV-003` | `UI-CONV-003` | initial + `ui-state` 源码入口 | 当前无 thinking；专项 source/continuity evidence 保留。 |
| `BASE-R-COMPOSER-001` | `UI-COMPOSER-001` | `runtime-composer-draft.png`；set draft → focus → clear | 草稿、placeholder 与 focus 恢复成功。 |
| `BASE-R-COMPOSER-002` | `UI-COMPOSER-002` | `runtime-composer-draft.png` + `ime_runtime_test` | 未打开 autocomplete；missing 分类不变。 |
| `BASE-R-COMPOSER-003` | `UI-COMPOSER-003` | `runtime-main-initial.png` | Attach button 可达；未打开 native picker/传输文件。 |
| `BASE-R-COMPOSER-004` | `UI-COMPOSER-004` | `runtime-main-initial.png` | model button `deepseek-v4-flash` 可达；未切换模型。 |
| `BASE-R-COMPOSER-005` | `UI-COMPOSER-005` | `runtime-main-initial.png` | thinking effort `max` 可达；未修改。 |
| `BASE-R-FLOW-001` | `UI-FLOW-001` | `runtime-main-initial.png` | 未启动 turn；queue/steering/fork/subagent 保持 partial。 |
| `BASE-R-TOOL-001` | `UI-TOOL-001` | `runtime-main-initial.png` + `tool-event` 入口 | 未运行 tool；完整 lifecycle 保持 partial。 |
| `BASE-R-TOOL-002` | `UI-TOOL-002` | `runtime-main-initial.png` + renderer 入口 | 未运行 specialized tool；保持 partial。 |
| `BASE-R-EVIDENCE-001` | `UI-EVIDENCE-001` | `runtime-main-initial.png` + renderer 入口 | 当前 empty session 无 evidence；后续须用同 fixture 重放。 |
| `BASE-R-MARKDOWN-001` | `UI-MARKDOWN-001` | `runtime-main-initial.png` + renderer 入口 | 当前 empty session 无 Markdown；保持 partial。 |
| `BASE-R-PERMISSION-001` | `UI-PERMISSION-001` | `runtime-settings-permissions.png` + `permission_runtime_test` | ownership test 通过；PermissionRequest 未人工制造，保持 partial。 |
| `BASE-R-PERMISSION-002` | `UI-PERMISSION-002` | 同上 | trust/queue/error/cancel 未人工制造，保持 missing。 |
| `BASE-R-QUESTION-001` | `UI-QUESTION-001` | `runtime-main-initial.png` + runtime renderer 入口 | question event 未人工注入；后续需受控 fixture。 |
| `BASE-R-SETTINGS-001` | `UI-SETTINGS-001` | `runtime-settings.png`、`runtime-settings-appearance.png` | navigation/Appearance 实际可达。 |
| `BASE-R-SETTINGS-002` | `UI-SETTINGS-002` | `runtime-settings.png` | General 可由 navigation 到达；异常数据未注入。 |
| `BASE-R-SETTINGS-003` | `UI-SETTINGS-003` | `runtime-settings.png` | Skills populated rows 实际可见；未切换/reset。 |
| `BASE-R-SETTINGS-004` | `UI-SETTINGS-004` | `runtime-settings.png` | Keyboard navigation 入口可见；未编辑 shortcut。 |
| `BASE-R-SETTINGS-005` | `UI-SETTINGS-005` | `runtime-settings-permissions.png` | populated permission rows 实际可见，未修改。 |
| `BASE-R-SETTINGS-006` | `UI-SETTINGS-006` | `runtime-settings.png` | Extensions navigation 入口可见。 |
| `BASE-R-NOTIFICATION-001` | `UI-NOTIFICATION-001` | initial → Dismiss → `runtime-notification-dismissed.png` | toast 转入 Unresolved errors list。 |
| `BASE-R-NOTIFICATION-002` | `UI-NOTIFICATION-002` | 同上 + `app-notification/notification` 入口 | lifecycle 有实际可见结果；model reconciliation 未触发。 |
| `BASE-R-FEEDBACK-001` | `UI-FEEDBACK-001` | initial 与 dismissed screenshot | 实际捕获 dashboard startup error；其他异常矩阵保持 partial。 |
| `BASE-R-DEVFLOW-001` | `UI-DEVFLOW-001` | `runtime-settings-dev-flow.png` | settings 实际可达；Ready/unavailable 矛盾状态按原样记录。 |
| `BASE-R-DEVFLOW-002` | `UI-DEVFLOW-002` | `runtime-main-initial.png` | sidebar Dev Flow row/status 可见。 |
| `BASE-R-DEVFLOW-003` | `UI-DEVFLOW-003` | initial + `dev_flow_runtime_wiring_test` | 当前 0 task/0 issue，无 detail payload；后续使用受控 fixture。 |
| `BASE-R-DEVFLOW-004` | `UI-DEVFLOW-004` | initial + runtime wiring test | 未执行 dow Bash tool presentation。 |
| `BASE-R-DEVFLOW-005` | `UI-DEVFLOW-005` | main/sidebar screenshot + scene identity/continuity tests | macOS 双 WebView 实际存在；未伪称 Linux/Windows 视觉通过。 |

## 专项测试

命令：

```text
cargo test -p rozsa-gui \
  --test prototype_coverage_inventory_test \
  --test frontend_scene_identity \
  --test frontend_scene_continuity \
  --test ime_runtime_test \
  --test permission_runtime_test \
  --test dev_flow_runtime_wiring_test
```

| Target | 结果 |
| --- | --- |
| `prototype_coverage_inventory_test` | 5 passed |
| `frontend_scene_identity` | 2 passed |
| `frontend_scene_continuity` | 3 passed |
| `ime_runtime_test` | 2 passed |
| `permission_runtime_test` | 2 passed |
| `dev_flow_runtime_wiring_test` | 10 passed；存在 3 条预存 dead-code warning |

合计 24 passed、0 failed。warning 涉及 `FakeRevision::{Unborn,Detached}`、`FakeRunner::set_status` 和 `FakeFactoryState::snapshots`，本任务未修改测试或 production 实现。

## 已确认的预存现象

1. Runtime 每次 scene/settings refresh 都出现：`Dev-flow dashboard failed to start dashboard process Some(<pid>) did not become ready within 5s: [dow dashboard] Listening on http://127.0.0.1:9801`。日志同时表明 dashboard 已监听，现有证据只能确认 readiness 判定与监听结果不一致，不能推断根因。
2. Dev Flow settings 同时显示 `Dashboard Ready` 与 disabled `Unavailable` button；按重构前表现记录，后续不得在架构搬移中顺手更改。
3. Prototype HTTP 仅 `/favicon.ico` 为 404；所有 HTML、CSS、JS 验证资源返回 200。
4. Linux/Windows 真实视觉验证为 `pending platform visual verification`；macOS 截图不能替代对应平台结果。

## 清理确认

- 浏览器验证 tab 已 finalize，不保留本地测试页面。
- HTTP server 已收到 Ctrl-C；`lsof -nP -iTCP:4173 -sTCP:LISTEN` 无输出。
- 精确定位 `/Users/xinyue/ballad/rozsa-demo/target/debug/Rózsa.app/Contents/MacOS/rozsa` 后关闭 PID 32867；最终进程列表无该 bundle 路径。
- Composer 临时 draft 已清空；没有发送消息或修改 settings/permission rule。
