// FrameworkTree
// gui_layout_polish_test.rs
// ├── sidebar_and_settings_keep_compact_titlebar_spacing()
// ├── composer_controls_follow_the_requested_order_and_hint_behavior()
// ├── native_main_content_stays_centered_while_resizing()
// ├── native_sidebar_button_reflects_real_collapsed_state()
// └── native_collapsed_sidebar_reveals_at_the_edge_and_avoids_the_titlebar()

#[test]
fn sidebar_and_settings_keep_compact_titlebar_spacing() {
    let sidebar = include_str!("../frontend/styles/layout/sidebar-shell.css");
    let main = include_str!("../frontend/styles/features/appearance.css");

    assert!(
        sidebar.contains(
            ".sidebar-scene { display: flex; flex-direction: column; padding-top: 24px; }"
        )
    );
    assert!(sidebar.contains("padding: 8px 14px 6px"));
    assert!(main.contains("--native-titlebar-offset: 32px"));
    assert!(main.contains("padding: 0 clamp(26px, 7vw, 108px) 96px"));
}

#[test]
fn composer_controls_follow_the_requested_order_and_hint_behavior() {
    let html = include_str!("../frontend/index.html");
    let css = include_str!("../frontend/styles/features/appearance.css");
    let layout_css = include_str!("../frontend/styles/layout/app-shell.css");
    let form_css = include_str!("../frontend/styles/components/forms.css");
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

    assert!(css.contains("border-radius: 8px;\n  background: transparent;"));
    assert!(layout_css.contains("border: 0;\n  appearance: none;"));
    assert!(form_css.contains("flex: 0 1 auto;"));
    assert!(!form_css.contains("flex: 0 1 clamp(110px, 22vw, 220px)"));
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
    let css = include_str!("../frontend/styles/features/appearance.css");
    let split = include_str!("../src/native_split_view.rs");

    assert!(css.contains("--main-content-max-width: 960px"));
    assert!(css.contains("body.native-split-main {\n  width: 100%;\n}"));
    assert!(css.contains(
        "body.native-split-main [data-od-id=\"main-panel\"] {\n  width: 100%;\n  height: 100%;\n  align-items: center;\n}"
    ));
    assert!(
        css.contains(
            "body.native-split-main [data-od-id=\"panel-header\"] { align-self: stretch; }"
        )
    );
    assert!(css.contains(
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
    let css = include_str!("../frontend/styles/features/appearance.css");
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
    // Feature-local hover surfaces may use pointerleave; the sidebar must not
    // own or hide the native overlay itself.
    assert!(!sidebar_frontend.contains("set_native_sidebar_overlay_visible"));
    assert!(html.contains("id=\"nativeSidebarEdgeTrigger\""));
    assert!(html.contains("id=\"nativeGuiInteractionLayer\" aria-hidden=\"true\""));
    assert!(css.contains("#nativeGuiInteractionLayer { display: none; }"));
    assert!(css.contains("z-index: 120;"));
    assert!(css.contains("pointer-events: none;"));
    assert!(css.contains("pointer-events: auto;"));
    assert_eq!(html.matches("id=\"nativeSidebarEdgeTrigger\"").count(), 1);
    let settings_panel = html
        .find("id=\"settingsPanel\"")
        .expect("settings scene root is missing");
    let interaction_layer = html
        .find("id=\"nativeGuiInteractionLayer\"")
        .expect("shared native interaction layer is missing");
    assert!(interaction_layer > settings_panel);
    assert!(css.contains("width: 18px;"));
    assert!(css.contains("body.native-split-main.sidebar-collapsed #nativeSidebarEdgeTrigger"));
    assert!(css.contains("body.native-split-main.sidebar-collapsed:not(.native-fullscreen) [data-od-id=\"panel-header\"]"));
    assert!(css.contains("height: 58px;\n  flex: 0 0 58px;\n  padding-top: 12px;"));
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
    assert!(
        split.contains(
            ".topAnchor()\n            .constraintEqualToAnchor(&split_root.topAnchor())"
        )
    );
}
