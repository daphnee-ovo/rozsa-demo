// FrameworkTree
// prototype_layout_parity_test.rs
// ├── application_mounts_preserve_grid_item_semantics()
// ├── application_scenes_use_public_styles_and_static_shell()
// ├── complete_session_keeps_public_running_panel_style_contract()
// ├── composer_mount_preserves_baseline_direct_children()
// └── repository_root()

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn application_mounts_preserve_grid_item_semantics() {
    let root = repository_root();
    let app_shell =
        fs::read_to_string(root.join("crates/rozsa-gui/frontend/ui/styles/layouts/app-shell.css"))
            .expect("application shell stylesheet must be readable");
    let sidebar_shell = fs::read_to_string(
        root.join("crates/rozsa-gui/frontend/ui/styles/layouts/sidebar-shell.css"),
    )
    .expect("sidebar shell stylesheet must be readable");

    assert!(app_shell.contains(
        "[data-od-id=\"app-body\"] > [data-template-mount=\"sidebar\"],\n[data-od-id=\"app-body\"] > [data-template-mount=\"conversation\"] { display: contents; }"
    ));
    assert!(app_shell.contains("[data-application-shell]"));
    assert!(app_shell.contains("flex: 1 1 0"));
    assert!(app_shell.contains("min-height: 0"));
    assert!(app_shell.contains("grid-template-columns: var(--main-sidebar-width) minmax(0, 1fr)"));
    assert!(app_shell.contains("overflow: hidden"));
    assert!(app_shell.contains("[data-od-id=\"app-body\"] { display: block; }"));
    assert!(sidebar_shell.contains("@media (max-width: 780px)"));
    assert!(sidebar_shell.contains("position: absolute"));
    assert!(
        sidebar_shell
            .contains("[data-od-id=\"app-body\"].sidebar-edge-visible [data-od-id=\"sidebar\"]")
    );
}

#[test]
fn application_scenes_use_public_styles_and_static_shell() {
    let root = repository_root();
    let registry = fs::read_to_string(root.join("prototype/scenarios/registry.js"))
        .expect("scene registry must be readable");
    let shell = fs::read_to_string(root.join("prototype/scripts/core/shell.js"))
        .expect("prototype shell must be readable");

    for scene in ["complete-session", "ask-user-question", "dev-flow-runtime"] {
        let start = registry
            .find(&format!("id: \"{scene}\""))
            .unwrap_or_else(|| panic!("registry 缺少 application scene: {scene}"));
        let end = registry[start..]
            .find("\n  },")
            .map(|offset| start + offset)
            .unwrap_or(registry.len());
        assert!(!registry[start..end].contains("stylesheet:"));
    }

    assert!(!shell.contains("loadSceneStylesheet"));
    assert!(!shell.contains("prototypeSceneStylesheet"));
}

#[test]
fn complete_session_keeps_public_running_panel_style_contract() {
    let root = repository_root();
    let sidebar_styles =
        fs::read_to_string(root.join("crates/rozsa-gui/frontend/ui/styles/features/sidebar.css"))
            .expect("sidebar stylesheet must be readable");
    let composer_styles = fs::read_to_string(
        root.join("crates/rozsa-gui/frontend/ui/styles/composites/composer.css"),
    )
    .expect("composer stylesheet must be readable");
    let input = fs::read_to_string(
        root.join("crates/rozsa-gui/frontend/ui/templates/primitives/input.html"),
    )
    .expect("input template must be readable");
    let composer = fs::read_to_string(
        root.join("crates/rozsa-gui/frontend/ui/templates/composites/composer.html"),
    )
    .expect("composer template must be readable");

    for selector in [
        ".subagent-sidebar-panel",
        ".subagent-sidebar-panel ol",
        ".subagent-sidebar-panel li::before",
    ] {
        assert!(
            sidebar_styles.contains(selector),
            "missing sidebar style contract: {selector}"
        );
    }
    assert!(
        composer_styles.contains(".input-wrapper > .running-messages:not([hidden]) ~ .rich-input")
    );
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
    let renderer =
        fs::read_to_string(root.join("crates/rozsa-gui/frontend/ui/scripts/core/renderer.js"))
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
