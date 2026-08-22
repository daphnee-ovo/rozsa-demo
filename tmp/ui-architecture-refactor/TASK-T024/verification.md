# TASK-T024 验证记录

## 结论

- sidebar 收起边界：`780px`。
- sidebar 展开滞回边界：`820px`。
- 产品与 prototype 最低窗口：`640×480`。
- 响应式规则已合并回实际 owner CSS；17 个 `*-responsive-*.css` 文件已移入废纸篓，入口不再 import 这些文件。

## 阈值依据

| 场景 | viewport | sidebar | main panel | Composer | 依据 |
| --- | ---: | ---: | ---: | ---: | --- |
| 正常 PC | 1280×720，DPR2 | 256px | 1024px | 900px | in-app browser 当前 prototype 实测 |
| PC 分屏基线 | 820×900，DPR1 | 229.6px | 590.4px | 541.2px | TASK-T002 原始窄屏截图与当前 token/formula 复核 |
| 收起边界 | 780px | 约 220px | 约 560px | 约 510px | `220px sidebar + 560px main comfort width` |
| 最低允许窗口 | 640×480 | overlay/drawer | 640px | 约 616px | CSS/Tauri 最低约束与 Composer padding 公式复核 |

780px 以下主区优先，sidebar 退为隐藏的 overlay/drawer；820px 以上才恢复展开，避免窗口在临界区间来回缩放时反复切换。

## 浏览器与 DOM

环境沿用 TASK-T002：macOS、Codex in-app browser、prototype HTTP server。当前修改后在 `index.html?scene=empty-session` 默认 viewport `1280×720 DPR2` 实测：

- `app-body`: `1280×702.984px`，grid `256px 1024px`。
- `sidebar`: `x=0, y=42, width=256px`。
- `main-panel`: `x=256, y=42, width=1024px`。
- `panel-header`: `width=1024px, height=64px`。
- `chat-input`: `width=1024px, height=153px`。
- `.input-wrapper`: `x=318, width=900px, height=115px`。
- `.rich-input`: `width=898px, height=56px`。
- `sidebar-collapsed`: `false`；当前 viewport 不命中 `max-width: 780px`。

820px 窄屏原始截图为 [`empty-session-narrow.png`](../TASK-T002/prototype-screenshots/empty-session-narrow.png)，显示 sidebar 保持 inline、主区和 Composer 可用；本任务只把布局收起边界从原 `760px` 调整为测量得到的 `780px`，因此 820px 基线不触发新收起规则。

in-app browser 当前没有 viewport resize capability，故 `640×480` 通过 CSS media/query、Tauri config、runtime boundary test 和 Composer 几何公式完成确定性复核；没有把未实际截图的尺寸伪称为浏览器截图。

## 自动化验证

```text
cargo test -p rozsa-gui --test prototype_stylesheet_extraction_test \
  --test prototype_coverage_inventory_test \
  --test native_window_behavior
```

结果：`3 + 6 + 9` 个测试全部通过。

此外通过：

- `cargo fmt --all -- --check`
- `source-order.json` 静态 CSS 前缀重建、owner/media 映射与入口无断点文件 import 检查。
- prototype / Tauri `640×480` 最低窗口一致性检查。
- runtime `780px` 收起、`820px` 展开滞回检查。

## 未改变范围

- 1280px 桌面基准下的视觉、文案、mock 数据和交互语义未改。
- 未进入 gallery 真源切换。
- macOS native split host 仍由现有 AppKit split/titlebar ownership 管理；本任务只调整 webview viewport auto-collapse 链路和 prototype 对齐约束。
