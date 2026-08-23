# TASK-T028 验证记录

## 修复结论

complete-session 的 input frame 已恢复为迁移前的实际渲染层级。`input.html` 仍是独立 primitive template，但 shell 挂载时把它的 children 直接插入 Composer 的 `.input-wrapper`，不再留下额外的中间 DOM 容器。

当前运行态结构为：

```text
chat-input
├─ forkPicker
└─ input-wrapper
   ├─ queuedMessages
   ├─ steeringConversation
   ├─ msgInput.rich-input
   └─ input-toolbar
```

这使迁移前的 complete-session scene selector 重新命中 `#msgInput`。在 populated queue/steering fixture 中，实际 computed style 为 `min-height: 52px`、`padding-top: 12px`；Queue、Steering 与输入区共享同一个连续的 `.input-wrapper` 背景、边框、圆角和阴影。

## 对照证据

迁移前基线：

- `tmp/ui-architecture-refactor/TASK-T002/prototype-screenshots/complete-session.png`
- 1440×1000、DPR 1、light theme
- 原始 Composer 中 `#msgInput` 与 Queue/Steering 为同一 `.input-wrapper` 的直接内容

迁移后：

- `tmp/ui-architecture-refactor/TASK-T028/screenshots/complete-session-fixed.png`
- `tmp/ui-architecture-refactor/TASK-T028/screenshots/complete-session-visible-chrome.png`
- `tmp/ui-architecture-refactor/TASK-T028/scenes.json`
- visible Chrome 中确认 Queue、Steering、输入区为一个连续容器；sidebar 的 Sessions 顶部、Subagents/Status/Settings 底部关系保留

浏览器当前验证窗口与 T002 基线不是同一截图尺寸，因此本记录不宣称像素级 diff 通过；视觉检查针对同一 complete-session fixture，结构、层级和 computed style 以 DOM 证据为准。

## 全部 scene 重放

使用新端口重新加载 11 个 application scenes，`scenes.json` 记录了 URL、scene identity、stylesheet、body overflow、Composer children 和关键 geometry。

结果：11 个 scene 均无 `alert`、`shellError` 或 scene error；complete-session 的 `directInput` 为 true，Queue/Steering/Input 的直接子级顺序正确。

## 自动化检查

```text
cargo fmt --all -- --check
cargo test -p rozsa-gui --test prototype_scene_fixture_test --test prototype_layout_parity_test --test prototype_coverage_inventory_test
node --check prototype/rozsa-gui.js
node --check prototype/scripts/core/renderer.js
node --check prototype/scripts/core/shell.js
git diff --check
```

以上检查通过。
