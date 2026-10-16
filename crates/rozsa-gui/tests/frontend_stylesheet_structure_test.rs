// FrameworkTree
// frontend_stylesheet_structure_test.rs
// ├── runtime_html_uses_external_stylesheet_entries()
// ├── stylesheet_entries_share_foundations_and_keep_feature_boundaries()
// ├── every_local_css_import_exists_and_import_graph_is_acyclic()
// ├── shared_style_owners_are_reachable_from_both_hosts()
// └── visit_imports()

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const MAIN_HTML: &str = include_str!("../../../desktop/renderer/tauri.html");
const SIDEBAR_HTML: &str = include_str!("../../../desktop/renderer/sidebar.html");
const MAIN_CSS: &str = include_str!("../../../desktop/renderer/styles/legacy/main.css");
const SIDEBAR_CSS: &str = include_str!("../../../desktop/renderer/styles/legacy/sidebar.css");

#[test]
fn runtime_html_uses_external_stylesheet_entries() {
    assert!(MAIN_HTML.contains("href=\"styles/legacy/main.css\""));
    assert!(SIDEBAR_HTML.contains("href=\"styles/legacy/sidebar.css\""));

    for html in [MAIN_HTML, SIDEBAR_HTML] {
        assert!(!html.contains("<style"), "runtime CSS must not be embedded");
        assert!(
            !html.contains(" style="),
            "static inline styles are forbidden"
        );
    }
}

#[test]
fn stylesheet_entries_share_foundations_and_keep_feature_boundaries() {
    for shared in ["tokens.css", "reset.css"] {
        let import = format!("url(\"./{shared}\")");
        assert!(MAIN_CSS.contains(&import));
        assert!(SIDEBAR_CSS.contains(&import));
    }

    let frontend = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../desktop/renderer");
    for entry in ["main", "sidebar"] {
        let mut owners = HashSet::new();
        visit_imports(
            &frontend.join(format!("styles/legacy/{entry}.css")),
            &mut HashSet::new(),
            &mut owners,
        );
        for layer in ["foundations", "layouts", "composites", "features"] {
            let shared_layer = frontend.join("styles").join(layer).canonicalize().unwrap();
            assert!(
                owners.iter().any(|owner| owner.starts_with(&shared_layer)),
                "{entry} cannot reach shared {layer} styles"
            );
        }
    }
}

#[test]
fn every_local_css_import_exists_and_import_graph_is_acyclic() {
    let styles = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../desktop/renderer/styles/legacy");
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();

    visit_imports(&styles.join("main.css"), &mut visiting, &mut visited);
    visit_imports(&styles.join("sidebar.css"), &mut visiting, &mut visited);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    visit_imports(
        &root.join("prototype/styles/main.css"),
        &mut visiting,
        &mut visited,
    );
    visit_imports(
        &root.join("prototype/styles/sidebar.css"),
        &mut visiting,
        &mut visited,
    );
}

#[test]
fn shared_style_owners_are_reachable_from_both_hosts() {
    let frontend = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../desktop/renderer");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(frontend.join("styles/source-order.json")).unwrap(),
    )
    .unwrap();
    let mut runtime_owners = HashSet::new();
    let mut prototype_owners = HashSet::new();
    visit_imports(
        &frontend.join("styles/legacy/main.css"),
        &mut HashSet::new(),
        &mut runtime_owners,
    );
    visit_imports(
        &root.join("prototype/styles/main.css"),
        &mut HashSet::new(),
        &mut prototype_owners,
    );
    for owner in manifest["owners"].as_array().unwrap() {
        let file = owner["file"].as_str().unwrap();
        let shared = frontend.join("styles").join(file).canonicalize().unwrap();
        let forward = fs::read_to_string(root.join("prototype/styles").join(file)).unwrap();
        let imports: Vec<_> = forward.lines().collect();
        assert!(imports.contains(
            &format!("@import url(\"../../../desktop/renderer/styles/{file}\");").as_str()
        ));
        for import in imports {
            let target = import
                .strip_prefix("@import url(\"../../../desktop/renderer/styles/")
                .and_then(|line| line.strip_suffix("\");"))
                .expect("prototype shim must contain only shared imports");
            assert!(
                manifest["owners"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|owner| owner["file"].as_str() == Some(target)),
                "shim references an unregistered owner: {target}"
            );
        }
        if file != "primitives/forms.css" {
            assert!(
                runtime_owners.contains(&shared),
                "runtime cannot reach {file}"
            );
            assert!(
                prototype_owners.contains(&shared),
                "prototype cannot reach {file}"
            );
        }
        let css = fs::read_to_string(&shared).unwrap();
        assert!(
            !css.contains("@import"),
            "shared owner imports another implementation: {file}"
        );
    }
    for entry in ["main", "sidebar"] {
        let css = fs::read_to_string(frontend.join(format!("styles/entries/{entry}.css"))).unwrap();
        assert!(
            !css.contains("prototype"),
            "production entry depends on prototype"
        );
    }
}

fn visit_imports(path: &Path, visiting: &mut HashSet<PathBuf>, visited: &mut HashSet<PathBuf>) {
    let canonical = path.canonicalize().unwrap_or_else(|error| {
        panic!("missing stylesheet {}: {error}", path.display());
    });
    if visited.contains(&canonical) {
        return;
    }
    assert!(
        visiting.insert(canonical.clone()),
        "cyclic CSS import at {}",
        canonical.display()
    );

    let css = fs::read_to_string(&canonical).unwrap();
    for line in css
        .lines()
        .filter(|line| line.trim_start().starts_with("@import"))
    {
        let relative = line
            .split("url(\"")
            .nth(1)
            .and_then(|tail| tail.split("\")").next())
            .unwrap_or_else(|| panic!("unsupported CSS import syntax: {line}"));
        let imported = canonical.parent().unwrap().join(relative);
        visit_imports(&imported, visiting, visited);
    }

    visiting.remove(&canonical);
    visited.insert(canonical);
}
