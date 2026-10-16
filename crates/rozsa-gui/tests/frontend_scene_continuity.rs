#[test]
fn main_scene_state_is_captured_without_rebuilding_stateful_roots() {
    let js = include_str!("../frontend/app.js");
    let html = include_str!("../frontend/index.html");

    assert!(html.contains("id=\"mainContentScene\" data-native-scene-root=\"main-content\""));
    let start = js.find("function captureMainSceneContinuity").unwrap();
    let end = js[start..]
        .find("function restoreMainSceneContinuity")
        .map(|offset| start + offset)
        .unwrap();
    let capture = &js[start..end];
    assert!(capture.contains("captureSessionDraft(activeSessionId)"));
    assert!(capture.contains("persistSessionViewState(activeSessionId, chat)"));
    assert!(capture.contains("capturePermissionUiState(activeSessionId)"));
    assert!(capture.contains("focusOwner: mainRoot?.contains(document.activeElement)"));
    assert!(capture.contains("inputSelection: getInputSelection(input)"));
    assert!(!capture.contains("innerHTML"));
    assert!(!capture.contains("replaceChildren"));

    for state in [
        "sessionDraftState",
        "sessionViewState",
        "expandedToolCallsBySession",
        "expandedThinkingBySession",
        "permissionUiStateBySession",
    ] {
        assert!(js.contains(state), "missing continuity state: {state}");
    }
}

#[test]
fn ime_defers_the_latest_complete_scene_until_composition_end() {
    let js = include_str!("../frontend/app.js");

    let apply_start = js.find("function applyGuiSceneSnapshot").unwrap();
    let apply_end = js[apply_start..]
        .find("function applyMainThemeState")
        .map(|offset| apply_start + offset)
        .unwrap();
    let apply = &js[apply_start..apply_end];
    assert!(apply.contains("isInputComposing && snapshot?.scene !== guiSceneState.scene"));
    assert!(apply.contains("snapshot.revision > pendingGuiSceneSnapshot.revision"));

    let composition_start = js.find("function handleCompositionEnd").unwrap();
    let composition_end = js[composition_start..]
        .find("function setInputText")
        .map(|offset| composition_start + offset)
        .unwrap();
    assert!(js[composition_start..composition_end].contains("flushPendingGuiSceneTransition()"));
    assert!(js.contains("if (snapshot) applyGuiSceneSnapshot(snapshot)"));
}

#[test]
fn returning_to_main_restores_focus_or_uses_the_composer_fallback() {
    let js = include_str!("../frontend/app.js");
    let start = js.find("function restoreMainSceneContinuity").unwrap();
    let end = js[start..]
        .find("function flushPendingGuiSceneTransition")
        .map(|offset| start + offset)
        .unwrap();
    let restore = &js[start..end];

    assert!(restore.contains("focusOwner?.isConnected"));
    assert!(restore.contains("focusOwner.focus({ preventScroll: true })"));
    assert!(restore.contains("setInputSelection(input, memory.inputSelection.start"));
    assert!(restore.contains("input.focus({ preventScroll: true })"));
    assert!(restore.contains("fallbackOffset"));
    assert!(js.contains("requestAnimationFrame(restoreMainSceneContinuity)"));
}
