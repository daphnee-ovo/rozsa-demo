use rozsa_tui::input::editor::EditorMode;

#[test]
fn editor_mode_default() {
    assert_eq!(EditorMode::default(), EditorMode::Default);
}
