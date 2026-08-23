// FrameworkTree
// prototype_root_cutover_test.rs
// ├── root_prototype_is_complete_and_unique()
// ├── active_references_use_root_source()
// └── repository_root()

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn root_prototype_is_complete_and_unique() {
    let root = repository_root();
    let prototype = root.join("prototype");

    for relative in [
        "index.html",
        "gallery/visual-direction.html",
        "gallery/visual-direction.css",
        "gallery/visual-direction.js",
        "scenarios/registry.js",
        "scripts/core/shell.js",
        "scripts/adapters/mock-adapter.js",
        "templates/features/sidebar.html",
        "styles/main.css",
        "styles/entries/main.css",
        "mock/sessions/empty.json",
        "docs/ARCHITECTURE.md",
        "docs/COMPONENTS.md",
        "docs/SCENARIOS.md",
        "docs/FLOWS.md",
        "docs/VALIDATION.md",
        "docs/MOCK_DATA.md",
        "docs/UI_COVERAGE.md",
        "docs/UI_COVERAGE.json",
        "docs/DESIGN_GAPS.md",
    ] {
        assert!(
            prototype.join(relative).is_file(),
            "root prototype 缺少迁移文件：{relative}"
        );
    }

    let retired = [
        ["docs", "gui", "prototype"].join("/"),
        ["docs", "gui", "new-version"].join("/"),
        ["docs", "gui", "UI_COVERAGE.md"].join("/"),
        ["docs", "gui", "UI_COVERAGE.json"].join("/"),
        ["docs", "gui", "DESIGN_GAPS.md"].join("/"),
        ["prototype", "rozsa-gui.html"].join("/"),
        ["prototype", "rozsa-visual-demo.html"].join("/"),
    ];
    for retired in retired {
        assert!(
            !root.join(&retired).exists(),
            "旧原型或兼容入口仍存在：{retired}"
        );
    }
}

#[test]
fn active_references_use_root_source() {
    let root = repository_root();
    let active_files = [
        "AGENTS.md",
        "docs/MIGRATION_RESIDUE_AUDIT.md",
        "docs/gui/FRONTEND_INTERFACE_STANDARD.md",
        "docs/gui/UI_USAGE_GUIDELINES.md",
        "docs/gui/ARCHITECTURE.md",
        "crates/rozsa-gui/tests/prototype_coverage_inventory_test.rs",
        "crates/rozsa-gui/tests/prototype_stylesheet_extraction_test.rs",
        "crates/rozsa-gui/tests/native_window_behavior.rs",
        "prototype/index.html",
        "prototype/styles/source-order.json",
        "prototype/docs/UI_COVERAGE.md",
        "prototype/docs/UI_COVERAGE.json",
        "prototype/docs/DESIGN_GAPS.md",
        "prototype/docs/ARCHITECTURE.md",
    ];
    let legacy_candidate = ["docs", "gui", "new-version"].join("/");
    let legacy_prototype = ["docs", "gui", "prototype"].join("/");
    let old_gallery = "rozsa-visual-".to_owned() + "demo.html";

    for relative in active_files {
        let source = fs::read_to_string(root.join(relative))
            .unwrap_or_else(|error| panic!("无法读取有效引用文件 {relative}: {error}"));
        assert!(
            !source.contains(&legacy_candidate),
            "{relative} 仍引用已删除的新版原型路径"
        );
        assert!(
            !source.contains(&legacy_prototype),
            "{relative} 仍引用已删除的旧原型路径"
        );
        assert!(
            !source.contains(&old_gallery),
            "{relative} 仍引用已删除的独立 visual demo 入口"
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
