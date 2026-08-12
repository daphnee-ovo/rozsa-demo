#[test]
fn sidebar_webview_surface_stays_transparent() {
    let css = include_str!("../frontend/styles/layout/sidebar-shell.css");
    let native = include_str!("../src/native_split_view.rs");

    assert!(css.contains("body { color: var(--fg); background: transparent;"));
    assert!(!css.contains("body { background: var(--sidebar-bg)"));
    assert!(native.contains("WebviewBuilder::new(\"sidebar\", sidebar_url)"));
    assert!(native.contains(".transparent(true)"));
}

#[test]
fn native_host_owns_translucent_and_opaque_backings() {
    let native = include_str!("../src/native_split_view.rs");

    assert!(native.contains("NSVisualEffectMaterial::Sidebar"));
    assert!(native.contains("NSVisualEffectBlendingMode::BehindWindow"));
    assert!(native.contains("NSVisualEffectState::FollowsWindowActiveState"));
    assert!(native.contains("NSBoxType::Custom"));
    assert!(native.contains("sidebar_opaque_backing.setFillColor"));
    assert!(native.contains("if surface.translucent_sidebar"));
    assert!(native.contains("parse_oklch_color"));
    assert!(native.contains("parse_rgb_color"));
    assert!(native.contains("parse_hex_color"));
}

#[test]
fn native_backing_updates_before_the_revision_is_emitted() {
    let events = include_str!("../src/events.rs");
    let start = events.find("pub fn emit_theme_state").unwrap();
    let function = &events[start..];

    let native = function.find("apply_native_theme_surface").unwrap();
    let webviews = function.find("emit_both(app, \"theme-state\"").unwrap();
    assert!(native < webviews);
    assert!(function.contains("THEME_REVISION.fetch_add(1, Ordering::SeqCst) + 1"));
    assert!(function.contains("light_theme"));
    assert!(function.contains("dark_theme"));
}

#[test]
fn both_webviews_discard_old_theme_revisions() {
    let shared = include_str!("../frontend/gui_shared.js");
    let main = include_str!("../frontend/app.js");
    let sidebar = include_str!("../frontend/sidebar.js");

    assert!(shared.contains("revision <= state.revision"));
    assert!(shared.contains("function applyThemeSnapshot"));
    assert!(shared.contains("snapshot.lightTheme"));
    assert!(shared.contains("snapshot.darkTheme"));
    assert!(main.contains("const mainThemeState = { revision: 0 }"));
    assert!(main.contains("applyThemeSnapshot(mainThemeState"));
    assert!(sidebar.contains("const sidebarThemeState = { revision: 0 }"));
    assert!(sidebar.contains("applyThemeSnapshot(sidebarThemeState"));
}

#[test]
fn system_appearance_change_requests_one_new_backend_revision() {
    let main = include_str!("../frontend/app.js");
    let start = main.find("function installSystemThemeListener").unwrap();
    let end = main[start..]
        .find("function applyThemeDefinition")
        .map(|offset| start + offset)
        .unwrap();
    let listener = &main[start..end];

    assert!(listener.contains("prefers-color-scheme: dark"));
    assert!(listener.contains("invoke('get_settings')"));
    assert!(!listener.contains("applySelectedTheme()"));
}
