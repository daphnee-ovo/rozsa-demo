# TASK-T027 验证记录

## 目标

针对迁移后 prototype 与 T002 complete-session baseline 的差异，恢复 application shell 的原有几何约束、scene 专用样式和 Composer/input frame，并覆盖用户指出的 sidebar 分区、Subagents 和输入框消失问题。

## 根因与修复

1. `prototype/index.html` 在 `app-body` 外新增了 `[data-application-shell]`，但它仍是普通 block。`app-body` 的 `flex: 1` 因此没有可用的 flex containing block，消息内容把 shell 撑到约 1819px，Composer 被推到 720px viewport 之外。`prototype/styles/layouts/app-shell.css` 现在让 application shell 成为受 viewport 约束的 column flex container，并保留 `min-height: 0` 与 `overflow: hidden`。
2. sidebar 和 conversation 通过 template mount wrapper 注入。wrapper 现在使用 `display: contents`，因此原有的 `sidebar`、`main-panel` 继续作为 app-body 的 grid item；complete-session 的 session/top、Subagents、status/bottom 分区不再被 wrapper 改变 sizing。
3. root shell 现在按 `scenarios/registry.js` 的 scene definition 加载迁移前对应的 scene stylesheet。complete-session 的 Subagents 圆点列表、Queue/Steering 分隔线和 running Composer 状态样式因此重新生效；permission-request 不错误加载 ask-user-question 专用样式。

## 前后证据

- 迁移前 baseline：`tmp/ui-architecture-refactor/TASK-T002/prototype-screenshots/complete-session.png`
- 修复后同一 scene：`tmp/ui-architecture-refactor/TASK-T027/screenshots/complete-session-fixed.png`
- 修复后 DOM/computed geometry：`tmp/ui-architecture-refactor/TASK-T027/scenes.json`
- gallery 三状态 geometry：`tmp/ui-architecture-refactor/TASK-T027/gallery.json`
- 全部截图：`tmp/ui-architecture-refactor/TASK-T027/screenshots/`

浏览器验证环境固定为 1280×720、devicePixelRatio 2；T002 baseline 是 1440×1000、devicePixelRatio 2，因此两张 PNG 不能作逐像素尺寸结论。布局契约使用同一 fixture 检查：修复后 complete-session 的 `appBody` 为 1280×720，`main-panel` 为 1024×678，Composer 为 x=318/y=581/900×115，`#msgInput` 为 x=319/y=582/898×56；sidebar children 依次为 sessions/top、Subagents、status、sidebar-bottom。

## 场景与 gallery 重放

- 11 个 scene 均完成加载；`scenes.json` 中没有 `shellError`、`sceneError` 或 alert。
- 11 个 scene 的 body 均为 `scrollWidth=1280`、`scrollHeight=720`、`overflowX=hidden`。
- complete-session 实际加载 `styles/scenes/complete-session.css`；gallery 实际加载 `gallery/visual-direction.css`。
- gallery 的 Empty、Active、Appearance 三个状态均已截图。

## 回归测试

通过：

```text
cargo fmt --all -- --check
cargo test -p rozsa-gui --test prototype_layout_parity_test --test prototype_scene_fixture_test --test prototype_coverage_inventory_test --test prototype_stylesheet_extraction_test --test prototype_root_cutover_test
node --check prototype/scripts/core/shell.js
node --check prototype/scenarios/registry.js
node --check prototype/rozsa-gui.js
git diff --check
```

新增静态回归断言覆盖 application-shell sizing、template mount grid semantics、scene stylesheet 接线、Subagents 专用样式和 Composer input template。server 在最终交付前关闭。
