# TASK-T016 根目录原型真源切换验证

## 执行边界

T015 完成后停止继续编辑 `docs/gui/new-version/` 的实现。T016 在同一迁移阶段内将已验证 candidate 搬入根目录 `prototype/`，同步内部路径、规范文档、测试和当前维护文档；没有保留 `rozsa-gui.html` 兼容入口或旧的 `rozsa-visual-demo.html`。旧独立 visual demo 的展示内容已由 `prototype/gallery/visual-direction.*` 承接，T015 截图仍作为重构前后对照材料。

## 根目录内容

根目录 prototype 包含：

- `index.html`、11 个 application scene、`scenarios/registry.js`；
- `styles/`、`templates/`、`scripts/`、`mock/`；
- `gallery/visual-direction.html`、`visual-direction.css`、`visual-direction.js`；
- `docs/ARCHITECTURE.md`、`COMPONENTS.md`、`SCENARIOS.md`、`FLOWS.md`、`VALIDATION.md`、`MOCK_DATA.md`；
- 从覆盖矩阵和缺口表迁入的 `docs/UI_COVERAGE.*`、`docs/DESIGN_GAPS.md`。

根目录样式 source order、模板、mock 和行为模块中的内部路径已统一为 `prototype/` 或当前文件的相对路径。旧单体 CSS、兼容 HTML 和旧 gallery HTML 不存在。

## 根目录 HTTP 与实际页面

服务命令：

```text
uv run python -m http.server 4173 --bind 127.0.0.1 --directory prototype
```

HTTP 检查覆盖默认入口、11 个 scene、统一 gallery query、直接 gallery URL 和 7 个 prototype docs URL，全部返回 200。

浏览器实际核验使用根目录 URL，fresh browser viewport 为 1280px 宽：

- `?gallery=visual-direction` 标题为 `Rozsa · Visual Direction Demo`，无 alert，包含 Empty/Active/Appearance 三个 tab，单个 stylesheet 正常加载。
- Active tab 后仅 active panel 可见；Appearance tab 后仅 appearance panel 可见，两个 tab 的 `aria-selected` 正确。
- 11 个 scene 全部打开成功：scene identity 与 query 一致，无 alert、无 shell error、无横向溢出；每个页面都完成公共 mount。
- 未知 gallery 显示 `Unknown prototype gallery: unknown`，未知 scene 显示 `Unknown prototype scene: unknown`，两者都不加载 application shell。

T015 保存的 `BASE-P-GALLERY-001..004` 截图、DOM、computed style 和交互材料继续作为 gallery 与独立 visual demo 的成对对照；根目录迁移只改变入口路径，不改变 gallery 结构或行为。

## 文档与有效引用

已同步：

- `AGENTS.md` 的 GUI 设计真源；
- `docs/gui/ARCHITECTURE.md`、`UI_USAGE_GUIDELINES.md`、`FRONTEND_INTERFACE_STANDARD.md` 的 prototype 入口与文档链接；
- `docs/MIGRATION_RESIDUE_AUDIT.md` 的已删除旧原型分类；
- `crates/rozsa-gui/tests/native_window_behavior.rs`、coverage test、stylesheet test 的根目录路径；
- prototype docs 内部目录、coverage、source-order 和 gallery 入口。

有效引用清单见同目录 `effective-reference-inventory.md`。历史任务记录与 SPEC 中的旧路径只作为开发证据，不属于当前有效实现引用。

## 自动化检查

```text
cargo test -p rozsa-gui --test prototype_coverage_inventory_test
cargo test -p rozsa-gui --test prototype_stylesheet_extraction_test
cargo test -p rozsa-gui --test prototype_root_cutover_test --test prototype_coverage_inventory_test --test prototype_stylesheet_extraction_test
cargo fmt --all -- --check
git diff --check
```

前两项在旧目录清理前已通过；第三项在旧目录清理后通过。server 在最终检查后关闭，端口 4173 无监听。
