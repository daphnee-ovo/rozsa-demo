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
    assert!(architecture.contains("emit_to"));
    assert!(frontend.contains("设置不是另一个 sidebar 容器"));
    assert!(frontend.contains("复用同一 native split"));
}

#[test]
fn validation_separates_product_harness_and_non_macos_results() {
    let validation = include_str!("../../../docs/gui/NATIVE_SPLIT_VALIDATION.md");

    assert!(validation.contains("## 验证方法"));
    assert!(validation.contains("## 本次共享 UI 验证范围"));
    assert!(validation.contains("## 自动化与限制"));
    assert!(validation.contains("Linux / Windows 真实视觉 | pending platform visual verification"));
    assert!(validation.contains("不能把浏览器 fixture 或 source contract 当成原生操作结果"));
    assert!(validation.contains("未重新执行的 fullscreen"));
    assert!(validation.contains("不因共享 UI 验收被宣称为本次实际验证通过"));
}

#[test]
fn implementation_keeps_two_webviews_targeted_routing_and_native_backing() {
    let native = include_str!("../src/native_split_view.rs");
    let router = include_str!("../src/scene_router.rs");
    let events = include_str!("../src/events.rs");
    let frontend = include_str!("../../../desktop/renderer/app.js");

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
