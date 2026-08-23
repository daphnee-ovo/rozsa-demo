// FrameworkTree
// prototype_scene_fixture_test.rs
// ├── running_state_markup_keeps_baseline_ownership()
// ├── scene_adapter_prefers_dev_flow_sessions()
// ├── running_fixture_covers_baseline_scene_states()
// └── repository_root()

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

#[test]
fn running_state_markup_keeps_baseline_ownership() {
    let root = repository_root();
    let sidebar = fs::read_to_string(root.join("prototype/templates/features/sidebar.html"))
        .expect("sidebar template must be readable");
    let composer = fs::read_to_string(root.join("prototype/templates/composites/composer.html"))
        .expect("composer template must be readable");

    assert_eq!(sidebar.matches("id=\"subagentPanel\"").count(), 1);
    assert_eq!(composer.matches("id=\"subagentPanel\"").count(), 0);
    for id in ["forkPicker", "queuedMessages", "steeringConversation"] {
        assert_eq!(
            composer.matches(&format!("id=\"{id}\"")).count(),
            1,
            "{id} 必须只位于 Composer template"
        );
    }
}

#[test]
fn scene_adapter_prefers_dev_flow_sessions() {
    let root = repository_root();
    let adapter = fs::read_to_string(root.join("prototype/scripts/adapters/mock-adapter.js"))
        .expect("mock adapter must be readable");

    assert!(
        adapter.contains("const sessions = runtime?.sessions || session?.sessions;"),
        "Dev Flow scene 必须优先使用 runtime session fixture"
    );
}

#[test]
fn running_fixture_covers_baseline_scene_states() {
    let root = repository_root();
    let states: Value = serde_json::from_str(
        &fs::read_to_string(root.join("prototype/mock/tools/call-states.json"))
            .expect("tool state mock must be readable"),
    )
    .expect("tool state mock must be valid JSON");

    for (scene, panels) in [
        (
            "complete-session",
            vec![
                "subagentPanel",
                "forkPicker",
                "queuedMessages",
                "steeringConversation",
            ],
        ),
        (
            "dev-flow-runtime",
            vec!["queuedMessages", "steeringConversation"],
        ),
    ] {
        for panel in panels {
            assert!(
                states["running"][scene][panel].is_object(),
                "{scene} 缺少 baseline running fixture: {panel}"
            );
        }
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}
