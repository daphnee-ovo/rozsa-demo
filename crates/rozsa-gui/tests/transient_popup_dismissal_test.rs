#[test]
fn transient_popups_share_outside_click_and_escape_dismissal() {
    let source = include_str!("../frontend/app.js");

    for id in [
        "autocomplete",
        "forkPicker",
        "subagentPanel",
        "quotaTooltip",
    ] {
        assert!(
            source.contains(&format!("'{}'", id)),
            "missing transient popup: {id}"
        );
    }
    assert!(source.contains("function dismissTransientPopupsOutside(target)"));
    assert!(
        source.contains("if (!isTransientPopupVisible(popup) || popup.contains(target)) continue;")
    );
    assert!(source.contains("document.addEventListener('pointerdown'"));
    assert!(source.contains("dismissTransientPopupsOutside(e.target);"));

    let keydown_start = source.find("document.addEventListener('keydown'").unwrap();
    let keydown = &source[keydown_start..];
    let ime_guard = keydown.find("if (isInputComposing").unwrap();
    let double_escape = keydown.find("if (isDoubleEscape)").unwrap();
    let dismiss = keydown.find("if (dismissTransientPopups())").unwrap();
    let settings = keydown.find("settingsPanel").unwrap();
    let streaming = keydown.find("if (isStreaming)").unwrap();
    assert!(ime_guard < double_escape);
    assert!(double_escape < dismiss);
    assert!(dismiss < settings);
    assert!(settings < streaming);
    assert!(source.contains("const DOUBLE_ESCAPE_WINDOW_MS = 1000"));
    assert!(source.contains("lastStreamingEscapeAt = isDoubleEscape ? 0 : now"));
}

#[test]
fn transient_popup_dismissal_preserves_permission_handling() {
    let source = include_str!("../frontend/app.js");
    let keydown_start = source.find("document.addEventListener('keydown'").unwrap();
    let keydown = &source[keydown_start..];

    let permission = keydown.find("if (currentPermissionId)").unwrap();
    let dismiss = keydown.find("if (dismissTransientPopups())").unwrap();
    assert!(permission < dismiss);
    assert!(keydown.contains("respondPermission('deny')"));
}
