use rozsa_tui::input::keys::{
    cursor_char_index, delete_char_backward, delete_char_forward, delete_word_backward_text,
    delete_word_forward_text, grapheme_count, grapheme_skip, grapheme_take, grapheme_to_byte_offset,
    insert_char, insert_text_at_cursor, is_autocomplete_context, is_word_char, jump_to_char, undo,
};
use rozsa_tui::input::kill_ring::PushOpts;
use rozsa_tui::input::{InputState, JumpDirection, SelectionAnchor};

#[test]
fn basic_insert() {
    let mut input = InputState::default();
    insert_char(&mut input, 'h');
    insert_char(&mut input, 'i');
    assert_eq!(input.text(), "hi");
    assert_eq!(input.cursor_col, 2);
}

#[test]
fn word_delete() {
    let mut input = InputState::default();
    input.set_text("hello world".to_string());
    input.cursor_col = 11;
    let deleted = delete_word_backward_text(&mut input);
    assert_eq!(deleted, "world");
    assert_eq!(input.text(), "hello ");
}

#[test]
fn undo_basic() {
    let mut input = InputState::default();
    input.push_undo();
    insert_char(&mut input, 'a');
    insert_char(&mut input, 'b');
    undo(&mut input);
    assert_eq!(input.text(), "");
}

#[test]
fn kill_ring_ctrl_k_ctrl_y() {
    let mut input = InputState::default();
    input.set_text("hello world".to_string());
    input.cursor_col = 5;
    // Ctrl+K: kill to end
    input.push_undo();
    let line = &input.lines[input.cursor_row];
    let deleted: String = line.chars().skip(input.cursor_col).collect();
    input.kill_ring.push(
        &deleted,
        PushOpts {
            prepend: false,
            accumulate: false,
        },
    );
    let line = &mut input.lines[input.cursor_row];
    *line = line.chars().take(input.cursor_col).collect();
    assert_eq!(input.text(), "hello");
    assert_eq!(input.kill_ring.peek(), Some(" world"));

    // Ctrl+Y: yank
    let text = input.kill_ring.peek().unwrap().to_string();
    insert_text_at_cursor(&mut input, &text);
    assert_eq!(input.text(), "hello world");
}

#[test]
fn kill_ring_accumulate() {
    let mut input = InputState::default();
    input.kill_ring.push(
        "first",
        PushOpts {
            prepend: false,
            accumulate: false,
        },
    );
    input.kill_ring.push(
        " second",
        PushOpts {
            prepend: false,
            accumulate: true,
        },
    );
    assert_eq!(input.kill_ring.peek(), Some("first second"));
}

#[test]
fn delete_word_forward() {
    let mut input = InputState::default();
    input.set_text("hello world end".to_string());
    input.cursor_col = 6;
    let deleted = delete_word_forward_text(&mut input);
    assert_eq!(deleted, "world ");
    assert_eq!(input.text(), "hello end");
}

#[test]
fn autocomplete_context_accepts_at_token_after_text() {
    let text = "please read @src/ma";
    assert!(is_autocomplete_context(text, text.chars().count()));
    assert!(!is_autocomplete_context("email foo@example.com", 6));
}

#[test]
fn cursor_char_index_counts_previous_lines() {
    let mut input = InputState::default();
    input.set_text("first\nsecond".to_string());
    input.cursor_row = 1;
    input.cursor_col = 3;
    assert_eq!(cursor_char_index(&input), 9);
}

#[test]
fn jump_forward_to_char() {
    let mut input = InputState::default();
    input.set_text("hello world".to_string());
    input.cursor_col = 0;
    jump_to_char(&mut input, 'o', JumpDirection::Forward);
    assert_eq!(input.cursor_col, 4); // 'o' in "hello"
    jump_to_char(&mut input, 'o', JumpDirection::Forward);
    assert_eq!(input.cursor_col, 7); // 'o' in "world"
}

#[test]
fn jump_backward_to_char() {
    let mut input = InputState::default();
    input.set_text("hello world".to_string());
    input.cursor_col = 10;
    jump_to_char(&mut input, 'l', JumpDirection::Backward);
    assert_eq!(input.cursor_col, 9); // 'l' in "world"
    jump_to_char(&mut input, 'l', JumpDirection::Backward);
    assert_eq!(input.cursor_col, 3); // second 'l' in "hello"
}

// --- Grapheme-aware editing tests ---

#[test]
fn grapheme_insert_emoji() {
    let mut input = InputState::default();
    input.set_text("ab".to_string());
    input.cursor_col = 1;
    insert_char(&mut input, '\u{1F389}');
    assert_eq!(input.text(), "a\u{1F389}b");
    assert_eq!(input.cursor_col, 2);
}

#[test]
fn grapheme_delete_backward_emoji() {
    let mut input = InputState::default();
    input.set_text("a\u{1F389}b".to_string());
    input.cursor_col = 2;
    delete_char_backward(&mut input);
    assert_eq!(input.text(), "ab");
    assert_eq!(input.cursor_col, 1);
}

#[test]
fn grapheme_delete_forward_emoji() {
    let mut input = InputState::default();
    input.set_text("a\u{1F389}b".to_string());
    input.cursor_col = 1;
    delete_char_forward(&mut input);
    assert_eq!(input.text(), "ab");
    assert_eq!(input.cursor_col, 1);
}

#[test]
fn grapheme_count_multibyte() {
    assert_eq!(grapheme_count("hello"), 5);
    assert_eq!(grapheme_count("\u{4F60}\u{597D}\u{4E16}\u{754C}"), 4);
    assert_eq!(grapheme_count("a\u{1F1EF}\u{1F1F5}b"), 3); // flag emoji = 1 grapheme
}

