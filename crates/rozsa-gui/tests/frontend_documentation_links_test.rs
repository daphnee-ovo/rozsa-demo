use std::{fs, path::Path};

const DOCUMENTS: &[&str] = &[
    "CONTRIBUTING.md",
    "docs/gui/ARCHITECTURE.md",
    "docs/gui/FRONTEND_INTERFACE_STANDARD.md",
    "docs/gui/FRONTEND_COMPONENTS.md",
    "docs/gui/FRONTEND_TERMINOLOGY.md",
    "docs/gui/TERMINOLOGY.md",
    "docs/gui/UI_USAGE_GUIDELINES.md",
    "docs/gui/DEV_FLOW_INTEGRATION.md",
    "docs/gui/NATIVE_SPLIT_VALIDATION.md",
    "docs/gui/themes.md",
    "crates/rozsa-gui/frontend/styles/README.md",
    "crates/rozsa-gui/frontend/ui/styles/README.md",
    "prototype/styles/README.md",
    "prototype/docs/ARCHITECTURE.md",
    "prototype/docs/COMPONENTS.md",
    "prototype/docs/SCENARIOS.md",
    "prototype/docs/FLOWS.md",
    "prototype/docs/MOCK_DATA.md",
    "prototype/docs/VALIDATION.md",
    "prototype/docs/UI_COVERAGE.md",
    "prototype/docs/DESIGN_GAPS.md",
];

fn missing_links(source: &str, directory: &Path) -> Vec<String> {
    source
        .split("](")
        .skip(1)
        .filter_map(|part| part.split_once(')').map(|(target, _)| target))
        .map(|target| target.trim().trim_matches(['<', '>']))
        .filter(|target| !target.contains("://") && !target.starts_with("mailto:"))
        .map(|target| target.split(['#', '?']).next().unwrap())
        .filter(|target| !target.is_empty() && !directory.join(target).exists())
        .map(str::to_owned)
        .collect()
}

#[test]
fn shared_frontend_documentation_has_resolvable_local_links() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut failures = Vec::new();
    for document in DOCUMENTS {
        let path = root.join(document);
        let source = fs::read_to_string(&path).unwrap();
        for target in missing_links(&source, path.parent().unwrap()) {
            failures.push(format!("{document}: {target}"));
        }
    }
    assert!(
        failures.is_empty(),
        "Broken documentation links:\n{}",
        failures.join("\n")
    );
}

#[test]
fn link_checker_rejects_missing_files_without_rejecting_anchors_or_urls() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let links = "[ok](Cargo.toml#package) [anchor](#section) [url](https://example.com) [bad](missing-frontend-document.md)";
    assert_eq!(missing_links(links, root), ["missing-frontend-document.md"]);
}
