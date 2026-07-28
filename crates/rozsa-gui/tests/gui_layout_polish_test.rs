#[test]
fn sidebar_and_settings_keep_compact_titlebar_spacing() {
    let sidebar = include_str!("../frontend/sidebar.html");
    let main = include_str!("../frontend/index.html");

    assert!(sidebar
        .contains(".sidebar-scene { display: flex; flex-direction: column; padding-top: 24px; }"));
    assert!(sidebar.contains("padding: 8px 14px 6px"));
    assert!(main.contains("--native-titlebar-offset: 32px"));
    assert!(main.contains("padding: 0 clamp(26px, 7vw, 108px) 96px"));
}

#[test]
fn composer_controls_follow_the_requested_order_and_hint_behavior() {
    let html = include_str!("../frontend/index.html");
    let js = include_str!("../frontend/app.js");
    let input = html.split("id=\"msgInput\"").nth(1).unwrap();
    let toolbar = html.split("<div class=\"input-toolbar\">").nth(1).unwrap();

    let slash = toolbar.find("Slash commands").unwrap();
    let context = toolbar.find("data-od-id=\"context-info\"").unwrap();
    let spacer = toolbar.find("class=\"input-spacer\"").unwrap();
    let model = toolbar.find("id=\"modelSelector\"").unwrap();
    assert!(slash < context && context < spacer && spacer < model);
    assert!(!toolbar.contains("composerHint"));
    assert!(input.contains("data-placeholder=\"Message Rózsa, supports Markdown…\""));
    assert!(!input.contains("data-default-placeholder"));

    assert!(html.contains("border-radius: 8px;\n  background: transparent;"));
    assert!(html.contains("border: 0;\n  appearance: none;"));
    assert!(html.contains("flex: 0 1 auto;"));
    assert!(!html.contains("flex: 0 1 clamp(110px, 22vw, 220px)"));
    assert!(js.contains("const COMPOSER_HINTS = ["));
    assert!(js.contains("const COMPOSER_HINT_ROTATION_MS = 30_000"));
    assert!(js.contains("window.setInterval(rotateComposerHint, COMPOSER_HINT_ROTATION_MS)"));
    assert!(
        js.contains("input.addEventListener('pointerdown', dismissComposerHints, { once: true })")
    );
    assert!(js.contains("if (input) input.dataset.placeholder = ''"));
    assert!(js.contains("updateContextUsage(snap.contextUsage)"));
}

#[test]
fn native_main_content_stays_centered_while_resizing() {
    let html = include_str!("../frontend/index.html");
    let split = include_str!("../src/native_split_view.rs");

    assert!(html.contains("--main-content-max-width: 960px"));
    assert!(html.contains("body.native-split-main {\n  width: 100%;\n}"));
    assert!(html.contains(
        "body.native-split-main [data-od-id=\"main-panel\"] {\n  width: 100%;\n  height: 100%;\n  align-items: center;\n}"
    ));
    assert!(html
        .contains("body.native-split-main [data-od-id=\"panel-header\"] { align-self: stretch; }"));
    assert!(html.contains(
        "[data-od-id=\"chat-messages\"],\n[data-od-id=\"chat-input\"] {\n  width: 100%;\n  max-width: var(--main-content-max-width);\n}"
    ));

    let install = split
        .split("fn install_native_split(")
        .nth(1)
        .unwrap()
        .split("fn close_sidebar_async(")
        .next()
        .unwrap();
    assert!(install.contains("let main_constraints = pin_to_parent(&main_view, &main_pane);"));
    assert!(!install.contains("main_view.setFrame("));
}

#[test]
fn native_sidebar_button_reflects_real_collapsed_state() {
    let split = include_str!("../src/native_split_view.rs");
    let titlebar = include_str!("../src/native_titlebar.rs");

    assert!(split.contains("pub fn toggle_sidebar() -> Result<bool, String>"));
    assert!(split.contains("Ok(host.sidebar_item.isCollapsed())"));
    assert!(split.contains("pub fn is_sidebar_collapsed() -> Result<bool, String>"));
    assert!(titlebar.contains("ns_string!(\"sidebar.left\")"));
    assert!(titlebar.contains("ns_string!(\"sidebar.right\")"));
    assert!(titlebar.contains("configurationWithScale(NSImageSymbolScale::Medium)"));
    assert!(titlebar.contains("image.setSize(NSSize::new(14.0, 14.0))"));
    assert!(titlebar.contains("button.setImageScaling(NSImageScaling::ScaleNone)"));
    assert!(titlebar.contains("let initial_sidebar_collapsed = on_sidebar_collapsed()"));
    assert!(titlebar.contains("drag_view.update_sidebar_symbol(collapsed)"));
}

