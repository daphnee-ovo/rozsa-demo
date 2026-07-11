use rozsa_app::session::manager::SessionManager;

#[test]
fn forked_session_persists_its_parent_session_path() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("child.jsonl");
    SessionManager::create(
        &path,
        "child".to_string(),
        temp.path().display().to_string(),
        Some("/sessions/parent.jsonl".to_string()),
    )
    .unwrap();

    let sessions = SessionManager::list_dir(temp.path()).unwrap();
    assert_eq!(
        sessions[0].parent_session_path.as_deref(),
        Some("/sessions/parent.jsonl")
    );
}
