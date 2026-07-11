// terminal_caps.rs
//
// Internal Framework:
// terminal_caps.rs
// ├── ImageProtocol      — Kitty / iTerm2 / None
// ├── TerminalCaps       — 终端能力集合
// └── detect()           — 从环境变量检测终端能力
//
// Related Docs:
// - [TS terminal-image.ts](../../../packages/tui/src/terminal-image.ts)
// - [Task T015](../../dev-doc/refactor/tui/task/task_2026-05-28_1.md)

use std::env;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProtocol {
    Kitty,
    Iterm2,
}

#[derive(Debug, Clone)]
pub struct TerminalCaps {
    pub images: Option<ImageProtocol>,
    pub true_color: bool,
    pub hyperlinks: bool,
}

pub static CAPS: LazyLock<TerminalCaps> = LazyLock::new(detect);

pub fn detect() -> TerminalCaps {
    let term_program = env::var("TERM_PROGRAM").unwrap_or_default().to_lowercase();
    let term = env::var("TERM").unwrap_or_default().to_lowercase();
    let color_term = env::var("COLORTERM").unwrap_or_default().to_lowercase();
    let has_true_color = color_term == "truecolor" || color_term == "24bit";

    // tmux/screen: 不支持 hyperlinks 和 images
    let in_tmux =
        env::var("TMUX").is_ok() || term.starts_with("tmux") || term.starts_with("screen");
    if in_tmux {
        return TerminalCaps {
            images: None,
            true_color: has_true_color,
            hyperlinks: false,
        };
    }

    // Kitty
    if env::var("KITTY_WINDOW_ID").is_ok() || term_program == "kitty" {
        return TerminalCaps {
            images: Some(ImageProtocol::Kitty),
            true_color: true,
            hyperlinks: true,
        };
    }

    // Ghostty (supports Kitty protocol)
    if term_program == "ghostty"
        || term.contains("ghostty")
        || env::var("GHOSTTY_RESOURCES_DIR").is_ok()
    {
        return TerminalCaps {
            images: Some(ImageProtocol::Kitty),
            true_color: true,
            hyperlinks: true,
        };
    }

    // WezTerm (supports Kitty protocol)
    if env::var("WEZTERM_PANE").is_ok() || term_program == "wezterm" {
        return TerminalCaps {
            images: Some(ImageProtocol::Kitty),
            true_color: true,
            hyperlinks: true,
        };
    }

    // iTerm2
    if env::var("ITERM_SESSION_ID").is_ok() || term_program == "iterm.app" {
        return TerminalCaps {
            images: Some(ImageProtocol::Iterm2),
            true_color: true,
            hyperlinks: true,
        };
    }

    // VS Code terminal
    if term_program == "vscode" {
        return TerminalCaps {
            images: None,
            true_color: true,
            hyperlinks: true,
        };
    }

    // Alacritty
    if term_program == "alacritty" {
        return TerminalCaps {
            images: None,
            true_color: true,
            hyperlinks: true,
        };
    }

    // Windows Terminal
    let wt = env::var("WT_SESSION").is_ok();

    TerminalCaps {
        images: None,
        true_color: has_true_color || wt,
        hyperlinks: false,
    }
}
