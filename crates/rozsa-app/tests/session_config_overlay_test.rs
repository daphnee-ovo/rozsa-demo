use std::path::PathBuf;

use rozsa_app::session::manager::SessionManager;
use rozsa_model::types::{Message, UserContent, UserMessage};
use tempfile::TempDir;

fn create_session(path: &std::path::Path, id: &str, text: &str) {
    let mut manager =
        SessionManager::create(path, id.to_string(), "/workspace".to_string(), None).unwrap();
    manager
        .append_message(Message::User(UserMessage {
            content: UserContent::Text(text.to_string()),
            display_text: None,
            timestamp: 1,
        }))
        .unwrap();
}

#[test]
fn project_session_with_same_id_overrides_global_session() {
    let temp = TempDir::new().unwrap();
    let global = temp.path().join("global/sessions");
    let project = temp.path().join("project/sessions");
    create_session(&global.join("shared.jsonl"), "shared", "global");
    create_session(
        &global.join("global-only.jsonl"),
        "global-only",
        "global only",
    );
    create_session(&project.join("shared.jsonl"), "shared", "project");
    create_session(
        &project.join("project-only.jsonl"),
        "project-only",
        "project only",
    );

    let sessions = SessionManager::list_dirs(&[global, project.clone()]).unwrap();
    assert_eq!(sessions.len(), 3);
    let shared = sessions
        .iter()
        .find(|session| session.id == "shared")
        .unwrap();
    assert_eq!(shared.path, project.join("shared.jsonl"));
    assert_eq!(shared.first_message, "project");
}

#[test]
fn missing_session_layers_are_empty_instead_of_errors() {
    let sessions = SessionManager::list_dirs(&[
        PathBuf::from("/definitely/missing/global"),
        PathBuf::from("/definitely/missing/project"),
    ])
    .unwrap();
    assert!(sessions.is_empty());
}
