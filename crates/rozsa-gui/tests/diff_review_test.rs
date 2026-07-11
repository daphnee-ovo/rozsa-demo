use std::path::Path;

use rozsa_gui::read_workspace_diff;

#[test]
fn workspace_diff_rejects_paths_outside_the_project() {
    let error = read_workspace_diff(Path::new("."), "../secret.txt").unwrap_err();
    assert_eq!(error, "Diff path must stay within the workspace");
}
