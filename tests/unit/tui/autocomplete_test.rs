// slash command 补全单测 — 验证 autocomplete 核心逻辑
//
// 覆盖：
// - is_autocomplete_context 对 "/" 前缀的判定
// - AutocompleteState 上下导航
// - apply_completion 对 slash command 的正确拼接
// - handle_autocomplete_key 的 Enter/Tab/Esc/Up/Down 行为
// - completion_text 的三种模式（slash / dir / normal）

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use rozsa_tui::components::autocomplete::{
    apply_completion, handle_autocomplete_key, AutocompleteAction, AutocompleteState,
};
use rozsa_tui::input::keys::is_autocomplete_context;
use rozsa_tui::protocol::NativeAutocompleteItem;

fn item(value: &str, label: &str, desc: Option<&str>) -> NativeAutocompleteItem {
    NativeAutocompleteItem {
        value: value.to_string(),
        label: label.to_string(),
        description: desc.map(|s| s.to_string()),
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

// --- is_autocomplete_context ---

#[test]
fn slash_prefix_triggers_autocomplete() {
    assert!(is_autocomplete_context("/", 1));
    assert!(is_autocomplete_context("/h", 2));
    assert!(is_autocomplete_context("/help", 5));
    assert!(is_autocomplete_context("/model", 6));
}

#[test]
fn empty_input_does_not_trigger() {
    assert!(!is_autocomplete_context("", 0));
}

#[test]
fn regular_text_does_not_trigger() {
    assert!(!is_autocomplete_context("hello world", 11));
    assert!(!is_autocomplete_context("some text", 9));
}

#[test]
fn at_prefix_triggers_autocomplete() {
    assert!(is_autocomplete_context("read @src/m", 11));
    assert!(is_autocomplete_context("@file.rs", 8));
}

#[test]
fn email_does_not_trigger() {
    assert!(!is_autocomplete_context("email foo@example.com", 6));
}

// --- AutocompleteState navigation ---

#[test]
fn state_navigation_up_down() {
    let items = vec![
        item("help", "/help", Some("Show help")),
        item("hotkeys", "/hotkeys", Some("Show shortcuts")),
        item("clear", "/clear", Some("Clear conversation")),
    ];
    let mut state = AutocompleteState::new("/h".to_string(), items);
    assert_eq!(state.selected, 0);

    state.down();
    assert_eq!(state.selected, 1);

    state.down();
    assert_eq!(state.selected, 2);

    // 不超过最后一个
    state.down();
    assert_eq!(state.selected, 2);

    state.up();
    assert_eq!(state.selected, 1);

    state.up();
    assert_eq!(state.selected, 0);

    // 不低于 0
    state.up();
    assert_eq!(state.selected, 0);
}

// --- apply_completion for slash commands ---

#[test]
fn apply_completion_slash_command() {
    let items = vec![item("help", "/help", Some("Show help"))];
    let state = AutocompleteState::new("/hel".to_string(), items);
    let (result, cursor) = apply_completion("/hel", 4, &state);
    assert_eq!(result, "/help ");
    assert_eq!(cursor, 6);
}

#[test]
fn apply_completion_slash_command_partial() {
    let items = vec![item("model", "/model", Some("Switch model"))];
    let state = AutocompleteState::new("/mo".to_string(), items);
    let (result, cursor) = apply_completion("/mo", 3, &state);
    assert_eq!(result, "/model ");
    assert_eq!(cursor, 7);
}

#[test]
fn apply_completion_at_prefix_directory() {
    // 目录路径补全：label 以 "/" 结尾 → 不加空格
    let items = vec![item("src/components/", "src/components/", None)];
    let state = AutocompleteState::new("@src/c".to_string(), items);
    let (result, cursor) = apply_completion("read @src/c", 11, &state);
    assert_eq!(result, "read src/components/");
    assert_eq!(cursor, 20);
}

#[test]
fn apply_completion_at_file_with_space() {
    // 普通文件补全：label 不以 "/" 结尾 → 加空格
    let items = vec![item("src/main.rs", "src/main.rs", None)];
    let state = AutocompleteState::new("@src/m".to_string(), items);
    let (result, cursor) = apply_completion("@src/m", 6, &state);
    assert_eq!(result, "src/main.rs ");
    assert_eq!(cursor, 12);
}

// --- handle_autocomplete_key ---

#[test]
fn enter_on_slash_command_returns_apply_and_submit() {
    let items = vec![item("help", "/help", None)];
    let state = AutocompleteState::new("/hel".to_string(), items);
    let (next_state, action) = handle_autocomplete_key(key(KeyCode::Enter), state);
    assert!(next_state.is_none());
    assert!(matches!(action, AutocompleteAction::ApplyAndSubmit));
}

#[test]
fn enter_on_at_file_returns_apply_and_edit() {
    let items = vec![item("src/main.rs", "src/main.rs", None)];
    let state = AutocompleteState::new("@src/m".to_string(), items);
    let (next_state, action) = handle_autocomplete_key(key(KeyCode::Enter), state);
    assert!(next_state.is_none());
    assert!(matches!(action, AutocompleteAction::ApplyAndEdit));
}

#[test]
fn tab_returns_apply_and_edit() {
    let items = vec![item("help", "/help", None)];
    let state = AutocompleteState::new("/hel".to_string(), items);
    let (next_state, action) = handle_autocomplete_key(key(KeyCode::Tab), state);
    assert!(next_state.is_none());
    assert!(matches!(action, AutocompleteAction::ApplyAndEdit));
}

#[test]
fn esc_closes_panel() {
    let items = vec![item("help", "/help", None)];
    let state = AutocompleteState::new("/hel".to_string(), items);
    let (next_state, action) = handle_autocomplete_key(key(KeyCode::Esc), state);
    assert!(next_state.is_none());
    assert!(matches!(action, AutocompleteAction::Close));
}

#[test]
fn up_navigates_and_keeps_open() {
    let items = vec![
        item("help", "/help", None),
        item("hotkeys", "/hotkeys", None),
    ];
    let mut state = AutocompleteState::new("/h".to_string(), items);
    state.selected = 1;
    let (next_state, action) = handle_autocomplete_key(key(KeyCode::Up), state);
    assert!(matches!(action, AutocompleteAction::KeepOpen));
    let ns = next_state.unwrap();
    assert_eq!(ns.selected, 0);
}

#[test]
fn down_navigates_and_keeps_open() {
    let items = vec![
        item("help", "/help", None),
        item("hotkeys", "/hotkeys", None),
    ];
    let state = AutocompleteState::new("/h".to_string(), items);
    let (next_state, action) = handle_autocomplete_key(key(KeyCode::Down), state);
    assert!(matches!(action, AutocompleteAction::KeepOpen));
    let ns = next_state.unwrap();
    assert_eq!(ns.selected, 1);
}

// --- KNOWN_COMMANDS 覆盖度 ---

#[test]
fn known_commands_includes_essential_slash_commands() {
    use rozsa_tui::command::KNOWN_COMMANDS;
    let essential = ["help", "clear", "model", "settings", "compact", "theme"];
    for cmd in essential {
        assert!(
            KNOWN_COMMANDS.contains(&cmd),
            "KNOWN_COMMANDS missing essential command: {cmd}"
        );
    }
}

// --- 边界场景 ---

#[test]
fn apply_completion_empty_items_returns_unchanged() {
    let state = AutocompleteState::new("/h".to_string(), vec![]);
    let (result, cursor) = apply_completion("/h", 2, &state);
    assert_eq!(result, "/h");
    assert_eq!(cursor, 2);
}

#[test]
fn slash_with_subpath_is_not_slash_command() {
    // "/src/f" 包含第二个 "/" → Enter 不走 ApplyAndSubmit
    let items = vec![item("src/file.rs", "src/file.rs", None)];
    let state = AutocompleteState::new("/src/f".to_string(), items);
    let (_, action) = handle_autocomplete_key(key(KeyCode::Enter), state);
    assert!(matches!(action, AutocompleteAction::ApplyAndEdit));
}
