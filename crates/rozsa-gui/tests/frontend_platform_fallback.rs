#[test]
fn non_macos_materializes_the_existing_css_sidebar_and_settings_navigation() {
    let html = include_str!("../frontend/index.html");
    let js = include_str!("../frontend/app.js");

    assert!(html.contains("id=\"fallbackSidebarTemplate\""));
    assert!(html.contains("id=\"fallbackSettingsNavigationTemplate\""));
    assert!(html.contains("grid-template-columns: var(--main-sidebar-width) minmax(0, 1fr)"));
    assert!(html.contains("grid-template-columns: var(--settings-sidebar-width) minmax(0, 1fr)"));
    assert!(js.contains("if (nativeSplitMode) return;"));
    assert!(js.contains("materializeFallbackTemplate('fallbackSidebarTemplate'"));
    assert!(js.contains("materializeFallbackTemplate('fallbackSettingsNavigationTemplate'"));
}

#[test]
fn macos_main_webview_does_not_use_css_grid_for_native_pane_width() {
    let html = include_str!("../frontend/index.html");
    let js = include_str!("../frontend/app.js");

    assert!(html.contains("body.native-split-main [data-od-id=\"app-body\"]"));
    assert!(html.contains("display: block;"));
    assert!(html.contains("body.native-split-main [data-od-id=\"main-panel\"]"));
    assert!(js.contains("document.body.classList.toggle('native-split-main', nativeSplitMode)"));
    assert!(js.contains("if (!nativeSplitMode) {\n    syncMainSidebarViewport();"));
    assert!(js.contains("if (!nativeSplitMode) updateSidebar(snap)"));
    assert!(js.contains("if (!nativeSplitMode) renderSessionList()"));
}
