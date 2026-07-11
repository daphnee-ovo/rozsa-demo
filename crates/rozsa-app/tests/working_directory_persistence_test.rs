use std::sync::Arc;

use rozsa_app::session::manager::SessionManager;
use rozsa_app::tools::bash::BashTool;
use rozsa_core::tool::Tool;

#[test]
fn session_manager_reopens_with_the_updated_cwd() {
    let temp = tempfile::tempdir().unwrap();
    let child = temp.path().join("child");
    std::fs::create_dir(&child).unwrap();
    let session_path = temp.path().join("session.jsonl");
    let mut manager = SessionManager::create(
        &session_path,
        "session".to_string(),
        temp.path().to_string_lossy().to_string(),
        None,
    )
    .unwrap();

    manager
        .set_cwd(child.to_string_lossy().to_string())
        .unwrap();
    let reopened = SessionManager::open(&session_path).unwrap();

    assert_eq!(reopened.cwd(), child.to_string_lossy());
}

#[tokio::test]
async fn bash_cd_updates_runtime_and_persisted_session_cwd() {
    let temp = tempfile::tempdir().unwrap();
    let child = temp.path().join("child");
    std::fs::create_dir(&child).unwrap();
    let session_path = temp.path().join("session.jsonl");
    let manager = Arc::new(tokio::sync::Mutex::new(
        SessionManager::create(
            &session_path,
            "session".to_string(),
            temp.path().to_string_lossy().to_string(),
            None,
        )
        .unwrap(),
    ));
    let current_cwd = Arc::new(tokio::sync::Mutex::new(temp.path().to_path_buf()));
    let bash = BashTool::new_with_session(
        temp.path().to_path_buf(),
        current_cwd.clone(),
        manager.clone(),
    );

    let result = bash
        .execute(
            "cd",
            serde_json::json!({"command": format!("cd '{}'", child.display())}),
            None,
            None,
        )
        .await
        .unwrap();

    assert!(result.content.iter().all(|block| match block {
        rozsa_model::types::ContentBlock::Text { text, .. } => !text.contains("__ROZSA_CWD__"),
        _ => true,
    }));
    assert_eq!(*current_cwd.lock().await, child);
    assert_eq!(manager.lock().await.cwd(), child.to_string_lossy());
    assert_eq!(
        SessionManager::open(&session_path).unwrap().cwd(),
        child.to_string_lossy()
    );
}

#[test]
fn relative_cwd_path_is_canonicalized_before_session_use() {
    let temp = tempfile::tempdir().unwrap();
    let child = temp.path().join("child");
    std::fs::create_dir(&child).unwrap();

    assert_eq!(
        temp.path().join("child/../child").canonicalize().unwrap(),
        child.canonicalize().unwrap()
    );
}
