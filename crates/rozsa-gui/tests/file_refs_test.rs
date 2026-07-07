use std::fs;

use rozsa_gui::file_refs::{
    complete_file_reference, expand_file_references, find_file_mentions,
};
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
    fs::write(dir.path().join("main.rs"), "fn main() {}\nprintln!(\"x\");\n").unwrap();

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
