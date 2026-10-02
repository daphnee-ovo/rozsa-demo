// FrameworkTree
// notification_center_test.rs
// ├── upsert_event_serializes_as_tagged_camel_case()
// ├── resolve_event_serializes_with_id_only()
// ├── severities_serialize_lowercase()
// ├── notification_event_name_is_stable()
// ├── notification_layer_is_global_to_the_main_webview()
// ├── stable_ids_deduplicate_toasts()
// ├── toasts_stack_downward_with_independent_six_second_timers()
// ├── errors_collapse_into_unresolved_tray_with_circled_count()
// └── model_config_diagnostics_map_to_shared_notifications_and_resolve()

use rozsa_app::model_registry::{ModelConfigDiagnostic, ModelConfigDiagnosticSeverity};
use rozsa_gui::notifications::{
    APP_NOTIFICATION_EVENT, AppNotificationEvent, NotificationSeverity, model_config_notification,
    reconcile_model_config_notifications,
};
use std::collections::HashSet;
use std::path::PathBuf;

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
    let template = include_str!("../frontend/ui/templates/composites/notification.html");
    let entry = include_str!("../frontend/runtime/entries/main.js");
    let main_start = html.find("id=\"mainContentScene\"").unwrap();
    let settings_start = html.find("id=\"settingsPanel\"").unwrap();
    let center = html.find("data-template-mount=\"notification\"").unwrap();
    assert!(
        center > main_start,
        "notification layer must live outside the Main root"
    );
    assert!(
        center > settings_start,
        "notification layer must live outside the Settings root"
    );
    assert!(template.contains("id=\"notificationCenter\""));
    assert!(template.contains("class=\"notification-center\""));
    assert!(template.contains("aria-live=\"polite\""));
    assert!(template.contains("id=\"notificationStack\""));
    assert!(template.contains("id=\"notificationErrorTray\""));
    assert!(entry.contains("composites/notification.html"));
    assert!(entry.contains("selector: \"[data-template-mount=notification]\""));
}

#[test]
fn stable_ids_deduplicate_toasts() {
    let js = include_str!("../frontend/app.js");
    assert!(js.contains("const notificationToasts = new Map();"));
    let dedup = js
        .find("const existing = notificationToasts.get(id);")
        .unwrap();
    let update = js
        .find("applyNotificationPresentation(existing, severity, title, message);")
        .unwrap();
    assert!(
        dedup < update,
        "existing toast is updated in place instead of duplicated"
    );
    assert!(js.contains("if (!existing.paused)"));
}

#[test]
fn toasts_stack_downward_with_independent_six_second_timers() {
    let js = include_str!("../frontend/app.js");
    let css = include_str!("../frontend/ui/styles/composites/feedback.css");
    assert!(js.contains("const NOTIFICATION_TIMEOUT_MS = 6000;"));
    assert!(js.contains("notificationStack().appendChild(element);"));
    assert!(js.contains("toast.timer = setTimeout(() => expireNotification(id), timeoutMs);"));
    assert!(css.contains("display: grid; gap: 8px;"));
    assert!(js.contains("toast.element.remove();"));
}

#[test]
fn errors_collapse_into_unresolved_tray_with_circled_count() {
    let js = include_str!("../frontend/app.js");
    let template = include_str!("../frontend/ui/templates/composites/notification.html");
    assert!(js.contains("function updateNotificationErrorTray()"));
    assert!(js.contains("notificationErrorCount().textContent = String(count);"));
    assert!(js.contains("notificationErrorTray().hidden = count === 0;"));
    assert!(template.contains("id=\"notificationErrorTrayButton\""));
    assert!(template.contains("id=\"notificationErrorCount\""));
    assert!(template.contains("class=\"notification-error-icon\""));
}

#[test]
fn model_config_diagnostics_map_to_shared_notifications_and_resolve() {
    let error = ModelConfigDiagnostic {
        path: PathBuf::from("/tmp/broken.json"),
        severity: ModelConfigDiagnosticSeverity::Error,
        provider: None,
        reference: None,
        message: "Failed to parse models.json".to_string(),
        hint: "Fix the JSON/schema error".to_string(),
    };
    let warning = ModelConfigDiagnostic {
        path: PathBuf::from("/Users/test/.rozsa/models/deepseek.json"),
        severity: ModelConfigDiagnosticSeverity::Warning,
        provider: Some("deepseek".to_string()),
        reference: Some("$DEEPSEEK_API_KEY".to_string()),
        message: "environment variable could not be resolved".to_string(),
        hint: "Check the variable name".to_string(),
    };

    assert_eq!(
        model_config_notification(&error),
        AppNotificationEvent::Upsert {
            id: error.notification_id(),
            severity: NotificationSeverity::Error,
            title: "Model configuration error".to_string(),
            message:
                "Failed to parse models.json File: /tmp/broken.json Hint: Fix the JSON/schema error"
                    .to_string(),
            timeout_ms: 6_000,
        }
    );

    let mut active_ids = HashSet::new();
    active_ids.insert(error.notification_id());
    let (events, next_ids) = reconcile_model_config_notifications(&active_ids, &[warning.clone()]);
    assert!(events.contains(&AppNotificationEvent::Resolve {
        id: error.notification_id(),
    }));
    assert!(events.contains(&model_config_notification(&warning)));
    assert!(!next_ids.contains(&error.notification_id()));
    assert!(next_ids.contains(&warning.notification_id()));
}
