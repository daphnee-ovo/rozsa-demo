// FrameworkTree
// notifications.rs
// ├── enum NotificationSeverity
// ├── enum AppNotificationEvent
// └── emit_notification()

//! Reusable in-app notification events for the main WebView.
//!
//! The notification layer is a main-WebView component outside the Main and
//! Settings scene roots. Backend code emits structured `AppNotificationEvent`
//! payloads; legacy string `notification` events remain supported and are
//! mapped to nonpersistent info toasts by the frontend.

use serde::Serialize;
use tauri::{AppHandle, Runtime};

use crate::events::emit_main;

pub const APP_NOTIFICATION_EVENT: &str = "app-notification";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationSeverity {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AppNotificationEvent {
    #[serde(rename_all = "camelCase")]
    Upsert {
        id: String,
        severity: NotificationSeverity,
        title: String,
        message: String,
        timeout_ms: u64,
    },
    Resolve {
        id: String,
    },
}

pub fn emit_notification<R: Runtime>(
    app: &AppHandle<R>,
    event: AppNotificationEvent,
) -> Result<(), String> {
    emit_main(app, APP_NOTIFICATION_EVENT, event)
}
