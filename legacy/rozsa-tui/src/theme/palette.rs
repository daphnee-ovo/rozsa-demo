// theme/palette.rs — 颜色调色板定义
//
// Internal Framework:
// palette.rs
// ├── Theme (struct)        — 主题颜色字段集
// ├── Default for Theme     — 默认 dark
// ├── Theme::dark_const()   — const dark 构造
// ├── Theme::dark()         — dark 调色板
// └── Theme::light()        — light 调色板
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

use ratatui::style::Color;

#[derive(Clone, Debug)]
pub struct Theme {
    // 核心 UI 颜色
    pub accent: Color,
    pub border: Color,
    pub border_accent: Color,
    pub border_muted: Color,
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub muted: Color,
    pub dim: Color,
    pub text: Color,

    // 消息颜色
    pub user_msg: Color,
    pub assistant_msg: Color,
    pub tool_call: Color,

    // Markdown
    pub heading: Color,
    pub md_link: Color,
    pub md_code: Color,
    pub md_code_block: Color,
    pub md_list_bullet: Color,
    pub md_quote: Color,
    pub md_quote_border: Color,

    // 背景色
    pub selected_bg: Color,
    pub user_message_bg: Color,
    pub tool_pending_bg: Color,
    pub tool_success_bg: Color,
    pub tool_error_bg: Color,
    pub custom_message_bg: Color,
    pub custom_message_label: Color,

    // Bash
    pub bash_mode: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    pub const fn dark_const() -> Self {
        Self {
            accent: Color::Rgb(138, 190, 183),
            border: Color::Rgb(95, 135, 255),
            border_accent: Color::Rgb(0, 215, 255),
            border_muted: Color::Rgb(80, 80, 80),
            success: Color::Rgb(181, 189, 104),
            error: Color::Rgb(204, 102, 102),
            warning: Color::Rgb(255, 255, 0),
            muted: Color::Rgb(128, 128, 128),
            dim: Color::Rgb(102, 102, 102),
            text: Color::Rgb(212, 212, 212),
            user_msg: Color::Rgb(212, 212, 212),
            assistant_msg: Color::Rgb(181, 189, 104),
            tool_call: Color::Rgb(138, 190, 183),
            heading: Color::Rgb(240, 198, 116),
            md_link: Color::Rgb(129, 162, 190),
            md_code: Color::Rgb(138, 190, 183),
            md_code_block: Color::Rgb(181, 189, 104),
            md_list_bullet: Color::Rgb(138, 190, 183),
            md_quote: Color::Rgb(128, 128, 128),
            md_quote_border: Color::Rgb(128, 128, 128),
            selected_bg: Color::Rgb(58, 58, 74),
            user_message_bg: Color::Rgb(52, 53, 65),
            tool_pending_bg: Color::Rgb(40, 40, 50),
            tool_success_bg: Color::Rgb(40, 50, 40),
            tool_error_bg: Color::Rgb(60, 40, 40),
            custom_message_bg: Color::Rgb(45, 40, 56),
            custom_message_label: Color::Rgb(149, 117, 205),
            bash_mode: Color::Rgb(181, 189, 104),
        }
    }

    pub fn dark() -> Self {
        Self {
            accent: Color::Rgb(138, 190, 183),
            border: Color::Rgb(95, 135, 255),
            border_accent: Color::Rgb(0, 215, 255),
            border_muted: Color::Rgb(80, 80, 80),
            success: Color::Rgb(181, 189, 104),
            error: Color::Rgb(204, 102, 102),
            warning: Color::Rgb(255, 255, 0),
            muted: Color::Rgb(128, 128, 128),
            dim: Color::Rgb(102, 102, 102),
            text: Color::Rgb(212, 212, 212),

            user_msg: Color::Rgb(212, 212, 212),
            assistant_msg: Color::Rgb(181, 189, 104),
            tool_call: Color::Rgb(138, 190, 183),

            heading: Color::Rgb(240, 198, 116),
            md_link: Color::Rgb(129, 162, 190),
            md_code: Color::Rgb(138, 190, 183),
            md_code_block: Color::Rgb(181, 189, 104),
            md_list_bullet: Color::Rgb(138, 190, 183),
            md_quote: Color::Rgb(128, 128, 128),
            md_quote_border: Color::Rgb(128, 128, 128),

            selected_bg: Color::Rgb(58, 58, 74),
            user_message_bg: Color::Rgb(52, 53, 65),
            tool_pending_bg: Color::Rgb(40, 40, 50),
            tool_success_bg: Color::Rgb(40, 50, 40),
            tool_error_bg: Color::Rgb(60, 40, 40),
            custom_message_bg: Color::Rgb(45, 40, 56),
            custom_message_label: Color::Rgb(149, 117, 205),

            bash_mode: Color::Rgb(181, 189, 104),
        }
    }

    pub fn light() -> Self {
        Self {
            accent: Color::Rgb(0, 128, 128),
            border: Color::Rgb(50, 100, 200),
            border_accent: Color::Rgb(0, 150, 200),
            border_muted: Color::Rgb(180, 180, 180),
            success: Color::Rgb(60, 140, 60),
            error: Color::Rgb(200, 50, 50),
            warning: Color::Rgb(180, 140, 0),
            muted: Color::Rgb(120, 120, 120),
            dim: Color::Rgb(160, 160, 160),
            text: Color::Rgb(30, 30, 30),

            user_msg: Color::Rgb(30, 30, 30),
            assistant_msg: Color::Rgb(60, 140, 60),
            tool_call: Color::Rgb(0, 128, 128),

            heading: Color::Rgb(180, 120, 0),
            md_link: Color::Rgb(50, 100, 180),
            md_code: Color::Rgb(0, 128, 128),
            md_code_block: Color::Rgb(60, 140, 60),
            md_list_bullet: Color::Rgb(0, 128, 128),
            md_quote: Color::Rgb(100, 100, 100),
            md_quote_border: Color::Rgb(150, 150, 150),

            selected_bg: Color::Rgb(220, 220, 240),
            user_message_bg: Color::Rgb(240, 240, 245),
            tool_pending_bg: Color::Rgb(245, 245, 250),
            tool_success_bg: Color::Rgb(235, 250, 235),
            tool_error_bg: Color::Rgb(255, 235, 235),
            custom_message_bg: Color::Rgb(240, 235, 250),
            custom_message_label: Color::Rgb(100, 60, 180),

            bash_mode: Color::Rgb(60, 140, 60),
        }
    }
}
