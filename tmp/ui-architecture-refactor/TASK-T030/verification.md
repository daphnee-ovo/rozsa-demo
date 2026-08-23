# TASK-T030 验证记录

日期：2026-08-24

## 结果

- application scene registry 不再声明 stylesheet；`shell.js` 不再动态加载 scene CSS。
- `complete-session`、`ask-user-question`、`dev-flow-runtime` 的运行时样式已移到公共 owner：Composer、queue、steering、sidebar、overlays 和 Dev Flow runtime feature。
- `styles/scenes/` 中对应文件只保留迁移边界标记，不再被 HTML 或 shell 加载。
- 窄视口的 app shell 在 780px 以下使用单列 block；sidebar drawer 规则由 `layouts/sidebar-shell.css` 在基础 owner 后维护，避免 `position: relative` 覆盖 drawer 定位。
- Composer 的 chat-input 绝对定位、input wrapper 直接 children、queue/steering/input frame 顺序均恢复。

## 自动化验证

```text
cargo fmt --all -- --check
git diff --check
node --check prototype/scripts/core/shell.js
cargo test -p rozsa-gui --test prototype_scene_ownership_test --test prototype_scene_fixture_test --test prototype_stylesheet_extraction_test --test prototype_layout_parity_test --test prototype_coverage_inventory_test
```

结果：全部通过；共 18 个测试通过。

## 浏览器验证

使用当前 prototype 文件启动本地 server，在 1440×1000、1280×720、780×720、760×720、640×720 实测。

| viewport | app-body | main-panel | sidebar | Composer | scrollWidth |
| --- | --- | --- | --- | --- | --- |
| 1440×1000 | grid | x=256, y=42 | 正常展开 | absolute、底部 | 1440 |
| 1280×720 | grid | x=256, y=42 | 正常展开 | absolute、底部 | 1280 |
| 780×720 | block | x=0, y=42 | absolute drawer、隐藏 | absolute、底部 | 780 |
| 760×720 | block | x=0, y=42 | absolute drawer、隐藏 | absolute、底部 | 760 |
| 640×720 | block | x=0, y=42 | absolute drawer、隐藏 | absolute、底部 | 640 |

`complete-session` 在 760px 下确认：

- `.input-wrapper` 的直接 children 依次为 `#queuedMessages`、`#steeringConversation`、`#msgInput`、`.input-toolbar`。
- queue、steering 和 `#msgInput` 均可见；input frame 未消失。
- `#msgInput` 的 computed style 为 `display:block`、`height:52px`。
- registry application scene 无 stylesheet，页面 stylesheet 只有 `styles/main.css`，`dynamicSceneCss` 为空。

逐一打开 registry 中的 11 个 application scene 与 `?gallery=visual-direction`：均无 shell error、无横向溢出；application scene 均为 `main-panel y=42`、Composer 为 `absolute`，gallery 使用独立 `visual-direction.css`。

成对截图与机器可读记录：

- 迁移前基线：`tmp/ui-architecture-refactor/TASK-T002/prototype-screenshots/complete-session.png`、`complete-session-narrow.png`
- 修复后：`screenshots/complete-session.png`、`complete-session-narrow.png`
- Composer 实测：`screenshots/composer-audit.json`
- scene/gallery 实测：`screenshots/scenes.json`

验证完成后关闭本 task 启动的本地 server。
