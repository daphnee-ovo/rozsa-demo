#[test]
fn permission_panel_matches_the_requested_action_layout() {
    let html = include_str!("../frontend/index.html");

    assert!(html.contains("class=\"perm-panel-tool\" id=\"permTool\""));
    assert!(html.contains("class=\"perm-panel-desc\" id=\"permDesc\""));
    assert!(html.contains("id=\"permCmdToggle\""));
    assert!(html.contains("perm-panel-opt-key\">Y</span>"));
    assert!(html.contains("Allow Once"));
    assert!(html.contains("perm-panel-opt-key\">T</span>"));
    assert!(html.contains("Trust in session"));
    assert!(html.contains("perm-panel-opt-key\">N</span>"));
    assert!(html.contains("Deny Execute"));
    assert!(html.contains("perm-panel-opt-key\">H</span>"));
    assert!(html.contains("Deny and hints"));
    assert!(!html.contains("perm-panel-risk"));
}

#[test]
fn permission_rendering_uses_description_and_bash_prompt() {
    let source = include_str!("../frontend/app.js");

    assert!(source.contains("desc.textContent = ev.description || '';"));
    assert!(source.contains(r#"perm-syn-prompt">$ </span>"#));
    assert!(source.contains("e.key === 'y' || e.key === 'Y'"));
    assert!(source.contains("e.key === 'n' || e.key === 'N'"));
    assert!(source.contains("cmd.scrollHeight > cmd.clientHeight + 1"));
}
