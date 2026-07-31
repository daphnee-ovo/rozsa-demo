// FrameworkTree
// notification_center_test.rs
// ├── upsert_event_serializes_as_tagged_camel_case()
// ├── resolve_event_serializes_with_id_only()
// ├── severities_serialize_lowercase()
// ├── notification_event_name_is_stable()
// ├── notification_layer_is_global_to_the_main_webview()
// ├── structured_and_legacy_notifications_are_both_supported()
// ├── stable_ids_deduplicate_toasts()
// ├── toasts_stack_downward_with_independent_six_second_timers()
// ├── hover_pauses_only_that_toasts_timer()
// ├── errors_collapse_into_unresolved_tray_with_circled_count()
// ├── closing_a_toast_does_not_resolve_the_condition()
// ├── error_list_supports_hover_focus_pin_and_escape()
// └── notification_accessibility_does_not_rely_on_color_alone()

use rozsa_gui::notifications::{
    APP_NOTIFICATION_EVENT, AppNotificationEvent, NotificationSeverity,
};

#[test]
fn upsert_event_serializes_as_tagged_camel_case() {
    let event = AppNotificationEvent::Upsert {
        id: "dev-flow.connection:abc".to_owned(),
        severity: NotificationSeverity::Error,
        title: "Connection lost".to_owned(),
        message: "SSE stalled".to_owned(),
        timeout_ms: 6000,
    };
    let value = serde_json::to_value(&event).unwrap();
    assert_eq!(value["type"], "upsert");
    assert_eq!(value["id"], "dev-flow.connection:abc");
    assert_eq!(value["severity"], "error");
    assert_eq!(value["title"], "Connection lost");
    assert_eq!(value["message"], "SSE stalled");
    assert_eq!(value["timeoutMs"], 6000);
}

#[test]
fn resolve_event_serializes_with_id_only() {
    let value = serde_json::to_value(AppNotificationEvent::Resolve {
        id: "dev-flow.cli".to_owned(),
    })
    .unwrap();
    assert_eq!(value["type"], "resolve");
    assert_eq!(value["id"], "dev-flow.cli");
}

#[test]
fn severities_serialize_lowercase() {
    for (severity, expected) in [
        (NotificationSeverity::Info, "info"),
        (NotificationSeverity::Success, "success"),
        (NotificationSeverity::Warning, "warning"),
        (NotificationSeverity::Error, "error"),
    ] {
        assert_eq!(
            serde_json::to_value(severity).unwrap(),
            serde_json::json!(expected)
        );
    }
}

#[test]
fn notification_event_name_is_stable() {
    assert_eq!(APP_NOTIFICATION_EVENT, "app-notification");
}

#[test]
fn notification_layer_is_global_to_the_main_webview() {
    let html = include_str!("../frontend/index.html");
    let main_start = html.find("id=\"mainContentScene\"").unwrap();
    let settings_start = html.find("id=\"settingsPanel\"").unwrap();
    let center = html.find("id=\"notificationCenter\"").unwrap();
    assert!(
        center > main_start,
        "notification layer must live outside the Main root"
    );
    assert!(
        center > settings_start,
        "notification layer must live outside the Settings root"
    );
    assert!(html.contains("class=\"notification-center\""));
    assert!(html.contains("aria-live=\"polite\""));
    assert!(html.contains("position: fixed;"));
    assert!(html.contains("top: 12px;"));
    assert!(html.contains("right: 12px;"));
    assert!(html.contains("z-index: 200;"));
}

#[test]
fn structured_and_legacy_notifications_are_both_supported() {
    let js = include_str!("../frontend/app.js");
    assert!(js.contains("await listen('app-notification'"));
    assert!(js.contains("payload.type === 'upsert'"));
    assert!(js.contains("payload.type === 'resolve'"));
    assert!(js.contains("function upsertNotification(payload)"));
    assert!(js.contains("function resolveNotification(id)"));
    assert!(js.contains("await listen('notification'"));
    assert!(js.contains("function showNotification(message)"));
    assert!(js.contains("'legacy-' + legacyNotificationCounter"));
    assert!(js.contains("severity: 'info'"));
    assert!(js.contains("timeoutMs: NOTIFICATION_TIMEOUT_MS"));
}

#[test]
fn stable_ids_deduplicate_toasts() {
    let js = include_str!("../frontend/app.js");
    assert!(js.contains("const notificationToasts = new Map();"));
    let dedup = js
        .find("const existing = notificationToasts.get(id);")
        .unwrap();
    let update = js
        .find("existing.element.querySelector('.notification-title').textContent = title;")
        .unwrap();
    assert!(
        dedup < update,
        "existing toast is updated in place instead of duplicated"
    );
    assert!(js.contains("existing.timer = setTimeout(() => expireNotification(id), timeoutMs);"));
}

