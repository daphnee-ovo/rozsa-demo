#[test]
fn deny_and_hints_opens_a_prefixed_editable_input() {
    let html = include_str!("../frontend/index.html");
    let source = include_str!("../frontend/app.js");

    assert!(html.contains("id=\"permHintInput\""));
    assert!(html.contains("value=\"Deny, \""));
    assert!(html.contains("oninput=\"normalizePermissionHint(this)\""));
    assert!(html.contains("onkeydown=\"handlePermissionHintKeydown(event)\""));
    assert!(html.contains("onclick=\"submitPermissionHint()\""));
    assert!(source.contains("const PERMISSION_HINT_PREFIX = 'Deny, ';"));
    assert!(source.contains("input.value = PERMISSION_HINT_PREFIX + suffix;"));
    assert!(source.contains("void respondPermission('deny-hint', hint);"));
}

#[test]
fn deny_and_hints_supports_h_and_tab_confirmation() {
    let source = include_str!("../frontend/app.js");

    assert!(source.contains(
        "if (e.key === 'h' || e.key === 'H') { e.preventDefault(); enterPermissionHint(); return; }"
    ));
    assert!(source.contains("if (e.key === 'Tab')"));
    assert!(source.contains("if (selectedKey === 'H')"));
    assert!(source.contains("function submitPermissionHint()"));
}

#[test]
fn custom_hint_reaches_the_permission_response() {
    let source = include_str!("../frontend/app.js");
    let commands = include_str!("../src/commands.rs");

    assert!(source.contains("hint: choice === 'deny-hint' ? hint || null : null"));
    assert!(commands.contains("hint: Option<String>"));
    assert!(commands.contains(".filter(|hint| !hint.trim().is_empty())"));
}
