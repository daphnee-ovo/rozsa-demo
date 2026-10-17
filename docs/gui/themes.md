# Rózsa 主题配置

本文的 `frontend/` 路径与 Tauri 行为属于旧 Rust 工程。新前端及其原型只共享 `desktop/renderer/`；两套工程的源码、测试、资源、入口和构建产物互不依赖。下文的共享约束仅限各自工程内部。

旧 Rust/Tauri Appearance 位于设置面板的 `Appearance` tab，行为来源是 `crates/rozsa-gui/frontend/ui/templates/features/settings.html` 与 `ui/scripts/features/settings.js`。Electron 产品由 `desktop/renderer/scripts/features/settings.tsx` 提供 React 界面，通过 `get_settings`、`update_setting`、`list_themes`、`get_theme` 与 `save_theme` 调用 Go `productsettings` 和 `themes` 契约；Go 返回持久化后的分层设置及完整主题快照。`global` 与 `project` scope 分别写入对应设置层，`reset_setting` 删除当前层的覆盖值。Renderer 应用主题 token，Electron main 持有窗口 vibrancy；目前 translucent sidebar 的偏好可持久化，但运行时开关尚未接入窗口 vibrancy 更新。

界面只暴露以下运行时选项：

- `Theme Mode`: `system`、`light`、`dark`
- `Font Size`: 5–30px
- Light Theme / Dark Theme：主题、Accent、Background、Foreground、UI font、Code font
- `Display`：macOS Translucent sidebar；该值属于 appearance settings，不属于主题文件

主题模式、字体大小和当前选中的主题会持久化到 `ROZSA_CONFIG_DIR/settings.json`（默认 `~/.rozsa/settings.json`）的 `appearance` 字段。项目设置可由 `ROZSA_PROJECT_CONFIG_DIR/settings.json`（默认 `<project>/.rozsa/settings.json`）覆盖。`system` 模式通过系统 `prefers-color-scheme` 选择 Light 或 Dark，并监听系统主题变化。

Accent、Background、Foreground、UI font 和 Code font 在修改后自动写入当前主题文件。首次修改内置主题时，GUI 会创建稳定的 `rozsa-custom.json` 或 `rozsa-dark-custom.json`，并把对应主题选择写入 `appearance`；之后重新打开设置或重启 GUI 都会恢复这些值。Translucent sidebar 单独持久化在 `appearance`。

## 自定义主题

自定义主题放在：

```text
~/.rozsa/themes/<theme_id>.json
```

`theme_id` 只能包含字母、数字、`-` 和 `_`。文件使用标准 JSON。最小配置如下：

```json
{
  "mode": "dark",
  "name": "My Dark",
  "accent": "#d88991",
  "background": "#1d1a1c",
  "foreground": "#f1e9eb",
  "uiFont": "system-ui, sans-serif",
  "codeFont": "ui-monospace, monospace"
}
```

主题文件也可以使用 `variables` 提供 Appearance UI 未暴露的 CSS 变量；这些变量会直接应用到 GUI 的 CSS 根节点：

```json
{
  "mode": "light",
  "name": "Paper",
  "accent": "#7d4f59",
  "background": "#fbfaf8",
  "foreground": "#211d1f",
  "uiFont": "system-ui, sans-serif",
  "codeFont": "ui-monospace, monospace",
  "variables": {
    "--surface": "#ffffff",
    "--muted": "#746b6f",
    "--border": "#ded8da",
    "--code-bg": "#f1eff0",
    "--sidebar-bg": "#f7f4f5",
    "--titlebar-bg": "#fcfbfb"
  }
}
```

`mode` 必须与 Light/Dark Theme 匹配。主题文件解析失败、字段为空、CSS 变量名非法或值包含不支持的字符时，GUI 会返回明确错误，不会静默加载空主题。

视觉 token 位于共享 `ui/styles/foundations/tokens.css`；新增组件不在宿主保存第二套颜色。设计与缺口边界见 [GUI 架构](./ARCHITECTURE.md)。

代码入口：旧 Rust [`crates/rozsa-app/src/themes.rs`](../../crates/rozsa-app/src/themes.rs) 与 GUI IPC [`crates/rozsa-gui/src/commands.rs`](../../crates/rozsa-gui/src/commands.rs)；Electron React [`desktop/renderer/scripts/features/settings.tsx`](../../desktop/renderer/scripts/features/settings.tsx)、Go product contract [`backend/internal/app/productsettings/productsettings.go`](../../backend/internal/app/productsettings/productsettings.go) 和 theme owner [`backend/internal/app/themes/themes.go`](../../backend/internal/app/themes/themes.go)。