#[test]
fn toasts_stack_downward_with_independent_six_second_timers() {
    let js = include_str!("../frontend/app.js");
    let html = include_str!("../frontend/index.html");
    assert!(js.contains("const NOTIFICATION_TIMEOUT_MS = 6000;"));
    assert!(js.contains("notificationStack().appendChild(element);"));
    assert!(js.contains("toast.timer = setTimeout(() => expireNotification(id), timeoutMs);"));
    assert!(html.contains("flex-direction: column;"));
    assert!(js.contains("toast.element.remove();"));
}

#[test]
fn hover_pauses_only_that_toasts_timer() {
    let js = include_str!("../frontend/app.js");
    assert!(js.contains("element.addEventListener('pointerenter', () => pauseToastTimer(id));"));
    assert!(js.contains("element.addEventListener('pointerleave', () => resumeToastTimer(id));"));
    assert!(js.contains("function pauseToastTimer(id)"));
    assert!(js.contains("clearTimeout(toast.timer);"));
    assert!(js.contains("toast.remainingMs = Math.max(0, toast.expiresAt - performance.now());"));
    assert!(js.contains("function resumeToastTimer(id)"));
    assert!(js.contains("toast.expiresAt = performance.now() + toast.remainingMs;"));
}

#[test]
fn errors_collapse_into_unresolved_tray_with_circled_count() {
    let js = include_str!("../frontend/app.js");
    let html = include_str!("../frontend/index.html");
    assert!(js.contains("if (toast.severity === 'error') {"));
    assert!(
        js.contains("unresolvedErrors.set(id, { title: toast.title, message: toast.message });")
    );
    assert!(js.contains("function updateNotificationErrorTray()"));
    assert!(js.contains("notificationErrorCount().textContent = String(count);"));
    assert!(js.contains("notificationErrorTray().hidden = count === 0;"));
    assert!(html.contains("id=\"notificationErrorTrayButton\""));
    assert!(html.contains("id=\"notificationErrorCount\""));
    assert!(html.contains("class=\"notification-error-icon\""));
    assert!(html.contains("border-radius: 50%;"));
}

#[test]
fn closing_a_toast_does_not_resolve_the_condition() {
    let js = include_str!("../frontend/app.js");
    let dismiss = js.find("function dismissToast(id)").unwrap();
    let resolve = js.find("function resolveNotification(id)").unwrap();
    let dismiss_body = &js[dismiss..resolve];
    assert!(
        dismiss_body.contains("unresolvedErrors.set(id,"),
        "dismissing an error toast keeps the condition unresolved"
    );
    assert!(
        !dismiss_body.contains("unresolvedErrors.delete(id)"),
        "only Resolve may reduce the unresolved count"
    );
    let resolve_body = &js[resolve..];
    assert!(resolve_body.contains("unresolvedErrors.delete(id);"));
    assert!(js.contains("'click', () => dismissToast(id)"));
}

#[test]
fn error_list_supports_hover_focus_pin_and_escape() {
    let js = include_str!("../frontend/app.js");
    let html = include_str!("../frontend/index.html");
    assert!(js.contains("tray.addEventListener('pointerenter'"));
    assert!(js.contains("tray.addEventListener('pointerleave'"));
    assert!(js.contains("if (!notificationErrorListPinned) closeNotificationErrorList();"));
    assert!(js.contains("button.addEventListener('click'"));
    assert!(js.contains("notificationErrorListPinned = true;"));
    assert!(js.contains("button.addEventListener('focusin'"));
    assert!(js.contains("button.addEventListener('focusout'"));
    assert!(js.contains("function openNotificationErrorList()"));
    assert!(js.contains("function closeNotificationErrorList()"));
    assert!(html.contains("aria-expanded=\"false\""));
    assert!(html.contains("aria-label=\"Unresolved errors\""));
    assert!(html.contains("role=\"list\""));
    let keydown_start = js.find("document.addEventListener('keydown'").unwrap();
    let keydown = &js[keydown_start..];
    let notification_escape = keydown.find("!notificationErrorList().hidden").unwrap();
    let double_escape = keydown.find("if (isDoubleEscape)").unwrap();
    assert!(
        notification_escape < double_escape,
        "Escape closes the notification list before streaming escape handling"
    );
    assert!(keydown.contains("closeNotificationErrorList();"));
}

#[test]
fn notification_accessibility_does_not_rely_on_color_alone() {
    let js = include_str!("../frontend/app.js");
    let html = include_str!("../frontend/index.html");
    assert!(html.contains("aria-hidden=\"true\""));
    assert!(js.contains("notificationIcon(severity)"));
    assert!(js.contains("setAttribute('role', severity === 'error' ? 'alert' : 'status')"));
    assert!(html.contains(".notification-error { border-color: var(--error); }"));
    assert!(js.contains("aria-label=\"Dismiss notification\""));
}
