# Rózsa 主题配置

Appearance 设置位于 GUI 设置面板的 `Appearance` tab。界面只暴露以下运行时选项：

- `Theme Mode`: `system`、`light`、`dark`
- `Font Size`: 5–50px
- Light Theme / Dark Theme：主题、Accent、Background、Foreground、UI font、macOS Translucent sidebar、Code font

主题模式、字体大小和当前选中的主题会持久化到 `~/.rozsa/agent/settings.json` 的 `appearance` 字段。`system` 模式通过系统 `prefers-color-scheme` 选择 Light 或 Dark，并监听系统主题变化。

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
  "translucentSidebar": true,
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
  "translucentSidebar": false,
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

代码入口：[`crates/rozsa-app/src/themes.rs`](../../crates/rozsa-app/src/themes.rs)；GUI IPC 入口：[`crates/rozsa-gui/src/commands.rs`](../../crates/rozsa-gui/src/commands.rs)。
