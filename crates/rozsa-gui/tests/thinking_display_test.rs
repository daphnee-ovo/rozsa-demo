// FrameworkTree
// thinking_display_test.rs
// ├── thinking_rendering_preserves_expand_state_and_patches_streaming_dom()
// ├── thinking_animation_does_not_restart_on_each_token()
// ├── thinking_block_uses_the_shared_prototype_card_and_disclosure()
// ├── hide_thinking_setting_hides_expanded_and_collapsed_shared_cards()
// └── thinking_effort_picker_uses_model_support_and_compacts_missing_levels()

#[test]
fn thinking_rendering_preserves_expand_state_and_patches_streaming_dom() {
    let source = include_str!("../../../desktop/renderer/app.js");
    assert!(source.contains("patchStreamingThinking"));
    assert!(source.contains("expandedThinkingBySession"));
    assert!(source.contains("aria-expanded"));
    assert!(source.contains("thinking-markdown"));
}

#[test]
fn thinking_animation_does_not_restart_on_each_token() {
    let source = include_str!("../../../desktop/renderer/styles/features/thinking.css");
    assert!(source.contains(".thinking-block.active .thinking-icon"));
    assert!(!source.contains("animation: thinkPulse"));
    assert!(!source.contains("animation: thinkDots"));
}

#[test]
fn thinking_block_uses_the_shared_prototype_card_and_disclosure() {
    let source = include_str!("../../../desktop/renderer/styles/features/thinking.css");
    let block = source
        .split(".thinking-block {")
        .nth(1)
        .and_then(|tail| tail.split('}').next())
        .unwrap();
    assert!(block.contains("border-radius: var(--radius-card)"));
    assert!(block.contains("background: color-mix"));
    assert!(source.contains(".thinking-block:not(.expanded) .thinking-content"));
    assert!(source.contains(".thinking-block.expanded .thinking-content { display: block; }"));
    assert!(!source.contains(".thinking-header:hover"));
}

#[test]
fn hide_thinking_setting_hides_expanded_and_collapsed_shared_cards() {
    let app = include_str!("../../../desktop/renderer/app.js");
    let css = include_str!("../../../desktop/renderer/styles/features/thinking.css");
    assert!(app.contains("classList.toggle('hide-thinking', Boolean(settings.hide_thinking))"));
    assert!(app.contains("classList.toggle('hide-thinking', enabled)"));
    let hidden_rule = css
        .split("body.hide-thinking .thinking-block {")
        .nth(1)
        .and_then(|tail| tail.split('}').next())
        .expect(
            "shared thinking cards must honor the hide-thinking setting in either expand state",
        );
    assert!(hidden_rule.contains("display: none;"));
}

#[test]
fn thinking_effort_picker_uses_model_support_and_compacts_missing_levels() {
    let source = include_str!("../../../desktop/renderer/app.js");

    assert!(source.contains("option.value === 'off' || unavailable[option.value] !== null"));
    assert!(source.contains("function normalizeThinkingEffort(options, requested)"));
    let component = include_str!("../../../desktop/renderer/scripts/components/popover.js");
    assert!(source.contains("window.RozsaPopover.renderChoice(options, selectedIndex)"));
    assert!(component.contains("slider.max = String(options.length - 1)"));
    assert!(component.contains("slider.disabled = options.length === 1"));
    assert!(source.contains("const selected = normalizeThinkingEffort(options, requested)"));
    assert!(source.contains("document.getElementById('modelSelector')?.textContent?.trim()"));
}
