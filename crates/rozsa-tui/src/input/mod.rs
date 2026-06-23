// input/mod.rs — 输入处理模块
//
// 内部结构:
// input/
// ├── mod.rs          # 类型定义 + InputState 结构 + 核心数据方法
// ├── keys.rs         # handle_key + 选区、折叠、grapheme 工具、文本编辑操作
// └── mouse.rs        # 鼠标事件 + 粘贴处理
//
// 相关文档:
// - [SPEC](../../../../.dev-doc/refactor/tui/SPEC.md)

pub mod keys;
pub mod mouse;

pub use keys::handle_key;

use std::sync::Arc;

use crate::{
    kill_ring::KillRing,
    undo::{EditorSnapshot, UndoStack},
};

/// Command sink trait — abstracts how the TUI sends commands to the backend.
pub trait CommandSink: Send + Sync {
    fn send_command(&self, msg: &crate::protocol::ClientMessage<'_>) -> Result<(), Box<dyn std::error::Error>>;
}

/// Writer type alias — any CommandSink implementor.
pub type Writer = Arc<dyn CommandSink>;

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum JumpDirection {
    Forward,
    Backward,
}

/// 文本选区锚点
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionAnchor {
    pub row: usize,
    pub col: usize,
}

/// 编辑器中不可部分编辑的原子 span（如 paste marker、图片附件）。
/// 光标不能进入 span 内部，删除时整体移除。
#[derive(Clone, Debug)]
pub struct AtomicSpan {
    pub row: usize,
    pub col_start: usize,
    pub col_len: usize,
}

#[derive(Clone, Debug)]
pub struct InputState {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub undo_stack: UndoStack<EditorSnapshot>,
    pub kill_ring: KillRing,
    pub last_action: Option<crate::kill_ring::LastAction>,
    /// yank 操作插入的文本长度（用于 yank-pop 删除）
    pub(crate) yank_len: usize,
    /// Jump 模式：等待下一个字符输入后跳转
    pub jump_mode: Option<JumpDirection>,
    /// 折叠行范围列表（行号 start..end 会被折叠为一行 "..."）
    pub folded_ranges: Vec<(usize, usize)>,
    /// 文本选区：锚点位置（Shift+方向键开始选择时记录）
    pub selection_anchor: Option<SelectionAnchor>,
    /// 粘贴折叠：存储大段粘贴的原始文本，编辑器中仅显示 marker
    pub pastes: Vec<String>,
    pub paste_counter: usize,
    /// 原子 span 列表：不可部分编辑的区域
    pub atomic_spans: Vec<AtomicSpan>,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            history: Vec::new(),
            history_index: None,
            undo_stack: UndoStack::new(200),
            kill_ring: KillRing::new(),
            last_action: None,
            yank_len: 0,
            jump_mode: None,
            folded_ranges: Vec::new(),
            selection_anchor: None,
            pastes: Vec::new(),
            paste_counter: 0,
            atomic_spans: Vec::new(),
        }
    }
}

impl InputState {
    /// 获取展开 paste marker 后的完整文本（发送给后端时使用）
    pub fn expanded_text(&self) -> String {
        let text = self.text();
        if self.pastes.is_empty() {
            return text;
        }
        let mut result = text;
        for (i, paste_content) in self.pastes.iter().enumerate() {
            let id = i + 1;
            // 替换所有可能的 marker 格式
            let line_count = paste_content.lines().count();
            let char_count = paste_content.len();
            let line_marker = format!("[paste #{id} +{line_count} lines]");
            let char_marker = format!("[paste #{id} {char_count} chars]");
            if result.contains(&line_marker) {
                result = result.replace(&line_marker, paste_content);
            } else if result.contains(&char_marker) {
                result = result.replace(&char_marker, paste_content);
            }
        }
        result
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn set_text(&mut self, text: String) {
        let normalized = crate::normalize_newlines(&text);
        self.lines = normalized.lines().map(String::from).collect();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.cursor_row = self.lines.len() - 1;
        self.cursor_col = keys::grapheme_count(&self.lines[self.cursor_row]);
        self.atomic_spans.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].is_empty()
    }

    pub fn clear(&mut self) {
        self.lines = vec![String::new()];
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.undo_stack.clear();
        self.last_action = None;
        self.pastes.clear();
        self.paste_counter = 0;
        self.selection_anchor = None;
        self.atomic_spans.clear();
    }

    pub fn push_undo(&mut self) {
        self.undo_stack.push(EditorSnapshot {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        });
    }

    pub fn snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        }
    }
}
