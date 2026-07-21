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
fn macos_main_webview_uses_native_pane_width_and_the_existing_sidebar_webview_overlay() {
    let html = include_str!("../frontend/index.html");
    let js = include_str!("../frontend/app.js");
    let split = include_str!("../src/native_split_view.rs");

    assert!(html.contains("body.native-split-main [data-od-id=\"app-body\"]"));
    assert!(html.contains("display: block;"));
    assert!(html.contains("body.native-split-main [data-od-id=\"main-panel\"]"));
    assert!(js.contains("document.body.classList.toggle('native-split-main', nativeSplitMode)"));
    assert!(js.contains("if (!nativeSplitMode) {\n    syncMainSidebarViewport();"));
    assert!(js.contains("addEventListener('pointerenter', showNativeSidebarOverlay)"));
    assert!(js.contains("invoke('set_native_sidebar_overlay_visible', { visible: true })"));
    assert!(js.contains("await listen('native-sidebar-state'"));
    assert!(split.contains("self.sidebar_overlay.addSubview(&self.sidebar_view)"));
}
