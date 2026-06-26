use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};
use rozsa_tui::app::{should_process_key_event, AppState};
use rozsa_tui::input::mouse::handle_mouse;

#[test]
fn ignores_key_release_events() {
    assert!(should_process_key_event(KeyEvent::new_with_kind(
        KeyCode::Char('w'),
        KeyModifiers::NONE,
        KeyEventKind::Press,
    )));
    assert!(should_process_key_event(KeyEvent::new_with_kind(
        KeyCode::Char('w'),
        KeyModifiers::NONE,
        KeyEventKind::Repeat,
    )));
    assert!(!should_process_key_event(KeyEvent::new_with_kind(
        KeyCode::Char('w'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    )));
}

#[test]
fn mouse_wheel_scrolls_one_line_per_tick() {
    let mut state = AppState::new();
    handle_mouse(
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        },
        &mut state,
    );
    assert_eq!(state.scroll, 1);
    assert!(!state.auto_scroll);

    handle_mouse(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        },
        &mut state,
    );
    assert_eq!(state.scroll, 0);
    assert!(state.auto_scroll);
}
