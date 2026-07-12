#[test]
fn appearance_settings_are_in_a_dedicated_tab() {
    let html = include_str!("../frontend/index.html");
    let tauri = include_str!("../tauri.conf.json");

    assert!(html.contains("switchSettingsTab('appearance', this)"));
    assert!(html.contains("id=\"pane-appearance\""));
    assert!(html.contains("class=\"settings-workspace\""));
    assert!(html.contains("class=\"settings-back\""));
    assert!(!html.contains("id=\"settingsSidebarToggleButton\""));
    assert!(!html.contains("class=\"traffic-lights\""));
    assert!(!html.contains("class=\"window-control\""));
    assert!(!html.contains("class=\"app-titlebar\""));
    assert!(!html.contains("class=\"settings-chrome\""));
    assert!(html.contains("data-od-id=\"chat-input\""));
    assert!(html.contains(".input-toolbar .model-selector"));
    assert!(html.contains("@media (max-width: 560px)"));
    assert!(tauri.contains("\"decorations\": true"));
    assert!(tauri.contains("\"hiddenTitle\": true"));
    assert!(tauri.contains("\"titleBarStyle\": \"Overlay\""));
    let native = include_str!("../src/native_titlebar.rs");
    assert!(native.contains("NSTitlebarAccessoryViewController"));
    assert!(native.contains("FullSizeContentView"));
    assert!(native.contains("setTitleVisibility(NSWindowTitleVisibility::Hidden)"));
    assert!(native.contains("toggleSidebar:"));
    assert!(native.contains("rectangle.split.2x1"));
    assert!(native.contains("mouseDown:"));
    assert!(native.contains("performWindowDragWithEvent"));
    assert!(native.contains("NSWindowDidEnterFullScreenNotification"));
    assert!(native.contains("NSWindowDidExitFullScreenNotification"));
    assert!(native.contains("NSWindowDidResizeNotification"));
    assert!(!native.contains("RÓZSA"));
    assert!(html.contains("id=\"settingsThemeMode\""));
    assert!(html.contains("value=\"system\""));
    assert!(html.contains("value=\"light\""));
    assert!(html.contains("value=\"dark\""));
    assert!(html.contains("id=\"settingsFontSizeRange\" type=\"range\" min=\"5\" max=\"50\""));
    assert!(html.contains("id=\"settingsFontSizeInput\" type=\"number\" min=\"5\" max=\"50\""));
    for mode in ["system", "light", "dark"] {
        assert!(
            html.contains(&format!("data-theme-mode-card=\"{mode}\"")),
            "missing theme mode card: {mode}"
        );
    }
    assert!(html.contains("Light Theme"));
    assert!(html.contains("Dark Theme"));
    for label in [
        "Accent",
        "Background",
        "Foreground",
        "UI font",
        "Translucent sidebar",
        "Code font",
    ] {
        assert!(html.contains(label), "missing appearance field: {label}");
    }
    for picker in [
        "lightThemeAccentPicker",
        "lightThemeBackgroundPicker",
        "lightThemeForegroundPicker",
        "darkThemeAccentPicker",
        "darkThemeBackgroundPicker",
        "darkThemeForegroundPicker",
    ] {
        assert!(html.contains(&format!("id=\"{picker}\" type=\"color\"")));
    }
    let light = html
        .split("id=\"appearanceLightSection\"")
        .nth(1)
        .expect("missing light theme section");
    assert!(light.find("Code font").unwrap() < light.find("Translucent sidebar").unwrap());
    let dark = html
        .split("id=\"appearanceDarkSection\"")
        .nth(1)
        .expect("missing dark theme section");
    assert!(dark.find("Code font").unwrap() < dark.find("Translucent sidebar").unwrap());
    assert!(!html.contains("id=\"settingsTheme\""));
    assert!(!html.contains("id=\"settingsFontSize\""));
}

#[test]
fn appearance_is_backend_persisted_and_theme_files_are_loaded() {
    let js = include_str!("../frontend/app.js");
    let rust = include_str!("../src/commands.rs");
    let lib = include_str!("../src/lib.rs");

    assert!(js.contains("invoke('get_settings')"));
    assert!(js.contains("invoke('list_themes')"));
    assert!(js.contains("invoke('get_theme'"));
    assert!(js.contains("invoke('save_theme'"));
    assert!(js.contains("appearance_theme_mode"));
    assert!(js.contains("applyThemeDefinition"));
    assert!(js.contains("selectThemeModeCard"));
    assert!(js.contains("setThemeColorControlValue"));
    assert!(js.contains("toggleMainSidebar"));
    assert!(js.contains("native-sidebar-toggle"));
    assert!(js.contains("native-fullscreen"));
    assert!(js.contains("syncChromeBackgroundGeometry"));
    assert!(!js.contains("startNativeWindowDrag"));
    assert!(js.contains("scheduleNativeFullscreenSync"));
    assert!(js.contains("offsetLeft + element.offsetWidth"));
    assert!(js.contains("document.body.classList.toggle('sidebar-collapsed'"));
    assert!(js.contains("getCurrentWindow"));
    assert!(!js.contains("startDragging"));
    assert!(!js.contains("toggleMaximizeWindow"));
    assert!(js.contains("prefers-color-scheme: dark"));
    assert!(!js.contains("localStorage"));
    assert!(rust.contains("pub fn list_themes"));
    assert!(rust.contains("pub fn get_theme"));
    assert!(rust.contains("pub fn save_theme"));
    assert!(rust.contains("appearance_font_size"));
    assert!(lib.contains("commands::list_themes"));
    assert!(lib.contains("commands::get_theme"));
    assert!(lib.contains("commands::save_theme"));
}
