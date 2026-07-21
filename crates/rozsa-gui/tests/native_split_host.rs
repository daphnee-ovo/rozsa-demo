fn package_version(lock: &str, name: &str, version: &str) -> bool {
    lock.split("[[package]]").any(|package| {
        package.contains(&format!("name = \"{name}\""))
            && package.contains(&format!("version = \"{version}\""))
    })
}

#[test]
fn native_split_dependencies_are_exact_and_verified() {
    let workspace = include_str!("../../../Cargo.toml");
    let gui = include_str!("../Cargo.toml");
    let lock = include_str!("../../../Cargo.lock");

    assert!(workspace.contains("tauri = { version = \"=2.11.5\""));
    assert!(workspace.contains("features = [\"devtools\", \"unstable\"]"));
    for feature in [
        "NSLayoutAnchor",
        "NSSplitView",
        "NSSplitViewController",
        "NSSplitViewItem",
        "NSViewController",
    ] {
        assert!(gui.contains(&format!("\"{feature}\"")));
    }
    assert!(package_version(lock, "tauri", "2.11.5"));
    assert!(package_version(lock, "tauri-runtime-wry", "2.11.4"));
    assert!(package_version(lock, "wry", "0.55.1"));
}

#[test]
fn native_split_host_owns_two_persistent_panes_on_the_main_thread() {
    let source = include_str!("../src/native_split_view.rs");
    let lib = include_str!("../src/lib.rs");
    let commands = include_str!("../src/commands.rs");
    let config = include_str!("../tauri.conf.json");

    assert!(source.contains("thread_local!"));
    assert!(source.contains("MainThreadMarker::new()"));
    assert!(source.contains("NSSplitViewController::new"));
    assert!(source.contains("sidebarWithViewController"));
    assert!(source.contains("splitViewItemWithViewController"));
    assert!(source.contains("addSplitViewItem(&sidebar_item)"));
    assert!(source.contains("addSplitViewItem(&main_item)"));
    assert!(source.contains("NSLayoutConstraint::activateConstraints"));
    assert!(source.contains("setContentViewController(Some(&split_controller))"));
    assert!(source.contains("WebviewBuilder::new(\"sidebar\""));
    assert!(lib.contains("WebviewUrl::App(std::path::PathBuf::from(\"sidebar.html\"))"));
    assert!(!source.contains("set_bounds("));
    assert!(!source.contains("set_size("));
    assert!(!source.contains("set_position("));
    assert!(lib.contains("mod native_split_view;"));
    assert!(lib.contains("native_split_view::install"));
    assert!(config.contains("\"visible\": false"));
    assert!(source.contains("split_controller.view().setHidden(true)"));
    assert!(source.contains("pub fn reveal_content()"));
    assert!(commands.contains("update.all_webviews_ready"));
    assert!(commands.contains("reveal_native_split(&app)?"));
    assert!(commands.contains("failed to show ready GUI window"));
}

#[test]
fn native_split_cleanup_restores_hierarchy_before_closing_child() {
    let source = include_str!("../src/native_split_view.rs");
    let restore = source
        .find("fn restore_hierarchy")
        .expect("missing hierarchy restore");
    let async_close = source
        .find("fn close_sidebar_async")
        .expect("missing asynchronous child close");

    assert!(restore < async_close);
    assert!(source.contains("deactivateConstraints(&self.main_constraints)"));
    assert!(source.contains("removeSplitViewItem(&self.main_item)"));
    assert!(source.contains("setContentViewController(Some(controller))"));
    assert!(source.contains("failed to close child WebView"));
    assert!(source.contains("app.exit(1)"));
    assert!(source.contains("NativeSplitHost::restore_hierarchy"));
}