#[test]
fn grapheme_take_and_skip() {
    let s = "a\u{1F389}b\u{1F680}c";
    assert_eq!(grapheme_take(s, 2), "a\u{1F389}");
    assert_eq!(grapheme_skip(s, 2), "b\u{1F680}c");
    assert_eq!(grapheme_take(s, 0), "");
    assert_eq!(grapheme_skip(s, 5), "");
}

#[test]
fn grapheme_to_byte_offset_chinese() {
    let s = "\u{4F60}\u{597D}\u{4E16}\u{754C}";
    assert_eq!(grapheme_to_byte_offset(s, 0), 0);
    assert_eq!(grapheme_to_byte_offset(s, 1), 3); // 一个中文字 3 bytes
    assert_eq!(grapheme_to_byte_offset(s, 4), s.len());
}

#[test]
fn is_word_char_classification() {
    assert!(is_word_char("a"));
    assert!(is_word_char("Z"));
    assert!(is_word_char("_"));
    assert!(is_word_char("5"));
    assert!(!is_word_char("."));
    assert!(!is_word_char(" "));
    assert!(!is_word_char("!"));
}

// --- Word movement with punctuation awareness ---

#[test]
fn word_delete_backward_punctuation() {
    let mut input = InputState::default();
    input.set_text("foo.bar baz".to_string());
    input.cursor_col = 7; // after "foo.bar"
    let deleted = delete_word_backward_text(&mut input);
    assert_eq!(deleted, "bar");
    assert_eq!(input.text(), "foo. baz");
}

#[test]
fn word_delete_forward_punctuation() {
    let mut input = InputState::default();
    input.set_text("foo.bar baz".to_string());
    input.cursor_col = 0;
    let deleted = delete_word_forward_text(&mut input);
    assert_eq!(deleted, "foo");
    assert_eq!(input.text(), ".bar baz");
}

// --- Text selection tests ---

#[test]
fn selection_single_line() {
    let mut input = InputState::default();
    input.set_text("hello world".to_string());
    input.cursor_row = 0;
    input.cursor_col = 6;
    input.selection_anchor = Some(SelectionAnchor { row: 0, col: 0 });
    assert_eq!(input.selection_range(), Some((0, 0, 0, 6)));
    assert_eq!(input.selected_text(), Some("hello ".to_string()));
}

#[test]
fn selection_multi_line() {
    let mut input = InputState::default();
    input.set_text("first\nsecond\nthird".to_string());
    input.cursor_row = 2;
    input.cursor_col = 3;
    input.selection_anchor = Some(SelectionAnchor { row: 0, col: 3 });
    let text = input.selected_text().unwrap();
    assert_eq!(text, "st\nsecond\nthi");
}

#[test]
fn selection_reversed_anchor() {
    let mut input = InputState::default();
    input.set_text("hello world".to_string());
    input.cursor_row = 0;
    input.cursor_col = 2;
    input.selection_anchor = Some(SelectionAnchor { row: 0, col: 8 });
    assert_eq!(input.selection_range(), Some((0, 2, 0, 8)));
    assert_eq!(input.selected_text(), Some("llo wo".to_string()));
}

#[test]
fn selection_empty_returns_none() {
    let mut input = InputState::default();
    input.set_text("hello".to_string());
    input.cursor_row = 0;
    input.cursor_col = 3;
    input.selection_anchor = Some(SelectionAnchor { row: 0, col: 3 });
    assert_eq!(input.selection_range(), None);
}

#[test]
fn delete_selection_single_line() {
    let mut input = InputState::default();
    input.set_text("hello world".to_string());
    input.cursor_row = 0;
    input.cursor_col = 5;
    input.selection_anchor = Some(SelectionAnchor { row: 0, col: 0 });
    let deleted = input.delete_selection().unwrap();
    assert_eq!(deleted, "hello");
    assert_eq!(input.text(), " world");
    assert_eq!(input.cursor_col, 0);
    assert!(input.selection_anchor.is_none());
}

#[test]
fn delete_selection_multi_line() {
    let mut input = InputState::default();
    input.set_text("first\nsecond\nthird".to_string());
    input.cursor_row = 2;
    input.cursor_col = 2;
    input.selection_anchor = Some(SelectionAnchor { row: 0, col: 3 });
    let deleted = input.delete_selection().unwrap();
    assert_eq!(deleted, "st\nsecond\nth");
    assert_eq!(input.text(), "firird");
    assert_eq!(input.cursor_row, 0);
    assert_eq!(input.cursor_col, 3);
}

#[test]
fn clear_selection() {
    let mut input = InputState::default();
    input.selection_anchor = Some(SelectionAnchor { row: 0, col: 5 });
    input.clear_selection();
    assert!(input.selection_anchor.is_none());
}

// --- Multiline delete_char_backward joining lines ---

#[test]
fn delete_backward_joins_lines() {
    let mut input = InputState::default();
    input.set_text("first\nsecond".to_string());
    input.cursor_row = 1;
    input.cursor_col = 0;
    delete_char_backward(&mut input);
    assert_eq!(input.text(), "firstsecond");
    assert_eq!(input.cursor_row, 0);
    assert_eq!(input.cursor_col, 5);
}

// --- delete_char_forward joining lines ---

#[test]
fn delete_forward_joins_lines() {
    let mut input = InputState::default();
    input.set_text("first\nsecond".to_string());
    input.cursor_row = 0;
    input.cursor_col = 5; // at end of "first"
    delete_char_forward(&mut input);
    assert_eq!(input.text(), "firstsecond");
}
