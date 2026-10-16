// FrameworkTree
// notification_position_test.rs
// └── notifications_have_one_global_host_and_expand_down_from_the_top_right()

#[test]
fn notifications_have_one_global_host_and_expand_down_from_the_top_right() {
    let sidebar = include_str!("../../../desktop/renderer/sidebar.html");
    let entry = include_str!("../../../desktop/renderer/runtime/entries/sidebar.js");
    let css = include_str!("../../../desktop/renderer/styles/composites/feedback.css");
    assert!(!sidebar.contains("data-template-mount=\"notification\""));
    assert!(!entry.contains("composites/notification.html"));
    let center = css
        .split(".notification-center {")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    assert!(center.contains("top: calc(var(--native-titlebar-offset) + 18px)"));
    assert!(center.contains("right: 20px"));
    let list = css
        .split(".notification-error-list {")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    assert!(list.contains("top: calc(100% + 8px)"));
    assert!(!list.contains("bottom:"));
}
