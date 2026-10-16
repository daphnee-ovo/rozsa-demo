// FrameworkTree
// prototype_scene_fixture_test.rs
// ├── running_state_markup_keeps_baseline_ownership()
// └── repository_root()

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn running_state_markup_keeps_baseline_ownership() {
    let root = repository_root();
    let sidebar = fs::read_to_string(
        root.join("crates/rozsa-gui/frontend/ui/templates/features/sidebar.html"),
    )
    .expect("sidebar template must be readable");
    let composer = fs::read_to_string(
        root.join("crates/rozsa-gui/frontend/ui/templates/composites/composer.html"),
    )
    .expect("composer template must be readable");
    let input = fs::read_to_string(
        root.join("crates/rozsa-gui/frontend/ui/templates/primitives/input.html"),
    )
    .expect("input template must be readable");

    assert_eq!(sidebar.matches("id=\"subagentPanel\"").count(), 1);
    assert_eq!(composer.matches("id=\"subagentPanel\"").count(), 0);
    for id in ["forkPicker", "queuedMessages", "steeringConversation"] {
        assert_eq!(
            composer.matches(&format!("id=\"{id}\"")).count(),
            1,
            "{id} 必须只位于 Composer template"
        );
    }

    let wrapper_start = composer
        .find("class=\"input-wrapper\"")
        .expect("Composer must own the input frame");
    let fork_start = composer
        .find("id=\"forkPicker\"")
        .expect("Composer must own the fork picker");
    let queue_start = composer
        .find("id=\"queuedMessages\"")
        .expect("Composer must own the queue panel");
    let steering_start = composer
        .find("id=\"steeringConversation\"")
        .expect("Composer must own the steering panel");
    let input_mount = composer
        .find("data-template-mount=\"input\"")
        .expect("Composer must mount the input primitive");
    assert!(fork_start < wrapper_start);
    assert!(wrapper_start < queue_start);
    assert!(queue_start < steering_start);
    assert!(steering_start < input_mount);
    assert!(input.contains("id=\"msgInput\""));
    assert!(!input.contains("class=\"input-wrapper\""));
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}
