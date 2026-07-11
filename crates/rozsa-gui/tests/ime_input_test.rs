#[test]
fn input_declares_ime_lifecycle_handlers() {
    let html = include_str!("../frontend/index.html");

    assert!(html.contains("oninput=\"handleInput(this)\""));
    assert!(html.contains("oncompositionstart=\"handleCompositionStart(event)\""));
    assert!(html.contains("oncompositionupdate=\"handleCompositionUpdate(this)\""));
    assert!(html.contains("oncompositionend=\"handleCompositionEnd(this)\""));
}

#[test]
fn composition_guards_input_refresh_and_keyboard_shortcuts() {
    let source = include_str!("../frontend/app.js");
    let handle_input_start = source.find("function handleInput(input)").unwrap();
    let handle_input_end = source[handle_input_start..]
        .find("function handleCompositionStart")
        .map(|offset| handle_input_start + offset)
        .unwrap();
    let handle_input = &source[handle_input_start..handle_input_end];

    assert!(handle_input.contains("if (isInputComposing)"));
    assert!(handle_input.contains("updateAutocomplete();"));

    let keydown_start = source.find("document.addEventListener('keydown'").unwrap();
    let keydown = &source[keydown_start..];
    assert!(keydown.contains("if (isInputComposing || e.isComposing || e.keyCode === 229) return;"));
}

#[test]
fn composition_end_refreshes_autocomplete_after_commit() {
    let source = include_str!("../frontend/app.js");
    let start = source.find("function handleCompositionEnd(input)").unwrap();
    let end = source[start..]
        .find("function setInputText")
        .map(|offset| start + offset)
        .unwrap();
    let handler = &source[start..end];

    assert!(handler.contains("isInputComposing = false;"));
    assert!(handler.contains("updateAutocomplete();"));
    assert!(handler.contains("updateAbortButton();"));
}
