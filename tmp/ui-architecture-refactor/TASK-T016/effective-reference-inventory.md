# TASK-T016 有效引用清单

## 扫描范围

扫描当前规则、源码、测试、配置和维护文档：

```text
rg -n --hidden --glob '!target/**' --glob '!.git/**' --glob '!tmp/**' --glob '!.dev-doc/**' 'docs/gui/(prototype|new-version)|rozsa-visual-demo\.html' AGENTS.md CONTRIBUTING.md docs crates prototype
```

## 当前有效引用

| 类别 | 当前真源 | 说明 |
| --- | --- | --- |
| 项目规则 | `AGENTS.md` → `prototype/` | GUI 设计真源 |
| GUI 说明 | `docs/gui/ARCHITECTURE.md`、`UI_USAGE_GUIDELINES.md`、`FRONTEND_INTERFACE_STANDARD.md` | 入口、目录职责与验证链接已指向根目录 |
| 原型清单 | `prototype/docs/UI_COVERAGE.json`、`prototype/docs/UI_COVERAGE.md` | 结构化 inventory 与中文审阅入口 |
| 原型缺口 | `prototype/docs/DESIGN_GAPS.md` | 仅收录 partial/missing 项 |
| 原型测试 | `crates/rozsa-gui/tests/prototype_*_test.rs` | 统一使用 `prototype/` |
| native reset fixture | `crates/rozsa-gui/tests/native_window_behavior.rs` | 使用 `prototype/styles/foundations/reset.css` |
| 原型内部 | `prototype/index.html`、`scenarios/registry.js`、`styles/source-order.json`、docs | 不含旧目录或旧 gallery 入口 |

## 已清理的旧引用

以下当前路径在最终切换后不存在，因此不会成为有效引用：

- `docs/gui/prototype/`
- `docs/gui/new-version/`
- `docs/gui/UI_COVERAGE.md`
- `docs/gui/UI_COVERAGE.json`
- `docs/gui/DESIGN_GAPS.md`
- `prototype/rozsa-gui.html`
- `prototype/rozsa-visual-demo.html`

## 允许保留的历史证据

`.dev-doc/` 中的 SPEC、task 和完成记录保留原始路径作为阶段历史，不参与当前实现扫描；T015 对照截图/JSON 保留旧 visual demo 作为 baseline 标签，但不提供可访问的旧入口。除此之外，扫描结果不应出现 `docs/gui/prototype`、`docs/gui/new-version` 或 `rozsa-visual-demo.html` 的当前有效引用。
