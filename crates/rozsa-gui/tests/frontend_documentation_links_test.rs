use std::{fs, path::Path};

const DOCUMENTS: &[&str] = &[
    "crates/rozsa-gui/frontend/styles/README.md",
    "crates/rozsa-gui/frontend/ui/styles/README.md",
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
