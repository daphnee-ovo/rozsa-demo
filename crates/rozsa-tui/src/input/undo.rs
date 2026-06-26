// undo.rs
//
// Internal Framework:
// undo.rs
// ├── UndoStack<S>
// │   ├── push()        — 快照推入（clone 语义）
// │   ├── pop()         — 弹出最近快照
// │   ├── clear()       — 清空栈
// │   └── len()         — 快照数量
// └── EditorSnapshot    — 编辑器状态快照类型
//
// Related Docs:
// - [Task T001](../../dev-doc/refactor/tui/task/task_2026-05-28_1.md)

use std::collections::VecDeque;

/// 泛型 Undo 栈，clone-on-push 语义，使用 VecDeque 实现 O(1) 淘汰
#[derive(Clone, Debug)]
pub struct UndoStack<S: Clone> {
    stack: VecDeque<S>,
    max_size: usize,
}

impl<S: Clone> UndoStack<S> {
    pub fn new(max_size: usize) -> Self {
        Self {
            stack: VecDeque::new(),
            max_size,
        }
    }

    pub fn push(&mut self, state: S) {
        if self.stack.len() >= self.max_size {
            self.stack.pop_front();
        }
        self.stack.push_back(state);
    }

    pub fn pop(&mut self) -> Option<S> {
        self.stack.pop_back()
    }

    pub fn clear(&mut self) {
        self.stack.clear();
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
}

/// 编辑器状态快照
#[derive(Clone, Debug)]
pub struct EditorSnapshot {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
}
