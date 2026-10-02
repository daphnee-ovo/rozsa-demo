// FrameworkTree
// prototype_stylesheet_extraction_test.rs
// ├── component_files_have_explicit_owners()
// ├── scene_artifacts_do_not_carry_runtime_css()
// ├── stylesheet_entries_are_external_acyclic_and_runtime_shaped()
// ├── imported_owners()
// └── repository_root()

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const SOURCE_ORDER: &str = include_str!("../../../prototype/styles/source-order.json");

#[test]
fn component_files_have_explicit_owners() {
    let root = repository_root();
    let manifest: Value = serde_json::from_str(SOURCE_ORDER).unwrap();
    let blocks = manifest["blocks"].as_array().unwrap();

    for block in blocks {
        let target = block["target"].as_str().unwrap();
        let content = fs::read_to_string(root.join(target)).unwrap();
        assert!(
            !content.contains("@import") || block["owners"].is_array(),
            "owner CSS 不得形成隐藏 import 图：{target}"
        );
        if let Some(owners) = block["owners"].as_array() {
            let imported = imported_owners(&root.join(target));
            for owner in owners {
                let owner = owner.as_str().unwrap();
                assert!(imported.contains(&root.join(owner).canonicalize().unwrap()));
                assert!(
                    !fs::read_to_string(root.join(owner))
                        .unwrap()
                        .contains("@import")
                );
            }
        }
    }

    let main_entry = fs::read_to_string(root.join("prototype/styles/entries/main.css")).unwrap();
    for import in [
        "@import url(\"../composites/composer.css\");",
        "@import url(\"../features/queue.css\");",
        "@import url(\"../features/steering.css\");",
        "@import url(\"../features/dev-flow-runtime.css\");",
    ] {
        assert!(
            main_entry.contains(import),
            "main entry 缺少公共 owner: {import}"
        );
    }
    assert!(!main_entry.contains("../primitives/forms.css"));
    assert!(!root.join("prototype/rozsa-gui.css").exists());

    let responsive_layout = &manifest["responsive_layout"];
    assert_eq!(
        responsive_layout["sidebar_auto_collapse_max_width"].as_u64(),
        Some(780)
    );
    assert_eq!(
        responsive_layout["minimum_window"]["width"].as_u64(),
        Some(640)
    );
    assert_eq!(
        responsive_layout["minimum_window"]["height"].as_u64(),
        Some(480)
    );
}

#[test]
fn scene_artifacts_do_not_carry_runtime_css() {
    let root = repository_root();
    let manifest: Value = serde_json::from_str(SOURCE_ORDER).unwrap();
    assert!(
        manifest["extracted_inline_styles"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    for scene in ["ask-user-question", "complete-session", "dev-flow-runtime"] {
        let css =
            fs::read_to_string(root.join(format!("prototype/styles/scenes/{scene}.css"))).unwrap();
        assert!(
            !css.contains("data-rozsa-scene"),
            "scene CSS 仍包含 scene selector: {scene}"
        );
        let html = fs::read_to_string(root.join(format!("prototype/scenes/{scene}.html"))).unwrap();
        assert!(
            !html.contains("styles/scenes/"),
            "scene artifact 仍加载 scene CSS: {scene}"
        );
    }
}

#[test]
fn stylesheet_entries_are_external_acyclic_and_runtime_shaped() {
    let root = repository_root();
    let main = fs::read_to_string(root.join("prototype/styles/main.css")).unwrap();
    let sidebar = fs::read_to_string(root.join("prototype/styles/sidebar.css")).unwrap();
    assert_eq!(main.trim(), "@import url(\"./entries/main.css\");");
    assert_eq!(sidebar.trim(), "@import url(\"./entries/sidebar.css\");");

    let main_entry = fs::read_to_string(root.join("prototype/styles/entries/main.css")).unwrap();
    let sidebar_entry =
        fs::read_to_string(root.join("prototype/styles/entries/sidebar.css")).unwrap();
    assert!(!main_entry.contains("../entries/main.css"));
    assert!(!main_entry.contains("../entries/sidebar.css"));
    assert!(!sidebar_entry.contains("../entries/main.css"));
    assert!(!sidebar_entry.contains("../entries/sidebar.css"));
    assert!(!main_entry.contains("utilities.css"));
    assert!(!sidebar_entry.contains("utilities.css"));

    let manifest: Value = serde_json::from_str(SOURCE_ORDER).unwrap();
    let blocks = manifest["blocks"].as_array().unwrap();
    let mut expected_sidebar_owners = Vec::new();
    // Shared main foundations include Settings/Appearance/Notification owners before
    // prototype sidebar supplements.
    for order in [1, 2, 3, 4, 5, 11, 12, 16, 6, 18] {
        let block = blocks
            .iter()
            .find(|block| block["order"].as_u64() == Some(order))
            .unwrap();
        let target = block["target"].as_str().unwrap();
        expected_sidebar_owners.push(root.join(target).canonicalize().unwrap());
    }
    expected_sidebar_owners.push(
        root.join("crates/rozsa-gui/frontend/ui/styles/foundations/motion.css")
            .canonicalize()
            .unwrap(),
    );
    assert_eq!(
        imported_owners(&root.join("prototype/styles/entries/sidebar.css")),
        expected_sidebar_owners
    );

    for html in [
        "prototype/scenes/ask-user-question.html",
        "prototype/scenes/complete-session.html",
        "prototype/scenes/dev-flow-runtime.html",
        "prototype/gallery/visual-direction.html",
    ] {
        let source = fs::read_to_string(root.join(html)).unwrap();
        assert!(!source.contains("<style>"), "{html} 仍包含 style 标签");
    }

    let gallery_css =
        fs::read_to_string(root.join("prototype/gallery/visual-direction.css")).unwrap();
    assert_eq!(
        gallery_css.trim(),
        "@import url(\"../styles/features/visual-demo.css\");"
    );
    let gallery_html =
        fs::read_to_string(root.join("prototype/gallery/visual-direction.html")).unwrap();
    assert!(gallery_html.contains("href=\"visual-direction.css\""));
    assert!(gallery_html.contains("src=\"visual-direction.js\""));

    for scene in fs::read_dir(root.join("prototype/scenes")).unwrap() {
        let path = scene.unwrap().path();
        if path.extension().and_then(|value| value.to_str()) != Some("html") {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        assert!(source.contains("href=\"../styles/main.css\""));
        assert!(!source.contains("href=\"../rozsa-gui.css\""));
    }
}

fn imported_owners(path: &Path) -> Vec<PathBuf> {
    let source = fs::read_to_string(path).unwrap();
    let imports: Vec<_> = source
        .lines()
        .filter(|line| line.trim_start().starts_with("@import"))
        .map(|line| {
            line.split("url(\"")
                .nth(1)
                .unwrap()
                .split("\")")
                .next()
                .unwrap()
        })
        .collect();
    if imports.is_empty() {
        return vec![path.canonicalize().unwrap()];
    }
    imports
        .into_iter()
        .flat_map(|relative| imported_owners(&path.parent().unwrap().join(relative)))
        .collect()
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}
