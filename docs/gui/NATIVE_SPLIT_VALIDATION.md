# 原生分栏验证

macOS 产品窗口使用持久 NSSplitViewController 和两个真实 Tauri WKWebView。实现入口为 [native_split_view.rs](../../crates/rozsa-gui/src/native_split_view.rs)、[native_titlebar.rs](../../crates/rozsa-gui/src/native_titlebar.rs) 与 [scene_router.rs](../../crates/rozsa-gui/src/scene_router.rs)。架构与共享 UI 边界见 [ARCHITECTURE.md](./ARCHITECTURE.md)。

## 验证方法

用 `./run.sh` 启动 staged debug app，`./run.sh --prepare-only` 核对 bundle。需要 Inspector 时显式设置 `ROZSA_WEB_INSPECTOR=1`，默认不开启。用真实窗口操作验证，不能把浏览器 fixture 或 source contract 当成原生操作结果。验证结束关闭测试 app。

检查 main/sidebar readiness 后统一显示、Main/Settings 切换、divider、collapse/restore、主题 backing、focus、viewport 与正常退出。原生分栏的收起和背景由 AppKit 管理，不交给浏览器自动收起逻辑。响应式以全窗口 outer width / scale factor 为准；共享 DOM 的布局仍按所在 main 区域计算。

## 本次共享 UI 验证范围

| 范围 | 验证结论 |
| --- | --- |
| macOS 双 WebView 与 Main/Settings | 真实 app 重放已覆盖页面与操作，共享 UI 正常展示 |
| 全窗口响应式 | 1280px 窗口与 705px 窄窗口实际重放；原生 sidebar 收起后 question 与原型同条件几何一致 |
| Permission / Question | 本地模拟 AI API 通过真实 agent/tool 路径触发；选择、Hint、提交及 session 切换恢复实际验证 |
| Read / Edit / Bash、thinking / Markdown | 真实 app 展示工具结果与代码，DOM/style 与同 payload 的共享原型成对核对 |
| Notification | 实际设置写入失败显示右上角通知与 tray；修复条件后重试成功 resolve |
| Dev Flow | 实际 Settings、sidebar claimed row、detail 与 Files 操作；未覆盖 done_when slot 留空 |
| 异步失败/乱序契约 | Node 与 Rust 契约验证；未人为制造所有原生 transport failure |
| Linux / Windows 真实视觉 | pending platform visual verification；自动化契约不替代实际图形环境 |

本表只描述本次迭代的验收范围。未重新执行的 fullscreen、Finder drop、活动中文 IME 和 sidebar Inspector 等原生组合，不因共享 UI 验收被宣称为本次实际验证通过。产品附件/拖放入口还受设计缺口约束。

## 自动化与限制

相关 Rust 测试位于 crate 的 `tests/`，覆盖 native split host、window behavior、theme surface、scene continuity 和非 macOS 单 WebView 布局契约。共享 UI 专项编排见 [CONTRIBUTING.md](../../CONTRIBUTING.md)。

缺失设计入口暂停；无法安全暂停的被动状态保留原实现并标为受阻。完整边界见 [UI_COVERAGE](../../prototype/docs/UI_COVERAGE.md) 和 [DESIGN_GAPS](../../prototype/docs/DESIGN_GAPS.md)。实际验证素材保存在项目 `tmp/`，不进入架构规范。