#[test]
fn native_collapsed_sidebar_reveals_at_the_edge_and_avoids_the_titlebar() {
    let html = include_str!("../frontend/index.html");
    let frontend = include_str!("../frontend/app.js");
    let sidebar_frontend = include_str!("../frontend/sidebar.js");
    let lib = include_str!("../src/lib.rs");
    let split = include_str!("../src/native_split_view.rs");

    assert!(frontend.contains("await invoke('native_sidebar_collapsed')"));
    assert!(frontend.contains("await listen('native-sidebar-state'"));
    assert!(frontend.contains("addEventListener('pointerenter', showNativeSidebarOverlay)"));
    assert!(frontend.contains("invoke('set_native_sidebar_overlay_visible', { visible: true })"));
    assert!(frontend.contains("let nativeSidebarOverlayWidth = 0"));
    assert!(frontend.contains("let nativeSidebarOverlayRevealInFlight = false"));
    assert!(frontend.contains(
        "nativeSidebarOverlayRevealInFlight = false;\n        nativeSidebarOverlayVisible = false;"
    ));
    assert!(frontend.contains("invoke('native_sidebar_overlay_width')"));
    assert!(frontend.contains("nativeSidebarOverlayVisible || nativeSidebarOverlayRevealInFlight"));
    assert!(frontend.contains("event.clientX > nativeSidebarOverlayWidth"));
    assert!(frontend.contains("event.clientX <= nativeSidebarEdgeTriggerWidth()"));
    assert!(frontend.contains("showNativeSidebarOverlay(event)"));
    assert!(!frontend.contains("event.clientX > NATIVE_SIDEBAR_EDGE_TRIGGER_WIDTH"));
    assert!(frontend.contains("invoke('set_native_sidebar_overlay_visible', { visible: false })"));
    assert!(frontend.contains("window.addEventListener('pointerdown', handleSidebarEdgeReveal)"));
    assert!(frontend.contains(
        "document.documentElement.addEventListener('pointerenter', handleSidebarEdgeReveal)"
    ));
    assert!(!sidebar_frontend.contains("pointerleave"));
    assert!(!sidebar_frontend.contains("set_native_sidebar_overlay_visible"));
    assert!(html.contains("id=\"nativeSidebarEdgeTrigger\""));
    assert!(html.contains("id=\"nativeGuiInteractionLayer\" aria-hidden=\"true\""));
    assert!(html.contains("#nativeGuiInteractionLayer { display: none; }"));
    assert!(html.contains("z-index: 120;"));
    assert!(html.contains("pointer-events: none;"));
    assert!(html.contains("pointer-events: auto;"));
    assert_eq!(html.matches("id=\"nativeSidebarEdgeTrigger\"").count(), 1);
    let settings_panel = html
        .find("id=\"settingsPanel\"")
        .expect("settings scene root is missing");
    let interaction_layer = html
        .find("id=\"nativeGuiInteractionLayer\"")
        .expect("shared native interaction layer is missing");
    assert!(interaction_layer > settings_panel);
    assert!(html.contains("width: 18px;"));
    assert!(html.contains("body.native-split-main.sidebar-collapsed #nativeSidebarEdgeTrigger"));
    assert!(html.contains("body.native-split-main.sidebar-collapsed:not(.native-fullscreen) [data-od-id=\"panel-header\"]"));
    assert!(html.contains("height: 58px;\n  flex: 0 0 58px;\n  padding-top: 12px;"));
    assert!(lib.contains("fn native_sidebar_collapsed() -> Result<bool, String>"));
    assert!(lib.contains("fn set_native_sidebar_overlay_visible(visible: bool)"));
    assert!(lib.contains("fn native_sidebar_overlay_width() -> Result<f64, String>"));
    assert!(lib.contains("emit(\"native-sidebar-state\", collapsed)"));
    assert!(split.contains("pub fn set_sidebar_overlay_visible(visible: bool)"));
    assert!(split.contains("pub fn sidebar_overlay_width() -> Result<f64, String>"));
    assert!(split.contains("sidebar_overlay_width_constraint.constant()"));
    assert!(split.contains("self.sidebar_overlay.addSubview(&self.sidebar_view)"));
    assert!(split.contains("self.sidebar_pane.addSubview(&self.sidebar_view)"));
    assert!(split.contains("host.sidebar_pane.frame().size.width"));
    assert!(split.contains("sidebar_overlay_width_constraint"));
    assert!(split
        .contains(".topAnchor()\n            .constraintEqualToAnchor(&split_root.topAnchor())"));
}
