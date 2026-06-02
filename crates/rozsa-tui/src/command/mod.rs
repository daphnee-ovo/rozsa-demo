// command/mod.rs — 命令注册中心
//
// 本地命令通过 LOCAL_COMMANDS 注册，同时用于：
// - autocomplete 注入（用户输入 / 时显示）
// - 本地拦截执行（不发给后端）
// 后端命令列表仅用于帮助文本展示。
//
// 相关文档:
// - [comparison-roadmap](../../../../docs/comparison-roadmap.md)

pub mod builtin;

/// 本地命令注册表 (command_name, description)
/// 这些命令由 TUI 本地拦截处理，不发给后端
pub const LOCAL_COMMANDS: &[(&str, &str)] = &[
    ("theme", "Toggle dark/light theme"),
];

/// 判断是否为本地命令（需要在 submit 前拦截）
pub fn is_local_command(text: &str) -> bool {
    LOCAL_COMMANDS.iter().any(|(cmd, _)| {
        text == format!("/{cmd}")
            || text.starts_with(&format!("/{cmd} "))
    })
}

/// 所有已知命令列表（含本地 + 后端，用于帮助文本展示）
pub const KNOWN_COMMANDS: &[&str] = &[
    "help", "hotkeys", "clear", "model", "compact", "session", "settings",
    "theme", "export", "import", "share", "copy", "name", "subagents", "main",
    "changelog", "fork", "clone", "tree", "graph", "new", "permissions",
    "resume", "reload", "search", "quit", "gc", "lsp",
];
