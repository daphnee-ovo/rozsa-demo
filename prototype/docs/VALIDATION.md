# 原型验证方法

## 服务器与环境

从仓库根目录运行：

```text
uv run python -m http.server 4173 --bind 127.0.0.1 --directory prototype
```

记录浏览器/WebView、viewport、DPR、theme、字体、reduced-motion、滚动位置、focus、展开项、选中项和使用的 mock。每个检查 ID 使用 fresh browser context，明确将页面 zoom 固定为 100%，并在截图 metadata 中记录 zoom；不得用被浏览器缩放或裁切的截图替代整体截图。验证结束关闭 server。

## 分层验证

1. 文件组织单元：比较 CSS rule block/source order、template 稳定 DOM、script 输入输出和事件绑定。
2. component：覆盖状态、键盘、focus、展开和选择。
3. feature/flow：重放数据传递、提交、成功、失败、取消和恢复。
4. 实际界面：打开全部 scene 与 gallery，使用同一 verification ID、fixture、环境和操作步骤与 baseline 成对比较。

页面能加载、文件数量一致或专项测试通过，都不能替代截图、DOM、computed style 和真实交互证据。

## 全部入口

逐一打开 `scenarios/registry.js` 中的 11 个 scene 与 `?gallery=visual-direction`。检查没有 alert、`shellError`、未知 fallback、横向溢出或资源 404，并核对完整页面中 sidebar、main content、running panels、Composer 和 settings 的位置。application scene 还必须确认 `data-template-mount="sidebar|conversation"` 不改变原始 grid sizing，并确认 registry 声明的 scene stylesheet 已加载。对 gallery 的 Empty、Active、Appearance 分别保存整体截图、稳定 DOM、computed style、focus 和 tab 操作结果。

## 交互重放

至少重放 AskUserQuestion 选择/提交/取消、PermissionRequest hint/错误/取消、notification dismiss、settings theme、Composer 输入/发送和 Dev Flow detail。记录操作后的值、焦点、可见性和恢复结果。

## 自动化检查

在根目录执行：

```text
cargo test -p rozsa-gui --test prototype_root_cutover_test --test prototype_coverage_inventory_test --test prototype_stylesheet_extraction_test
cargo fmt --all -- --check
git diff --check
```

专项测试检查路径、coverage 双向闭合、CSS source order、外置样式、gallery 入口和旧路径删除；视觉等价仍以实际页面证据为准。详细本阶段记录见 `tmp/ui-architecture-refactor/TASK-T016/verification.md`。
