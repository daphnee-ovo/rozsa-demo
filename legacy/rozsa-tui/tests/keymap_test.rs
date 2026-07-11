use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rozsa_tui::input::keymap::{matches_action, matches_key_id};

fn bindings(action: &str, keys: Vec<&str>) -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([(
        action.to_string(),
        keys.into_iter().map(str::to_string).collect(),
    )])
}

#[test]
fn matches_control_key_action() {
    let map = bindings("app.model.cycleForward", vec!["ctrl+p"]);
    let key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(matches_action(&map, key, "app.model.cycleForward"));
}

#[test]
fn matches_shift_tab_from_backtab_event() {
    let map = bindings("app.editMode.cycle", vec!["shift+tab"]);
    let key = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
    assert!(matches_action(&map, key, "app.editMode.cycle"));
}

#[test]
fn matches_escape_aliases() {
    assert!(matches_key_id(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        "escape"
    ));
    assert!(matches_key_id(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        "esc"
    ));
}
