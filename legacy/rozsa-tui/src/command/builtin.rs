// command/builtin.rs — 本地命令帮助文本（显示用，不再本地执行）
//
// 所有命令均通过 submit 提交给后端处理。
// 此文件仅保留帮助文本常量供 UI 展示。

/// /help 展示的帮助文本
pub const HELP_TEXT: &str = "\
Available commands (all processed by backend):
  /help        Show help
  /hotkeys     Show keyboard shortcuts
  /clear       Clear and start new conversation
  /model       Switch model
  /compact     Compact context
  /session     Manage sessions
  /settings    Open settings
  /theme       Toggle dark/light theme
  /export      Export session
  /graph       Session timeline
  !<cmd>       Run shell command";

/// /hotkeys 展示的快捷键文本（动态部分由 keymap 提供）
pub const HOTKEYS_TEXT: &str = "\
Keyboard shortcuts:
  Ctrl+C      Abort / clear input
  Ctrl+D      Exit
  Ctrl+L      Select model
  Ctrl+P      Cycle model forward
  Ctrl+O      Toggle compaction details
  Ctrl+T      Toggle thinking
  Ctrl+G      External editor
  Ctrl+Z      Suspend / Undo
  Alt+T       Toggle dark/light theme
  Shift+Tab   Cycle edit mode
  PageUp/Dn   Scroll
  Esc×2       Session graph
  Ctrl+]      Next subagent
  Alt+[       Previous subagent
  Alt+]       Jump forward to char
  Alt+Shift+[ Fold block
  Alt+Shift+] Unfold block";
