use std::fs;
use std::process::Command;

use rozsa_gui::git_diff::{read_workspace_diff, workspace_diff_stat};

#[test]
fn unborn_repository_renders_untracked_file_against_dev_null() {
    let workspace = tempfile::tempdir().unwrap();
    assert!(Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(workspace.path())
        .status()
        .unwrap()
        .success());
    fs::write(workspace.path().join("poem.md"), "first\nsecond\n").unwrap();

    let diff = read_workspace_diff(workspace.path(), "poem.md").unwrap();
    assert!(diff.patch.contains("--- /dev/null"));
    assert!(diff.patch.contains("+++ b/poem.md"));
    assert!(diff.patch.contains("+first"));
    assert_eq!(workspace_diff_stat(workspace.path()), (2, 0));
}

#[test]
fn workspace_diff_rejects_paths_outside_the_workspace() {
    let workspace = tempfile::tempdir().unwrap();
    assert_eq!(
        read_workspace_diff(workspace.path(), "../secret.txt").unwrap_err(),
        "Diff path must stay within the workspace"
    );
}
