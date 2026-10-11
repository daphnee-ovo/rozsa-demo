#[path = "../src/window_visibility.rs"]
mod window_visibility;

use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::{LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl, WindowBuilder};

#[test]
fn ready_window_remains_available_with_two_webviews() {
    let app = mock_builder().build(mock_context(noop_assets())).unwrap();
    let window = WindowBuilder::new(&app, "main")
        .visible(false)
        .build()
        .unwrap();
    for label in ["main", "sidebar"] {
        window
            .add_child(
                WebviewBuilder::new(label, WebviewUrl::default()),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(640.0, 480.0),
            )
            .unwrap();
    }
    assert!(app.get_webview_window("main").is_none());
    let ready_window = window_visibility::main_window(app.handle())
        .expect("both ready WebViews must still resolve their main window");
    assert_eq!(ready_window.label(), "main");
    ready_window.show().unwrap();
}

#[test]
fn missing_main_window_is_not_replaced_by_another_window() {
    let app = mock_builder().build(mock_context(noop_assets())).unwrap();
    WindowBuilder::new(&app, "other").build().unwrap();
    assert!(window_visibility::main_window(app.handle()).is_none());
}
