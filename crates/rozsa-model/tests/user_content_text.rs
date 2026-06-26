use rozsa_model::types::{ContentBlock, UserContent};

#[test]
fn text_variant_returns_inner_string() {
    let c = UserContent::Text("hello".to_string());
    assert_eq!(c.text(), "hello");
}

#[test]
fn blocks_with_single_text_returns_text() {
    let c = UserContent::Blocks(vec![ContentBlock::Text {
        text: "hi".to_string(),
        signature: None,
    }]);
    assert_eq!(c.text(), "hi");
}

#[test]
fn blocks_skips_non_text_blocks() {
    let c = UserContent::Blocks(vec![
        ContentBlock::Text {
            text: "a".to_string(),
            signature: None,
        },
        ContentBlock::Image {
            data: "base64data".to_string(),
            mime_type: "image/png".to_string(),
        },
    ]);
    assert_eq!(c.text(), "a");
}

#[test]
fn blocks_joins_multiple_text_with_newline() {
    let c = UserContent::Blocks(vec![
        ContentBlock::Text {
            text: "first".to_string(),
            signature: None,
        },
        ContentBlock::Text {
            text: "second".to_string(),
            signature: None,
        },
    ]);
    assert_eq!(c.text(), "first\nsecond");
}

#[test]
fn empty_blocks_returns_empty_string() {
    let c = UserContent::Blocks(vec![]);
    assert_eq!(c.text(), "");
}
