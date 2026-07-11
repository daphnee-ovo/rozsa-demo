use rozsa_app::tools::bash::BashTool;
use rozsa_app::tools::file_delta::{FileDeltaStatus, build_file_delta};
use rozsa_app::tools::write::WriteTool;
use rozsa_core::tool::Tool;
use serde_json::json;

#[test]
fn file_delta_distinguishes_added_and_modified_files() {
    let added = build_file_delta("new.rs", None, Some("one\ntwo\n".to_string())).unwrap();
    assert_eq!(added.status, FileDeltaStatus::Added);
    assert_eq!((added.added, added.deleted), (2, 0));

    let modified = build_file_delta(
        "lib.rs",
        Some("old\n".to_string()),
        Some("new\n".to_string()),
    )
    .unwrap();
    assert_eq!(modified.status, FileDeltaStatus::Modified);
    assert!(modified.patch.contains("-old\n+new"));
}

#[tokio::test]
async fn write_and_bash_emit_serializable_file_deltas() {
    let workspace = tempfile::tempdir().unwrap();
    let file = workspace.path().join("written.md");
    let write = WriteTool::new()
        .execute(
            "write-1",
            json!({"file_path": file, "content": "hello\n"}),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(write.details["file_deltas"][0]["status"], "added");

    let bash = BashTool::new(workspace.path().to_string_lossy().to_string())
        .execute(
            "bash-1",
            json!({"command": "printf 'from bash\\n' > generated.txt"}),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(bash.details["file_deltas"][0]["path"], "generated.txt");
    assert_eq!(bash.details["capture_complete"], true);
}
