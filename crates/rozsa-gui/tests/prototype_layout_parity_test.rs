// FrameworkTree
// prototype_layout_parity_test.rs
// ├── application_mounts_preserve_grid_item_semantics()
// ├── scene_stylesheets_are_declared_and_loaded_by_root_shell()
// ├── complete_session_keeps_running_panel_style_contract()
// ├── composer_mount_preserves_baseline_direct_children()
// └── repository_root()

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn application_mounts_preserve_grid_item_semantics() {
    let root = repository_root();
    let app_shell = fs::read_to_string(root.join("prototype/styles/layouts/app-shell.css"))
        .expect("application shell stylesheet must be readable");

    assert!(app_shell.contains(
        "[data-od-id=\"app-body\"] > [data-template-mount=\"sidebar\"],\n[data-od-id=\"app-body\"] > [data-template-mount=\"conversation\"] { display: contents; }"
    ));
    assert!(app_shell.contains("[data-application-shell]"));
    assert!(app_shell.contains("flex: 1 1 0"));
    assert!(app_shell.contains("min-height: 0"));
    assert!(app_shell.contains("grid-template-columns: var(--main-sidebar-width) minmax(0, 1fr)"));
    assert!(app_shell.contains("overflow: hidden"));
}

#[test]
fn scene_stylesheets_are_declared_and_loaded_by_root_shell() {
    let root = repository_root();
    let registry = fs::read_to_string(root.join("prototype/scenarios/registry.js"))
        .expect("scene registry must be readable");
    let shell = fs::read_to_string(root.join("prototype/scripts/core/shell.js"))
        .expect("prototype shell must be readable");

    for (scene, stylesheet) in [
        ("complete-session", "styles/scenes/complete-session.css"),
        ("ask-user-question", "styles/scenes/ask-user-question.css"),
        ("dev-flow-runtime", "styles/scenes/dev-flow-runtime.css"),
    ] {
        assert!(registry.contains(&format!("id: \"{scene}\"")));
        assert!(registry.contains(&format!("stylesheet: \"{stylesheet}\"")));
        assert!(root.join(format!("prototype/{stylesheet}")).is_file());
    }

    assert!(shell.contains("await loadSceneStylesheet(sceneId);"));
    assert!(shell.contains("dataset.prototypeSceneStylesheet"));
}

#[test]
fn complete_session_keeps_running_panel_style_contract() {
    let root = repository_root();
    let scene_styles =
        fs::read_to_string(root.join("prototype/styles/scenes/complete-session.css"))
            .expect("complete-session stylesheet must be readable");
    let input = fs::read_to_string(root.join("prototype/templates/primitives/input.html"))
        .expect("input template must be readable");
    let composer = fs::read_to_string(root.join("prototype/templates/composites/composer.html"))
        .expect("composer template must be readable");

    for selector in [
        ".subagent-sidebar-panel",
        ".subagent-sidebar-panel ol",
        ".subagent-sidebar-panel li::before",
        ".input-wrapper > .running-messages:not([hidden]) ~ .rich-input",
    ] {
        assert!(
            scene_styles.contains(selector),
            "missing scene style contract: {selector}"
        );
    }
    assert!(input.contains("id=\"msgInput\""));
    assert!(composer.contains("class=\"input-wrapper\""));
    assert!(composer.contains("id=\"queuedMessages\""));
    assert!(composer.contains("id=\"steeringConversation\""));
    assert!(composer.contains("data-template-mount=\"input\""));
}

#[test]
fn composer_mount_preserves_baseline_direct_children() {
    let root = repository_root();
    let shell = fs::read_to_string(root.join("prototype/scripts/core/shell.js"))
        .expect("prototype shell must be readable");
    let renderer = fs::read_to_string(root.join("prototype/scripts/core/renderer.js"))
        .expect("prototype renderer must be readable");

    assert!(shell.contains("mountTemplateChildren(inputFragment"));
    assert!(renderer.contains("export function mountTemplateChildren"));
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}
