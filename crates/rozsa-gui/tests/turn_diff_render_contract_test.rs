// FrameworkTree
// turn_diff_render_contract_test.rs
// └── frontend_keeps_prototype_write_edit_and_turn_diff_contracts()

#[test]
fn frontend_keeps_prototype_write_edit_and_turn_diff_contracts() {
    let app = include_str!("../../../desktop/renderer/app.js");
    let html = include_str!("../../../desktop/renderer/tauri.html");
    let overlay_css =
        include_str!("../../../desktop/renderer/styles/legacy/components/overlays.css");
    let tool_css = include_str!("../../../desktop/renderer/styles/legacy/features/tools.css");
    let form_css = include_str!("../../../desktop/renderer/styles/legacy/components/forms.css");

    assert!(app.contains("function renderCodeView(content)"));
    assert!(app.contains("typeof args.content === 'string' ? args.content : null"));
    assert!(app.contains("let expandedToolCallsBySession = {}"));
    assert!(app.contains("data-tool-call-id"));
    assert!(app.contains("if (snap.streamUpdate)"));
    assert!(app.contains("function updateThinkingTimings("));
    assert!(!app.contains("Date.now() - messageTimestampMs(msg)"));
    assert!(app.contains("renderedQueueKey"));
    assert!(overlay_css.contains(".perm-panel-opt:focus"));
    assert!(app.contains("let sessionViewState = {}"));
    assert!(app.contains("function persistSessionViewState("));
    assert!(app.contains("function restoreSessionViewState("));
    assert!(!app.contains("rozsa.sessionView.v1"));
    assert!(!app.contains("rozsa.toolExpansion.v1"));
    assert!(app.contains("let sessionDraftState = {}"));
    assert!(app.contains("let permissionUiStateBySession = {}"));
    assert!(app.contains("function captureSessionDraft("));
    assert!(app.contains("function restoreSessionDraft("));
    assert!(app.contains("function capturePermissionUiState("));
    assert!(app.contains("function renderDiffView(patch)"));
    assert!(app.contains("function toggleTurnDiff(button)"));
    assert!(!app.contains("function closeTurnDiff(button)"));
    assert!(app.contains("summary.assistantMessageIndex"));
    assert!(tool_css.contains(".code-view"));
    assert!(tool_css.contains(".diff-del"));
    assert!(tool_css.contains(".diff-add"));
    assert!(form_css.contains(".turn-diff-inline"));
    assert!(!html.contains("id=\"turnDiff\""));
}
