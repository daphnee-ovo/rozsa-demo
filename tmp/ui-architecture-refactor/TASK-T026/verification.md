# TASK-T026 补充修复验证

## 结论

本任务修复了阶段三迁移后 application scenes 与 gallery 的两类可见差异：运行态面板归属错误，以及 Dev Flow scene 选错 session fixture。修复后重新打开全部 11 个 application scene 和 gallery 的 Empty、Active、Appearance 视图，并保存截图、关键 DOM/computed geometry 与环境 metadata。

## 根因与修复

### 1. 运行态面板被放入了错误的层级

重构后的共享 Composer template 曾把 `subagentPanel` 放在 Composer 内；重构前 `complete-session` 和 `ask-user-question` 的该面板位于 sidebar。与此同时，`forkPicker`、`queuedMessages`、`steeringConversation` 本来就属于 Composer，不能随 subagent 面板一起移动。

已修复为：

- `prototype/templates/features/sidebar.html` 唯一提供 `#subagentPanel`，位于 session list 后、Status 前；
- `prototype/templates/composites/composer.html` 不再提供 `#subagentPanel`；
- Composer 继续唯一提供 `#forkPicker`、`#queuedMessages`、`#steeringConversation`；
- `prototype/mock/tools/call-states.json` 的 complete-session running fixture 保持四种状态输入，scene 脚本只负责装配，不复制稳定 DOM。

回归检查覆盖了稳定 DOM 唯一性、baseline fixture 以及 Composer/sidebar 归属。
对 `5400367` 的迁移后原始文件做了只读归因检查：sidebar 没有 `subagentPanel`，Composer 仍有该节点，adapter 仍使用 `session?.sessions || runtime?.sessions`；因此新增断言对应真实回归，旧内容不能通过当前 fixture/归属检查。

### 2. Dev Flow scene 选择了空 session 列表

`mock-adapter.js` 原先优先读取共享 `mock/sessions/empty.json` 的 `sessions`，导致 `dev-flow-runtime` 的主标题和 sidebar 不再是 baseline 的 “Dev Flow active work”。现在 Dev Flow runtime fixture 优先提供 session 列表，workspace/messages 仍按现有 mock contract 装配：

```js
const sessions = runtime?.sessions || session?.sessions;
```

这不是空数据 fallback；如果两个 fixture 都没有数组，adapter 仍直接报错。

## 对照结果

旧 prototype baseline 位于 `tmp/ui-architecture-refactor/TASK-T002/prototype-screenshots/`。已实际对照以下迁移前/迁移后对象：

| 对象 | 迁移前关注点 | T026 迁移后结果 |
| --- | --- | --- |
| `complete-session` | `SUBAGENTS 1` 在 sidebar；Fork/Queue/Steering 在 Composer | `#subagentPanel` 位于 sidebar，矩形为 `x=10,y=240,w=236,h=117`；三个 Composer 面板分别位于 Composer 下方，未产生重复稳定 DOM |
| `ask-user-question` | subagent/sidebar 与问题面板的相对位置 | sidebar 保留 `SUBAGENTS 1`，问题面板仍占用主内容/Composer replacement 区域 |
| `dev-flow-runtime` | session 标题为 “Dev Flow active work”，状态与队列在主工作区 | session 列表和标题恢复为 Dev Flow fixture；Queue/Steering 仍在 Composer |
| 其余 8 个 application scenes | shell、settings、notifications、permission、tool-call 的公共布局和状态 | 全部无 `alert`、`shellError`、`sceneError`；逐项保存 panels geometry |
| gallery Empty/Active/Appearance | 独立 gallery 布局，不误套 application shell | 三个 tab 均在同一 gallery，显式切换后分别保存；`scrollWidth=clientWidth=1280`，无横向溢出 |

## 验证环境与限制

- server：`uv run python -m http.server 4173 --bind 127.0.0.1 --directory prototype`；验证结束后关闭。
- browser：fresh validation tab；所有 scene 和 gallery capture 均重新导航，gallery tab 显式点击 Empty、Active、Appearance；theme 为 `light`。
- 当前浏览器后端固定提供 `1280×720 CSS px`、`DPR=2`，页面 computed `zoom=1`，motion 未人为改变，scroll 为 `(0,0)`；PNG 实际尺寸均为 `1280×720`。
- TASK-T002 历史 baseline 的主 viewport 是 `1440×1000 CSS px`、`DPR=2`。当前浏览器后端没有 viewport setter，不能声称本次 PNG 与该历史 baseline 是同 viewport 的像素级截图。因此这里同时保存 viewport、DPR、theme、zoom、focus、scroll、DOM rect 和 scrollWidth，明确区分真实布局证据与 capture 环境差异；不能用当前截图替代未来在 `1440×1000` 环境下的最终像素 diff。
- 这次没有把浏览器缩放、DPR 放大或裁切误判为页面布局：gallery 的 `scrollWidth` 与 viewport 一致，application scenes 的 `overflowX` 为 `hidden`，截图尺寸和 metadata 一致。

## 证据文件

- `screenshots/scenes.json`：11 个 application scene 的 viewport、focus、shell error 和全部关键 panel 的 DOM rect；对应 `scene-*.png`。
- `screenshots/gallery.json`：gallery 三个状态的 tab state、viewport、zoom、scrollWidth、rail/workspace rect；对应 `visual-direction-*.png`。
- 关键复核图：`scene-complete-session.png`、`scene-ask-user-question.png`、`scene-dev-flow-runtime.png`、`visual-direction-empty.png`、`visual-direction-active.png`、`visual-direction-appearance.png`。

## 自动化验证

```text
cargo test -p rozsa-gui --test prototype_scene_fixture_test \
  --test prototype_coverage_inventory_test \
  --test prototype_root_cutover_test
```

结果：`11 passed, 0 failed`（fixture 3、coverage 6、root cutover 2）。另行运行 prototype stylesheet extraction 专项测试、Rust formatting、JavaScript syntax check 和 `git diff --check`。
