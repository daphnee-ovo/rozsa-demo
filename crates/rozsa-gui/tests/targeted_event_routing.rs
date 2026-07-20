#[test]
fn runtime_events_have_one_explicit_webview_target() {
    let events = include_str!("../src/events.rs");
    let commands = include_str!("../src/commands.rs");
    let lib = include_str!("../src/lib.rs");

    assert!(events.contains("emit_main(&app, \"ui-state\""));
    assert!(events.contains("emit_main(\n                                &app,\n                                \"tool-event\""));
    assert!(events.contains("emit_main(&app, \"permission-request\""));
    assert!(events.contains("emit_sidebar(app, \"sidebar-state\", snapshot)"));
    assert!(commands.contains("publish_theme_state(&app, &snapshot.appearance)"));
    assert!(commands.contains("publish_theme_state(app, &appearance)"));
    assert!(events.contains("emit_both(app, \"theme-state\", snapshot.clone())"));
    assert!(events.contains("apply_native_theme_surface(app, revision, &snapshot)"));
    assert!(!events.contains("app.emit("));
    assert!(!commands.contains("app.emit("));

    assert!(lib.contains("commands::gui_webview_ready"));
    assert!(commands.contains("GuiWebview::Sidebar"));
    assert!(commands.contains("emit_sidebar_state(&app, state.inner()).await?"));
}

#[test]
fn sidebar_snapshot_is_complete_and_has_lifecycle_triggers() {
    let state = include_str!("../src/state.rs");
    let commands = include_str!("../src/commands.rs");
    let events = include_str!("../src/events.rs");

    for field in [
        "pub sessions: Vec<SidebarSessionSnapshot>",
        "pub active_session_id: Option<String>",
        "pub git: Option<GitStatus>",
        "pub quota: Option<rozsa_model::rate_limit::RateLimitSnapshot>",
        "pub actions: SidebarActionsSnapshot",
        "pub activity: String",
    ] {
        assert!(
            state.contains(field),
            "missing sidebar snapshot field: {field}"
        );
    }
    for trigger in [
        "pub async fn switch_session",
        "pub async fn new_session",
        "pub async fn rename_session",
        "pub async fn delete_session",
        "async fn refresh_rate_limits",
    ] {
        let start = commands.find(trigger).unwrap();
        let tail = &commands[start..commands.len().min(start + 2600)];
        assert!(
            tail.contains("emit_sidebar_state"),
            "missing trigger: {trigger}"
        );
    }
    assert!(events.contains("AgentEvent::AgentStart"));
    assert!(events.contains("AgentEvent::AgentEnd { .. }"));
    assert!(events.contains("AgentEvent::ToolExecutionEnd { .. }"));
}

#[test]
fn webviews_do_not_duplicate_main_only_or_sidebar_only_consumers() {
    let main = include_str!("../frontend/app.js");
    let sidebar = include_str!("../frontend/sidebar.js");

    for event in ["ui-state", "tool-event", "permission-request"] {
        assert!(main.contains(&format!("listen('{event}'")));
        assert!(!sidebar.contains(&format!("sidebarListen('{event}'")));
    }
    assert!(sidebar.contains("sidebarListen('sidebar-state'"));
    assert!(!main.contains("listen('sidebar-state'"));
    assert!(main.contains("listen('gui-scene-snapshot'"));
    assert!(sidebar.contains("sidebarListen('gui-scene-snapshot'"));
    assert!(main.contains("listen('theme-state'"));
    assert!(sidebar.contains("sidebarListen('theme-state'"));
}

#[test]
fn scene_snapshots_remain_targeted_to_ready_webviews() {
    let router = include_str!("../src/scene_router.rs");
    let commands = include_str!("../src/commands.rs");
    let events = include_str!("../src/events.rs");

    assert!(router.contains("ready_webviews: BTreeSet<GuiWebview>"));
    assert!(router.contains("last_revision < self.snapshot.revision"));
    assert!(commands.contains("emit_gui_scene_snapshot(&app, &targets"));
    assert!(commands.contains("emit_gui_scene_snapshot(&app, &[webview]"));
    assert!(events.contains("app.emit_to("));
}
