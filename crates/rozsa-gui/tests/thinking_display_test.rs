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
