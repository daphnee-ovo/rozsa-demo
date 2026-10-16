#[test]
fn native_scene_roots_are_precreated_and_switched_by_visibility() {
    let main_html = include_str!("../frontend/index.html");
    let sidebar_html = include_str!("../frontend/sidebar.html");
    let main_js = include_str!("../frontend/app.js");
    let sidebar_js = include_str!("../frontend/sidebar.js");
    let shared = include_str!("../frontend/gui_shared.js");

    assert!(main_html.contains("id=\"mainContentScene\" data-native-scene-root=\"main-content\""));
    assert!(main_html.contains("id=\"settingsPanel\" data-native-scene-root=\"settings-content\""));
    assert!(sidebar_html.contains("id=\"mainSidebarScene\""));
    assert!(sidebar_html.contains("id=\"settingsSidebarScene\""));
    assert!(shared.contains("revision <= state.revision"));
    assert!(shared.contains("root.hidden = !visible"));
    assert!(shared.contains("root.inert = !visible"));

    for source in [main_js, sidebar_js] {
        assert!(source.contains("gui_webview_ready"));
        assert!(source.contains("gui-scene-snapshot"));
        assert!(source.contains("set_gui_scene"));
    }

    let render_start = main_js.find("function renderNativeMainScene").unwrap();
    let render_end = main_js[render_start..]
        .find("async function requestGuiScene")
        .map(|offset| render_start + offset)
        .unwrap();
    let renderer = &main_js[render_start..render_end];
    assert!(!renderer.contains("innerHTML"));
    assert!(!renderer.contains("replaceChildren"));
    assert!(!renderer.contains("remove()"));
    assert!(!renderer.contains("location.reload"));
}

#[test]
fn product_loads_one_persistent_sidebar_webview() {
    let lib = include_str!("../src/lib.rs");
    let split = include_str!("../src/native_split_view.rs");

    assert!(lib.contains("PathBuf::from(\"sidebar.html\")"));
    assert_eq!(split.matches("WebviewBuilder::new(\"sidebar\"").count(), 1);
    assert!(!split.contains("reload("));
}
