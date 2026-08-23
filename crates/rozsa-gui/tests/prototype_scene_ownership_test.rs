// FrameworkTree
// prototype_scene_ownership_test.rs
// ├── application_scenes_are_configuration_only()
// ├── shared_state_rules_live_with_component_owners()
// └── repository_root()

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn application_scenes_are_configuration_only() {
    let root = repository_root();
    let registry = fs::read_to_string(root.join("prototype/scenarios/registry.js")).unwrap();
    let shell = fs::read_to_string(root.join("prototype/scripts/core/shell.js")).unwrap();

    for scene in ["complete-session", "ask-user-question", "dev-flow-runtime"] {
        let start = registry
            .find(&format!("\"{scene}\": {{"))
            .unwrap_or_else(|| panic!("registry 缺少 application scene: {scene}"));
        let end = registry[start..]
            .find("\n  },")
            .map(|offset| start + offset)
            .unwrap_or(registry.len());
        let definition = &registry[start..end];
        assert!(
            !definition.contains("stylesheet"),
            "application scene 不得声明 stylesheet: {scene}"
        );
    }

    assert!(!shell.contains("loadSceneStylesheet"));
    assert!(!shell.contains("prototypeSceneStylesheet"));
    assert!(!shell.contains("getSceneDefinition"));

    for scene in ["complete-session", "ask-user-question", "dev-flow-runtime"] {
        let html = fs::read_to_string(root.join(format!("prototype/scenes/{scene}.html"))).unwrap();
        assert!(
            !html.contains("styles/scenes/"),
            "scene artifact 不得加载 application scene CSS: {scene}"
        );
    }
}

#[test]
fn shared_state_rules_live_with_component_owners() {
    let root = repository_root();
    let entries = fs::read_to_string(root.join("prototype/styles/entries/main.css")).unwrap();
    let composer =
        fs::read_to_string(root.join("prototype/styles/composites/composer.css")).unwrap();
    let queue = fs::read_to_string(root.join("prototype/styles/features/queue.css")).unwrap();
    let steering = fs::read_to_string(root.join("prototype/styles/features/steering.css")).unwrap();
    let sidebar = fs::read_to_string(root.join("prototype/styles/features/sidebar.css")).unwrap();
    let dev_flow =
        fs::read_to_string(root.join("prototype/styles/features/dev-flow-runtime.css")).unwrap();
    let overlays =
        fs::read_to_string(root.join("prototype/styles/composites/overlays.css")).unwrap();

    assert!(entries.contains("../composites/composer.css"));
    assert!(entries.contains("../features/queue.css"));
    assert!(entries.contains("../features/steering.css"));
    assert!(entries.contains("../features/dev-flow-runtime.css"));
    assert!(!entries.contains("../primitives/forms.css"));

    assert!(composer.contains(".input-wrapper > .running-messages:not([hidden]) ~ .rich-input"));
    assert!(composer.contains("[data-od-id=\"chat-input\"] { position: absolute;"));
    assert!(queue.contains("#queuedMessages"));
    assert!(steering.contains("#steeringConversation"));
    assert!(sidebar.contains(".subagent-sidebar-panel"));
    assert!(dev_flow.contains(".dev-flow-detail"));
    assert!(overlays.contains(":has(> .question-panel.visible)"));

    for path in [
        "prototype/styles/scenes/complete-session.css",
        "prototype/styles/scenes/ask-user-question.css",
        "prototype/styles/scenes/dev-flow-runtime.css",
    ] {
        let scene_css = fs::read_to_string(root.join(path)).unwrap();
        assert!(
            !scene_css.contains("data-rozsa-scene"),
            "scene CSS 仍包含 scene selector: {path}"
        );
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}
