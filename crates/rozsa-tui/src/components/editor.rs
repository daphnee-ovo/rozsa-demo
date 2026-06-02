// editor_component.rs — 编辑器组件接口
//
// Internal Framework:
// editor_component.rs
// ├── EditorMode            — 编辑器模式枚举
// ├── EditorComponent trait — 编辑器抽象接口
// └── DefaultEditor        — 默认实现（包装 InputState）
//
// Related Docs:
// - [TS editor-component](../../../packages/tui/src/editor-component.ts)

use crossterm::event::KeyEvent;

/// 编辑器模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    /// 默认模式（Emacs 风格）
    Default,
    /// Vim Normal 模式
    VimNormal,
    /// Vim Insert 模式
    VimInsert,
}

impl Default for EditorMode {
    fn default() -> Self {
        Self::Default
    }
}

/// 编辑器组件 trait — 定义编辑器行为接口
pub trait EditorComponent {
    /// 获取当前文本内容
    fn text(&self) -> String;

    /// 设置文本内容
    fn set_text(&mut self, text: String);

    /// 是否为空
    fn is_empty(&self) -> bool;

    /// 清空编辑器
    fn clear(&mut self);

    /// 处理按键事件，返回是否已消费
    fn handle_key(&mut self, key: KeyEvent) -> bool;

    /// 当前光标行
    fn cursor_row(&self) -> usize;

    /// 当前光标列
    fn cursor_col(&self) -> usize;

    /// 获取所有行
    fn lines(&self) -> &[String];

    /// 当前编辑器模式
    fn mode(&self) -> EditorMode {
        EditorMode::Default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_mode_default() {
        assert_eq!(EditorMode::default(), EditorMode::Default);
    }
}
