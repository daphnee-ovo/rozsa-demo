use rozsa_gui::scene_router::{GuiScene, GuiWebview, SceneRouter, SettingsPane};

#[test]
fn commits_intents_serially_and_only_increments_on_changes() {
    let mut router = SceneRouter::default();
    assert_eq!(router.snapshot().revision, 1);

    let settings = router
        .set_scene(GuiScene::Settings, Some(SettingsPane::Appearance), 1)
        .unwrap();
    assert!(settings.changed);
    assert_eq!(settings.snapshot.revision, 2);
    assert_eq!(settings.snapshot.scene, GuiScene::Settings);
    assert_eq!(
        settings.snapshot.selected_pane,
        Some(SettingsPane::Appearance)
    );

    let duplicate = router
        .set_scene(GuiScene::Settings, Some(SettingsPane::Appearance), 2)
        .unwrap();
    assert!(!duplicate.changed);
    assert_eq!(duplicate.snapshot.revision, 2);
}

#[test]
fn stale_expected_revision_returns_the_latest_complete_snapshot() {
    let mut router = SceneRouter::default();
    router
        .set_scene(GuiScene::Settings, Some(SettingsPane::General), 1)
        .unwrap();

    let stale = router.set_scene(GuiScene::Main, None, 1).unwrap();
    assert!(stale.stale);
    assert!(!stale.changed);
    assert_eq!(stale.snapshot, router.snapshot());
    assert_eq!(stale.snapshot.revision, 2);
    assert_eq!(stale.snapshot.selected_pane, Some(SettingsPane::General));
}

#[test]
fn unready_webviews_receive_only_the_latest_snapshot_when_ready() {
    let mut router = SceneRouter::default();
    let main_ready = router.webview_ready(GuiWebview::Main, 0);
    assert!(main_ready.should_emit);
    assert!(!main_ready.all_webviews_ready);

    let first = router
        .set_scene(GuiScene::Settings, Some(SettingsPane::General), 1)
        .unwrap();
    assert_eq!(first.ready_webviews, vec![GuiWebview::Main]);
    let second = router
        .set_scene(GuiScene::Settings, Some(SettingsPane::Tools), 2)
        .unwrap();
    assert_eq!(second.ready_webviews, vec![GuiWebview::Main]);

    let sidebar_ready = router.webview_ready(GuiWebview::Sidebar, 0);
    assert!(sidebar_ready.should_emit);
    assert!(sidebar_ready.all_webviews_ready);
    assert_eq!(sidebar_ready.snapshot.revision, 3);
    assert_eq!(
        sidebar_ready.snapshot.selected_pane,
        Some(SettingsPane::Tools)
    );
}

#[test]
fn snapshots_use_the_exact_ipc_shape() {
    let router = SceneRouter::default();
    let value = serde_json::to_value(router.snapshot()).unwrap();

    assert_eq!(
        value,
        serde_json::json!({
            "revision": 1,
            "scene": "main",
            "selectedPane": null
        })
    );
}

#[test]
fn keyboard_shortcuts_is_a_supported_settings_pane() {
    let mut router = SceneRouter::default();
    let update = router
        .set_scene(GuiScene::Settings, Some(SettingsPane::KeyboardShortcuts), 1)
        .unwrap();
    assert_eq!(
        serde_json::to_value(update.snapshot.selected_pane).unwrap(),
        serde_json::json!("keyboard-shortcuts")
    );
}

#[test]
fn settings_requires_a_selected_pane_and_main_clears_it() {
    let mut router = SceneRouter::default();
    assert!(router.set_scene(GuiScene::Settings, None, 1).is_err());

    router
        .set_scene(GuiScene::Settings, Some(SettingsPane::Skills), 1)
        .unwrap();
    let main = router
        .set_scene(GuiScene::Main, Some(SettingsPane::Appearance), 2)
        .unwrap();
    assert_eq!(main.snapshot.selected_pane, None);
}
