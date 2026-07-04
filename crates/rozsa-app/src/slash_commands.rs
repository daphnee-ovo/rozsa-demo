// File: slash_commands.rs
//
// Internal Framework:
// slash_commands.rs
// ├── BuiltinSlashCommand          # 单条 builtin 命令的元数据
// ├── BUILTIN_SLASH_COMMANDS       # builtin 命令的静态注册表
// ├── SlashCommandSource           # extension / prompt / skill 来源标记
// ├── SlashCommandInfo             # 动态注册命令（来自 extension/skill）
// ├── AutocompleteItem             # 给 UI 的补全候选项
// ├── AutocompleteEngine
// │   ├── new() / with_dynamic()   # 构造（可注入扩展命令）
// │   ├── set_dynamic()            # 重新加载扩展时刷新
// │   └── complete()               # text + cursor → Vec<AutocompleteItem>
// └── parse_slash_prefix()         # 内部：把光标处的 / 前缀切出来
//
// 迁移自 packages/coding-agent/src/core/slash-commands.ts，保持名称、描述与
// 用法一致以避免用户体验断层。autocomplete 引擎是新写的：原 TS 版本的补全
// 散落在 input/keys.ts，这里集中到一处供 NativeBackend 调用。
//
// Related Docs:
// - [SPEC](../../../dev-doc/main/SPEC.md)
// - 旧实现：packages/coding-agent/src/core/slash-commands.ts

/// Static metadata for a built-in slash command.
#[derive(Debug, Clone)]
pub struct BuiltinSlashCommand {
    pub name: &'static str,
    pub description: &'static str,
    /// Optional usage hint, e.g. `"/compact [prompt]"`.
    pub usage: Option<&'static str>,
    /// Concrete usage examples shown in `/help`.
    pub examples: &'static [&'static str],
}

/// Source of a dynamically-registered slash command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommandSource {
    Extension,
    Prompt,
    Skill,
}

/// A slash command contributed at runtime by an extension, prompt, or skill.
#[derive(Debug, Clone)]
pub struct SlashCommandInfo {
    pub name: String,
    pub description: Option<String>,
    pub source: SlashCommandSource,
}

/// One autocomplete candidate to surface in the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutocompleteItem {
    /// The literal text to insert (without the leading `/`).
    pub value: String,
    /// Display label.
    pub label: String,
    /// Optional one-line description.
    pub description: Option<String>,
}

