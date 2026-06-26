use rozsa_tui::data::session_tree::build_and_flatten;
use rozsa_tui::panels::session_selector::SessionEntry;

fn make_entry(path: &str, parent: Option<&str>, modified: &str) -> SessionEntry {
    SessionEntry {
        path: path.to_string(),
        name: None,
        first_message: format!("msg for {path}"),
        cwd: "/tmp".to_string(),
        message_count: 1,
        last_modified: modified.to_string(),
        parent_session_path: parent.map(|s| s.to_string()),
        all_messages_text: String::new(),
    }
}

#[test]
fn test_flat_no_parents() {
    let entries = vec![
        make_entry("/a", None, "2026-05-29T03:00:00Z"),
        make_entry("/b", None, "2026-05-29T02:00:00Z"),
        make_entry("/c", None, "2026-05-29T01:00:00Z"),
    ];
    let indices = vec![0, 1, 2];
    let flat = build_and_flatten(&entries, &indices);
    assert_eq!(flat.len(), 3);
    // 按 last_modified 降序
    assert_eq!(flat[0].entry_index, 0);
    assert_eq!(flat[1].entry_index, 1);
    assert_eq!(flat[2].entry_index, 2);
    assert_eq!(flat[0].depth, 0);
}

#[test]
fn test_parent_child_tree() {
    let entries = vec![
        make_entry("/parent", None, "2026-05-29T03:00:00Z"),
        make_entry("/child1", Some("/parent"), "2026-05-29T02:00:00Z"),
        make_entry("/child2", Some("/parent"), "2026-05-29T01:00:00Z"),
    ];
    let indices = vec![0, 1, 2];
    let flat = build_and_flatten(&entries, &indices);
    assert_eq!(flat.len(), 3);
    assert_eq!(flat[0].entry_index, 0);
    assert_eq!(flat[0].depth, 0);
    assert_eq!(flat[1].entry_index, 1);
    assert_eq!(flat[1].depth, 1);
    assert!(!flat[1].is_last);
    assert_eq!(flat[2].entry_index, 2);
    assert_eq!(flat[2].depth, 1);
    assert!(flat[2].is_last);
}

#[test]
fn test_orphan_parent_becomes_root() {
    let entries = vec![
        make_entry("/a", Some("/nonexistent"), "2026-05-29T02:00:00Z"),
        make_entry("/b", None, "2026-05-29T01:00:00Z"),
    ];
    let indices = vec![0, 1];
    let flat = build_and_flatten(&entries, &indices);
    assert_eq!(flat.len(), 2);
    // /a 有无效 parent，归为根节点
    assert_eq!(flat[0].depth, 0);
    assert_eq!(flat[1].depth, 0);
}

#[test]
fn test_nested_three_levels() {
    let entries = vec![
        make_entry("/root", None, "2026-05-29T03:00:00Z"),
        make_entry("/mid", Some("/root"), "2026-05-29T02:00:00Z"),
        make_entry("/leaf", Some("/mid"), "2026-05-29T01:00:00Z"),
    ];
    let indices = vec![0, 1, 2];
    let flat = build_and_flatten(&entries, &indices);
    assert_eq!(flat.len(), 3);
    assert_eq!(flat[0].depth, 0); // /root
    assert_eq!(flat[1].depth, 1); // /mid
    assert_eq!(flat[2].depth, 2); // /leaf
}
