// FrameworkTree
// ime_runtime_test.rs
// ├── stale_autocomplete_cannot_replace_dom_during_composition()
// ├── composition_start_invalidates_pending_autocomplete()
// └── hiding_autocomplete_removes_visible_suggestions()

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

    let highlight_start = source
        .find("function updateInputHighlight(ranges)")
        .unwrap();
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

#[test]
fn hiding_autocomplete_removes_visible_suggestions() {
    let source = include_str!("../frontend/app.js");
    let start = source.find("function hideAutocomplete(").unwrap();
    let end = source[start..]
        .find("function isTransientPopupVisible")
        .unwrap()
        + start;
    let script = format!(
        "const assert=require('node:assert/strict');let acVisible=true,acSelectedIndex=0,acPrefix='/',acItems=[1];const popup={{classList:{{remove(){{}}}},children:[1],replaceChildren(){{this.children=[];}}}};const document={{getElementById(){{return popup;}}}};{};hideAutocomplete(false);assert.equal(popup.children.length,0,'shared popup displays nonempty suggestions');assert.equal(acVisible,false);",
        &source[start..end]
    );
    let output = std::process::Command::new("node")
        .args(["-e", &script])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