/// Built-in slash commands. Mirrors `BUILTIN_SLASH_COMMANDS` from
/// `packages/coding-agent/src/core/slash-commands.ts`.
pub const BUILTIN_SLASH_COMMANDS: &[BuiltinSlashCommand] = &[
    BuiltinSlashCommand {
        name: "settings",
        description: "Open settings menu",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "model",
        description: "Select model (opens selector UI)",
        usage: Some("/model [name]"),
        examples: &["/model sonnet:high", "/model"],
    },
    BuiltinSlashCommand {
        name: "scoped-models",
        description: "Enable/disable models for Ctrl+P cycling",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "export",
        description: "Export session (HTML default, or specify path: .html/.jsonl)",
        usage: Some("/export [format|path]"),
        examples: &["/export html", "/export md", "/export ./session.jsonl"],
    },
    BuiltinSlashCommand {
        name: "import",
        description: "Import and resume a session from a JSONL file",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "share",
        description: "Share session as a secret GitHub gist",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "copy",
        description: "Copy last agent message to clipboard",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "name",
        description: "Set session display name",
        usage: Some("/name [session-name]"),
        examples: &["/name auth-refactor", "/name"],
    },
    BuiltinSlashCommand {
        name: "session",
        description: "Show session info and stats",
        usage: Some("/session [id]"),
        examples: &["/session"],
    },
    BuiltinSlashCommand {
        name: "subagents",
        description: "List or switch subagent views",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "main",
        description: "Switch back to the main agent view",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "changelog",
        description: "Show changelog entries",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "help",
        description: "Show help (topics: permissions, sessions, commands)",
        usage: Some("/help [topic|command]"),
        examples: &["/help compact", "/help permissions", "/help"],
    },
    BuiltinSlashCommand {
        name: "hotkeys",
        description: "Show all keyboard shortcuts",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "fork",
        description: "Create a new fork from a previous user message",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "clone",
        description: "Duplicate the current session at the current position",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "tree",
        description: "Navigate session tree (switch branches)",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "graph",
        description: "Visual session timeline (git graph style)",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "login",
        description: "Configure provider authentication",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "logout",
        description: "Remove provider authentication",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "usage",
        description: "Show rate limit usage for codex-oauth",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "new",
        description: "Start a new session",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "compact",
        description: "Manually compact the session context",
        usage: Some("/compact [prompt]"),
        examples: &["/compact", "/compact focus on the auth refactor"],
    },
    BuiltinSlashCommand {
        name: "permissions",
        description: "Show permission decisions for this session",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "resume",
        description: "Resume a different session",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "reload",
        description: "Reload keybindings, extensions, skills, prompts, and themes",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "search",
        description: "Search tool outputs for a pattern",
        usage: Some("/search <pattern>"),
        examples: &["/search error", "/search TODO", "/search 'function.*init'"],
    },
    BuiltinSlashCommand {
        name: "quit",
        description: "Quit Rózsa",
        usage: None,
        examples: &[],
    },
    BuiltinSlashCommand {
        name: "gc",
        description: "Clean up old session files",
        usage: Some("/gc [days]"),
        examples: &["/gc", "/gc 7"],
    },
    BuiltinSlashCommand {
        name: "lsp",
        description: "Configure LSP auto-diagnostics mode",
        usage: Some("/lsp [agent_end|edit_write|disabled]"),
        examples: &["/lsp", "/lsp agent_end", "/lsp disabled"],
    },
];

/// Fan-out source for autocomplete: builtin + dynamic.
#[derive(Default)]
pub struct AutocompleteEngine {
    dynamic: Vec<SlashCommandInfo>,
}

impl AutocompleteEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_dynamic(dynamic: Vec<SlashCommandInfo>) -> Self {
        Self { dynamic }
    }

    /// Replace the dynamic command list (call when extensions reload).
    pub fn set_dynamic(&mut self, dynamic: Vec<SlashCommandInfo>) {
        self.dynamic = dynamic;
    }

    /// Compute candidates for the given input + cursor position.
    ///
    /// Returns `None` when the cursor is not inside a `/...` token
    /// (e.g. plain message input — the UI should hide its menu).
    pub fn complete(&self, text: &str, cursor: usize) -> Option<Vec<AutocompleteItem>> {
        let prefix = parse_slash_prefix(text, cursor)?;
        let prefix_lower = prefix.to_ascii_lowercase();

        let mut items: Vec<AutocompleteItem> = Vec::new();

        for cmd in BUILTIN_SLASH_COMMANDS {
            if cmd.name.starts_with(&prefix_lower) {
                items.push(AutocompleteItem {
                    value: cmd.name.to_string(),
                    label: format!("/{}", cmd.name),
                    description: Some(cmd.description.to_string()),
                });
            }
        }

        for cmd in &self.dynamic {
            if cmd.name.to_ascii_lowercase().starts_with(&prefix_lower) {
                items.push(AutocompleteItem {
                    value: cmd.name.clone(),
                    label: format!("/{}", cmd.name),
                    description: cmd.description.clone(),
                });
            }
        }

        items.sort_by(|a, b| a.value.cmp(&b.value));
        Some(items)
    }
}

/// If the cursor is positioned inside a slash-command token (the line begins
/// with `/` after optional leading whitespace and there is no whitespace
/// between `/` and the cursor), return the substring between `/` and the
/// cursor (without the leading `/`).
pub(crate) fn parse_slash_prefix(text: &str, cursor: usize) -> Option<&str> {
    if cursor > text.len() {
        return None;
    }
    let head = &text[..cursor];
    let trimmed = head.trim_start();
    if !trimmed.starts_with('/') {
        return None;
    }
    let after_slash = &trimmed[1..];
    if after_slash.contains(char::is_whitespace) {
        return None;
    }
    Some(after_slash)
}
