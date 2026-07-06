use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// Helper to create a temporary test file
fn create_test_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, content).expect("Failed to write test file");
    path
}

#[tokio::test]
async fn test_exact_match_still_works() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_path = create_test_file(
        &temp_dir,
        "exact.txt",
        "Hello world\nThis is a test\nGoodbye world",
    );

    let old_string = "This is a test";
    let new_string = "This is modified";

    // Import the edit_file function (assuming it's made pub(crate) or we use the Tool trait)
    // For now, we'll test through the file system
    use rozsa_app::tools::create_edit_tool;
    use rozsa_core::tool::Tool;
    use serde_json::json;

    let tool = create_edit_tool();
    let params = json!({
        "file_path": file_path.to_str().unwrap(),
        "old_string": old_string,
        "new_string": new_string,
    });

    let result = tool
        .execute("test_id", params, None, None)
        .await
        .expect("Edit failed");

    // Read the file back
    let new_content = fs::read_to_string(&file_path).expect("Failed to read file");
    assert!(new_content.contains("This is modified"));
    assert!(!new_content.contains("This is a test"));
    assert!(new_content.contains("Hello world"));

    // Check that result indicates exact match (no strategy message)
    let text = match &result.content[0] {
        rozsa_model::types::ContentBlock::Text { text, .. } => text,
        _ => panic!("Expected text block"),
    };
    assert!(!text.contains("using whitespace-normalized"));
}

#[tokio::test]
async fn test_crlf_file_preserves_line_endings() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let content_with_crlf = "Hello world\r\nThis is a test\r\nGoodbye world";
    let file_path = create_test_file(&temp_dir, "crlf.txt", content_with_crlf);

    let old_string = "This is a test";
    let new_string = "This is modified";

    use rozsa_app::tools::create_edit_tool;
    use rozsa_core::tool::Tool;
    use serde_json::json;

    let tool = create_edit_tool();
    let params = json!({
        "file_path": file_path.to_str().unwrap(),
        "old_string": old_string,
        "new_string": new_string,
    });

    tool.execute("test_id", params, None, None)
        .await
        .expect("Edit failed");

    // Read the file back as bytes to check line endings
    let new_content_bytes = fs::read(&file_path).expect("Failed to read file");
    let new_content = String::from_utf8(new_content_bytes).expect("Invalid UTF-8");

    // Check that CRLF is preserved
    assert!(
        new_content.contains("\r\n"),
        "CRLF line endings were not preserved"
    );
    assert!(new_content.contains("This is modified"));
    assert!(!new_content.contains("This is a test"));
}

#[tokio::test]
async fn test_whitespace_normalized_match_works() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    // File has extra spaces
    let content = "Hello world\nThis    is     a   test\nGoodbye world";
    let file_path = create_test_file(&temp_dir, "whitespace.txt", content);

    // old_string with normalized whitespace
    let old_string = "This is a test";
    let new_string = "This is modified";

    use rozsa_app::tools::create_edit_tool;
    use rozsa_core::tool::Tool;
    use serde_json::json;

    let tool = create_edit_tool();
    let params = json!({
        "file_path": file_path.to_str().unwrap(),
        "old_string": old_string,
        "new_string": new_string,
    });

    let result = tool
        .execute("test_id", params, None, None)
        .await
        .expect("Edit failed");

    // Read the file back
    let new_content = fs::read_to_string(&file_path).expect("Failed to read file");
    assert!(new_content.contains("This is modified"));
    assert!(!new_content.contains("This    is     a   test"));

    // Check that result indicates whitespace-normalized match
    let text = match &result.content[0] {
        rozsa_model::types::ContentBlock::Text { text, .. } => text,
        _ => panic!("Expected text block"),
    };
    assert!(text.contains("using whitespace-normalized match"));
}

#[tokio::test]
async fn test_bom_is_preserved() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    // Content with BOM
    let content_with_bom = "\u{feff}Hello world\nThis is a test\nGoodbye world";
    let file_path = create_test_file(&temp_dir, "bom.txt", content_with_bom);

    let old_string = "This is a test";
    let new_string = "This is modified";

    use rozsa_app::tools::create_edit_tool;
    use rozsa_core::tool::Tool;
    use serde_json::json;

    let tool = create_edit_tool();
    let params = json!({
        "file_path": file_path.to_str().unwrap(),
        "old_string": old_string,
        "new_string": new_string,
    });

    tool.execute("test_id", params, None, None)
        .await
        .expect("Edit failed");

    // Read the file back
    let new_content = fs::read_to_string(&file_path).expect("Failed to read file");

    // Check that BOM is preserved
    assert!(new_content.starts_with('\u{feff}'), "BOM was not preserved");
    assert!(new_content.contains("This is modified"));
    assert!(!new_content.contains("This is a test"));
}

#[tokio::test]
async fn test_exact_match_fails_with_whitespace_difference() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let content = "Hello world\nThis is a test\nGoodbye world";
    let file_path = create_test_file(&temp_dir, "no_fuzzy.txt", content);

    // old_string with different whitespace - should fail with exact match
    // but we test that fuzzy match can handle it
    let old_string = "This  is  a  test"; // double spaces
    let new_string = "This is modified";

    use rozsa_app::tools::create_edit_tool;
    use rozsa_core::tool::Tool;
    use serde_json::json;

    let tool = create_edit_tool();
    let params = json!({
        "file_path": file_path.to_str().unwrap(),
        "old_string": old_string,
        "new_string": new_string,
    });

    // This should succeed via whitespace-normalized matching
    let result = tool.execute("test_id", params, None, None).await;

    assert!(result.is_ok(), "Fuzzy match should have succeeded");
}
