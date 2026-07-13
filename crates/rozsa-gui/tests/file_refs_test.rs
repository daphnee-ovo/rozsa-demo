use std::fs;

use rozsa_gui::file_refs::{complete_file_reference, expand_file_references, find_file_mentions};
use rozsa_model::types::ContentBlock;

#[test]
fn parses_plain_and_quoted_file_mentions() {
    let mentions = find_file_mentions(r#"read @src/main.rs and @"my file.md""#);
    assert_eq!(mentions.len(), 2);
    assert_eq!(mentions[0].path, "src/main.rs");
    assert_eq!(mentions[1].path, "my file.md");
}

#[test]
fn expands_text_file_with_line_numbers_and_display_text() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("main.rs"),
        "fn main() {}\nprintln!(\"x\");\n",
    )
    .unwrap();

    let expansion = expand_file_references("check @main.rs", dir.path());

    assert_eq!(expansion.display_text.as_deref(), Some("check @main.rs"));
    assert_eq!(expansion.blocks.len(), 1);
    let ContentBlock::Text { text, .. } = &expansion.blocks[0] else {
        panic!("expected text block");
    };
    assert!(text.contains("<path>"));
    assert!(text.contains("main.rs"));
    assert!(text.contains("     1\tfn main() {}"));
    assert!(text.contains("     2\tprintln!"));
}

#[test]
fn completes_at_file_references() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("Cargo.toml"), "").unwrap();

    let items = complete_file_reference("@C", dir.path());

    assert!(items.iter().any(|item| item.value == "@Cargo.toml "));
    assert!(items.iter().all(|item| item.value.starts_with('@')));
}

#[test]
fn blocks_external_and_secret_mentions_without_blocking_workspace_files() {
    let workspace = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("source.rs"), "fn source() {}\n").unwrap();
    fs::write(workspace.path().join(".env"), "TOKEN=secret\n").unwrap();
    let external_path = external.path().join("outside.rs");
    fs::write(&external_path, "fn outside() {}\n").unwrap();

    let expansion = expand_file_references(
        &format!("use @source.rs @.env @{}", external_path.to_string_lossy()),
        workspace.path(),
    );

    assert_eq!(expansion.blocks.len(), 1);
    assert_eq!(expansion.notices.len(), 2);
    assert!(
        expansion
            .notices
            .iter()
            .any(|notice| notice.reason.contains("secret-like"))
    );
    assert!(
        expansion
            .notices
            .iter()
            .any(|notice| notice.reason.contains("workspace"))
    );
}
