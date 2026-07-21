#[test]
fn thinking_rendering_preserves_expand_state_and_patches_streaming_dom() {
    let source = include_str!("../frontend/app.js");
    assert!(source.contains("patchStreamingThinking"));
    assert!(source.contains("expandedThinkingBySession"));
    assert!(source.contains("aria-expanded"));
    assert!(source.contains("thinking-markdown"));
}

#[test]
fn thinking_animation_does_not_restart_on_each_token() {
    let source = include_str!("../frontend/index.html");
    assert!(source.contains(".thinking-block.active .thinking-icon"));
    assert!(!source.contains("animation: thinkPulse"));
    assert!(!source.contains("animation: thinkDots"));
}

#[test]
fn thinking_block_uses_typography_without_card_chrome() {
    let source = include_str!("../frontend/index.html");
    let block = source
        .split(".thinking-block {")
        .nth(1)
        .and_then(|tail| tail.split('}').next())
        .unwrap();
    let content = source
        .split(".thinking-content {")
        .nth(1)
        .and_then(|tail| tail.split('}').next())
        .unwrap();

    assert!(block.contains("border: 0"));
    assert!(block.contains("background: transparent"));
    assert!(content.contains("border: 0"));
    assert!(!source.contains(".thinking-header:hover"));
}
