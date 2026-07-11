#[test]
fn frontend_keeps_prototype_write_edit_and_turn_diff_contracts() {
    let app = include_str!("../frontend/app.js");
    let html = include_str!("../frontend/index.html");

    assert!(app.contains("function renderCodeView(content)"));
    assert!(app.contains("tc.arguments.content"));
    assert!(app.contains("let expandedToolCallsBySession = {}"));
    assert!(app.contains("data-tool-call-id"));
    assert!(app.contains("if (snap.streamUpdate)"));
    assert!(app.contains("function updateThinkingTimings("));
    assert!(!app.contains("Date.now() - messageTimestampMs(msg)"));
    assert!(app.contains("renderedQueueKey"));
    assert!(html.contains(".perm-panel-opt:focus"));
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
    assert!(html.contains(".code-view"));
    assert!(html.contains(".diff-del"));
    assert!(html.contains(".diff-add"));
    assert!(html.contains(".turn-diff-inline"));
    assert!(!html.contains("id=\"turnDiff\""));
}
