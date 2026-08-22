# TASK-T015 原型统一验证

## 验证范围与环境

本次验证覆盖 `docs/gui/new-version/` 下的规则/模板、脚本、gallery、组件状态、交互流程和完整场景入口。统一入口为 `index.html`：场景使用 `?scene=<id>`，gallery 使用 `?gallery=visual-direction`。gallery 仍是验证内容，不作为运行时应用场景。

验证环境为本地静态服务器：

```text
uv run python -m http.server 4173 --bind 127.0.0.1 --directory docs/gui/new-version
```

桌面核验使用 1440×1000 viewport，窄视口核验使用 820×900 viewport。截图和 JSON 证据均保存在本目录 `screenshots/`；对旧 visual demo 与新 gallery 的配对比较在同一浏览器 tab、同一 viewport 内完成，以隔离浏览器环境差异。

## 分层结果

### 规则、模板与样式层

- `gallery/visual-direction.html` 保留原 visual demo 的 DOM、文本、状态标记和可见结构，只将 inline script 提取为 `gallery/visual-direction.js`。
- `gallery/visual-direction.css` 只导入共享的 `styles/features/visual-demo.css`，没有复制稳定 CSS 规则。
- 样式提取测试确认 gallery HTML 不含 inline style，且引用 gallery CSS 与外置行为脚本。
- `index.html` 在加载应用 shell 前解析并校验 scene/gallery ID；未知值显示明确错误，不进入部分组装状态。

### gallery 组件与交互层

旧入口 `rozsa-visual-demo.html` 与新入口 `gallery/visual-direction.html` 的配对核验结果：

| 检查 | 结果 | 证据 |
| --- | --- | --- |
| Empty 初始结构、computed style、交互状态 | 通过；稳定 `data-od-id` 集合一致，仅存在可忽略的末尾换行差异 | `visual-direction-empty-initial.json` |
| Active panel | 通过；规范化 body markup、computed style、焦点和可见标题一致 | `visual-direction-active.json`、`visual-direction-active.png` |
| Appearance panel | 通过；规范化 body markup、computed style、焦点和可见标题一致 | `visual-direction-appearance.json`、`visual-direction-appearance.png` |
| 窄视口几何与横向溢出 | 通过；旧入口和新入口在同一窄视口下几何一致、无横向溢出 | `visual-direction-narrow.json`、`visual-direction-narrow.png` |

gallery 交互重放：

- Empty → Active → Appearance 的 tab 切换与面板隐藏/显示状态正常。
- `scene-ask-user-question` 选择第二项后，选中值为 `Open the settings surface`。
- `scene-permission-request` 提交 `Deny and hints` 后，hint 输入框获得焦点并保留 `Deny, ` 前缀。
- `scene-notifications` dismiss 第一条通知后，状态数从 4 变为 3，目标文本消失。
- `scene-settings-appearance` 选择 dark 后，dark 卡片 `aria-pressed=true`、light 卡片 `aria-pressed=false`。

交互证据分别见 `ask-user-question-selected.json`、`permission-request-hint.json`、`notifications-dismissed.json` 和 `settings-appearance-dark.json`。

### 完整场景层

11 个注册场景均从统一入口打开，没有 alert 或 `shellError`，每个场景均完成应用 mount：

```text
ask-user-question
complete-session
dev-flow-active
dev-flow-runtime
dev-flow
empty-session
notifications
permission-request
permissions
settings-appearance
tool-calls
```

场景逐项证据见 `scenes.json` 与对应 `scene-*.png`。其中 `empty-session`、`complete-session`、`settings-appearance`、`dev-flow-runtime` 另外完成 820×900 窄视口核验，证据见 `scenes-narrow.json` 和带 `-narrow` 后缀的截图；均无横向溢出。

直接 gallery URL 与统一入口均通过：

- `/gallery/visual-direction.html` 正常加载 gallery CSS、脚本和 scene tabs。
- `/index.html?gallery=visual-direction` 正常加载 gallery。
- `/index.html?gallery=unknown` 显示 `Unknown prototype gallery: unknown`，且不加载应用 shell。
- `/index.html?scene=unknown` 显示 `Unknown prototype scene: unknown`，且不加载应用 shell。

## 基线索引与差异定位

| 基线 ID | 验证内容 | 旧基线 | 新证据 | 差异定位 |
| --- | --- | --- | --- | --- |
| BASE-P-GALLERY-001 | 顶部栏、标签和 gallery 入口 | `docs/gui/new-version/rozsa-visual-demo.html` | `visual-direction-empty-initial.json` | 仅入口资源路径从根目录文件变为 `gallery/` 下的外置资源 |
| BASE-P-GALLERY-002 | Empty 状态 | `visual-demo.png` | `visual-direction-empty.png`、`visual-direction-narrow.png` | 同一 DOM/样式/状态，窄视口无横向溢出 |
| BASE-P-GALLERY-003 | Active 状态 | `visual-demo-active.png` | `visual-direction-active.png` | 规范化 markup、computed style 和 focus 一致 |
| BASE-P-GALLERY-004 | Appearance 状态 | `visual-demo-appearance.png` | `visual-direction-appearance.png` | 规范化 markup、computed style 和 focus 一致 |

该索引与 `docs/gui/UI_COVERAGE.json` 中的 gallery registration 保持一致；覆盖矩阵仍将旧 visual demo 作为基线输入，统一验证入口已切换为 `gallery/visual-direction.html`。

## 自动化检查

```text
cargo test -p rozsa-gui --test prototype_coverage_inventory_test --test prototype_stylesheet_extraction_test
```

结果：`prototype_coverage_inventory_test` 3 passed，`prototype_stylesheet_extraction_test` 3 passed。

另行通过：

- `node --check docs/gui/new-version/gallery/visual-direction.js`
- `node --check docs/gui/new-version/scenarios/registry.js`
- `cargo fmt --all -- --check`
- `git diff --check`

## 说明

本任务不改变 runtime 应用场景的视觉或交互实现；`rozsa-visual-demo.html` 在 T015 期间保留为配对基线，根目录真源切换属于后续 TASK-T016。截图文件的像素尺寸受浏览器会话 DPR 影响，配对结论以同一 tab、同一 viewport 的 DOM、computed style、交互状态和几何比较为准。
