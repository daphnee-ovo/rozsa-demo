#[test]
fn one_native_action_toggles_the_persistent_split_sidebar() {
    let split = include_str!("../src/native_split_view.rs");
    let titlebar = include_str!("../src/native_titlebar.rs");
    let lib = include_str!("../src/lib.rs");

    assert!(titlebar.contains("setAction(Some(sel!(toggleSidebar:)))"));
    assert!(lib.contains("native_split_view::toggle_sidebar()"));
    assert!(split.contains("host.split_controller.toggleSidebar"));
    assert!(!lib.contains("native-sidebar-toggle"));
}

#[test]
fn divider_constraints_and_restoration_are_appkit_owned() {
    let split = include_str!("../src/native_split_view.rs");

    assert!(split.contains("setMinimumThickness(SIDEBAR_MIN_WIDTH)"));
    assert!(split.contains("setMaximumThickness(SIDEBAR_MAX_WIDTH)"));
    let initial = split
        .find("setPosition_ofDividerAtIndex(SIDEBAR_INITIAL_WIDTH, 0)")
        .unwrap();
    let autosave = split
        .find("setAutosaveName(Some(ns_string!(\"RózsaNativeSidebarSplit\")))")
        .unwrap();
    assert!(
        initial < autosave,
        "explicit initial width must precede autosave restore"
    );
    assert!(!split.contains("set_bounds("));
    assert!(!split.contains("set_size("));
    assert!(!split.contains("set_position("));
}

#[test]
fn titlebar_installs_after_split_and_uses_the_stable_content_root() {
    let split = include_str!("../src/native_split_view.rs");
    let titlebar = include_str!("../src/native_titlebar.rs");
    let lib = include_str!("../src/lib.rs");

    assert!(split.contains("on_installed: Option<Box<dyn FnOnce(usize)"));
    assert!(
        lib.contains("native_split_view::install(&window, sidebar_url, move |main_webview_raw|")
    );
    assert!(lib.contains("native_titlebar::install("));
    assert!(titlebar.contains("ns_window\n        .contentView()"));
    assert!(!titlebar.contains("window.ns_view()"));
    assert!(titlebar.contains("performWindowDragWithEvent"));
    assert!(titlebar.contains("event.clickCount() == 2"));
    assert!(titlebar.contains("performZoom"));
}

#[test]
fn inspector_detaches_from_the_frontend_loaded_delegate() {
    let inspector = include_str!("../src/inspector.rs");
    let lib = include_str!("../src/lib.rs");

    let split_install = lib
        .find("native_split_view::install(&window, sidebar_url")
        .expect("native split installation is missing");
    let titlebar_install = lib
        .find("native_titlebar::install(")
        .expect("native titlebar installation is missing");
    let inspector_open = lib
        .find("inspector::open_from_webview_raw(main_webview_raw)")
        .expect("detached Inspector launch is missing");
    assert!(split_install < titlebar_install);
    assert!(titlebar_install < inspector_open);

    let delegate = inspector.find("setDelegate: delegate_object").unwrap();
    let connect = inspector.find("msg_send![&inspector, connect]").unwrap();
    let show = inspector.find("msg_send![&inspector, show]").unwrap();
    assert!(delegate < connect);
    assert!(connect < show);
    assert!(inspector.contains("method(inspectorFrontendLoaded:)"));
    assert!(inspector.contains("msg_send![inspector, detach]"));
    assert!(inspector.contains("_delegate: Retained<InspectorDelegate>"));
    assert!(inspector.contains("respondsToSelector: selector"));
    assert!(!inspector.contains("isAttached"));
    assert!(!inspector.contains("std::thread"));
    assert!(!inspector.contains("run_on_main_thread"));
}

#[test]
fn close_releases_titlebar_observers_and_actions_before_split_teardown() {
    let titlebar = include_str!("../src/native_titlebar.rs");
    let lib = include_str!("../src/lib.rs");

    assert!(titlebar.contains("removeObserver(self.fullscreen_observer"));
    assert!(titlebar.contains("self.sidebar_button.setAction(None)"));
    assert!(titlebar.contains("self.sidebar_button.setTarget(None)"));
    assert!(titlebar.contains("self.drag_view.removeFromSuperview()"));
    let titlebar_teardown = lib.find("native_titlebar::teardown()").unwrap();
    let inspector_teardown = lib.find("inspector::teardown()").unwrap();
    let split_teardown = lib.find("native_split_view::teardown()").unwrap();
    let deny = lib.find("deny_pending_approvals(approvals, None)").unwrap();
    assert!(inspector_teardown < titlebar_teardown);
    assert!(titlebar_teardown < split_teardown);
    assert!(split_teardown < deny);
}

#[test]
fn native_frontend_does_not_run_the_css_width_threshold() {
    let app = include_str!("../frontend/app.js");
    let start = app.find("function syncMainSidebarViewport()").unwrap();
    let end = app[start..]
        .find("function sidebarChromeBoundary")
        .map(|offset| start + offset)
        .unwrap();
    let viewport = &app[start..end];

    assert!(viewport.contains("if (nativeSplitMode) return;"));
    assert!(viewport.contains("window.innerWidth <= 1100"));
}
