#[test]
fn stale_autocomplete_cannot_replace_dom_during_composition() {
    let source = include_str!("../frontend/app.js");
    let start = source.find("async function updateAutocomplete()").unwrap();
    let end = source[start..]
        .find("function acHighlight")
        .map(|offset| start + offset)
        .unwrap();
    let autocomplete = &source[start..end];

    assert!(autocomplete.contains("if (isInputComposing || seq !== acRequestSeq) return;"));

    let highlight_start = source.find("function updateInputHighlight(ranges)").unwrap();
    let highlight_end = source[highlight_start..]
        .find("function syncInputHighlightScroll")
        .map(|offset| highlight_start + offset)
        .unwrap();
    let highlight = &source[highlight_start..highlight_end];
    assert!(highlight.contains("if (!input || isInputComposing) return;"));
}

#[test]
fn composition_start_invalidates_pending_autocomplete() {
    let source = include_str!("../frontend/app.js");
    let start = source.find("function handleCompositionStart").unwrap();
    let end = source[start..]
        .find("function handleCompositionUpdate")
        .map(|offset| start + offset)
        .unwrap();
    let handler = &source[start..end];

    assert!(handler.contains("acRequestSeq++"));
    assert!(handler.contains("hideAutocomplete(false)"));
}
