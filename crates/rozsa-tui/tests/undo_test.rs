use rozsa_tui::input::undo::{EditorSnapshot, UndoStack};

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
