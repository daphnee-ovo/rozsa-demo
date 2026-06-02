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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_pop() {
        let mut stack = UndoStack::new(100);
        stack.push(EditorSnapshot {
            lines: vec!["hello".to_string()],
            cursor_row: 0,
            cursor_col: 5,
        });
        let s = stack.pop().unwrap();
        assert_eq!(s.lines[0], "hello");
        assert_eq!(s.cursor_col, 5);
        assert!(stack.pop().is_none());
    }

    #[test]
    fn max_size_eviction() {
        let mut stack: UndoStack<u32> = UndoStack::new(3);
        stack.push(1);
        stack.push(2);
        stack.push(3);
        stack.push(4);
        assert_eq!(stack.len(), 3);
        assert_eq!(stack.pop(), Some(4));
        assert_eq!(stack.pop(), Some(3));
        assert_eq!(stack.pop(), Some(2));
    }

    #[test]
    fn clear() {
        let mut stack: UndoStack<u32> = UndoStack::new(100);
        stack.push(1);
        stack.push(2);
        stack.clear();
        assert_eq!(stack.len(), 0);
    }

    #[test]
    fn eviction_preserves_newest() {
        // 验证 VecDeque pop_front 淘汰的是最旧条目
        let mut stack: UndoStack<u32> = UndoStack::new(2);
        stack.push(10); // oldest
        stack.push(20);
        stack.push(30); // triggers eviction of 10
        stack.push(40); // triggers eviction of 20
        assert_eq!(stack.len(), 2);
        // 最新的两个应该保留
        assert_eq!(stack.pop(), Some(40));
        assert_eq!(stack.pop(), Some(30));
    }

    #[test]
    fn is_empty_behavior() {
        let mut stack: UndoStack<u32> = UndoStack::new(10);
        assert!(stack.is_empty());
        stack.push(1);
        assert!(!stack.is_empty());
        stack.pop();
        assert!(stack.is_empty());
    }
}
