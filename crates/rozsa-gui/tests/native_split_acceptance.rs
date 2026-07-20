#[test]
fn gui_docs_describe_the_current_native_split_architecture() {
    let architecture = include_str!("../../../docs/gui/ARCHITECTURE.md");
    let terminology = include_str!("../../../docs/gui/TERMINOLOGY.md");
    let frontend = include_str!("../../../docs/gui/FRONTEND_TERMINOLOGY.md");

    assert!(architecture.contains("NativeSplitHost"));
    assert!(architecture.contains("NSSplitViewController"));
    assert!(terminology.contains("NativeSplitHost"));
    assert!(terminology.contains("NSSplitViewController"));
    for document in [architecture, terminology] {
        assert!(document.contains("native_split_view.rs"));
        assert!(document.contains("native_titlebar.rs"));
        assert!(document.contains("scene_router.rs"));
    }
    assert!(architecture.contains("两个持久 WebView"));
    assert!(architecture.contains("main/sidebar targeted event"));
    assert!(architecture.contains("emit_to(\"main\""));
    assert!(frontend.contains("设置不是另一个 sidebar 容器"));
    assert!(frontend.contains("复用同一 native split"));
}

#[test]
fn validation_separates_product_harness_and_non_macos_results() {
    let validation = include_str!("../../../docs/gui/NATIVE_SPLIT_VALIDATION.md");

    assert!(validation.contains("## Product app acceptance (2026-07-14)"));
    assert!(validation.contains("## Foreground observations"));
    assert!(validation.contains("## Automated acceptance"));
    assert!(validation.contains("Real non-macOS GUI | UNVERIFIED"));
    assert!(validation.contains("Harness or automated evidence"));
    assert!(validation.contains("Translucent Dark sidebar | PASS"));
    assert!(validation.contains("Rows marked `UNVERIFIED`"));
}

#[test]
fn implementation_keeps_two_webviews_targeted_routing_and_native_backing() {
    let native = include_str!("../src/native_split_view.rs");
    let router = include_str!("../src/scene_router.rs");
    let events = include_str!("../src/events.rs");
    let frontend = include_str!("../frontend/app.js");

    assert!(native.contains("WebviewBuilder::new(\"sidebar\""));
    assert!(native.contains(".transparent(true)"));
    assert!(native.contains("NSSplitViewController::new"));
    assert!(native.contains("setAutosaveName(Some(ns_string!(\"RózsaNativeSidebarSplit\")))"));
    assert!(native.contains("NSAppearanceNameDarkAqua"));
    assert!(router.contains("pub enum GuiScene"));
    assert!(router.contains("pub enum GuiWebview"));
    assert!(events.contains("emit_to("));
    assert!(frontend.contains("nativeSplitMode"));
}
